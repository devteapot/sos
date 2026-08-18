import process from "node:process";
import {
  InMemoryCredentialStore,
  contentText,
  type AuthEvent,
  type AuthPrompt,
  type Credential,
} from "@earendil-works/pi-ai";
import { registerBunOAuthFlows } from "@earendil-works/pi-ai/bun-oauth";
import type { AgentMessage } from "@earendil-works/pi-agent-core";
import type { AuthoringBackend } from "./authoring.js";
import {
  createAgentRuntime,
  createFauxAgentRuntime,
  createProviderModels,
  type SupportedProvider,
} from "./runtime.js";
import { readSystemPrompt, type PromptDocuments } from "./prompt-policy.js";
import {
  isBoundedPrompt,
  isBoundedSource,
  MAX_SOURCE_BYTES,
  MAX_STDIO_REQUEST_BYTES,
} from "./contract.js";

const MAX_SUMMARY_BYTES = 2048;
export const PINNED_OPENROUTER_MODEL = "deepseek/deepseek-v4-flash-0731";

export interface SanitizedRunnerFailure {
  type: "error";
  stage: "request" | "credential" | "provider" | "protocol" | "validation";
  category:
    | "invalid_request"
    | "credential_rejected"
    | "provider_rejected"
    | "rate_limited"
    | "provider_unavailable"
    | "provider_error"
    | "tool_sequence"
    | "invalid_candidate"
    | "protocol_error";
  error: string;
  model?: string | undefined;
  status?: number;
}

class RunnerFailure extends Error {
  constructor(readonly failure: SanitizedRunnerFailure) {
    super(failure.error);
  }
}

function numericHttpStatus(error: unknown): number | undefined {
  if (!error || typeof error !== "object") return undefined;
  const candidate = error as Record<string, unknown>;
  for (const value of [candidate.status, candidate.statusCode]) {
    if (typeof value === "number" && Number.isInteger(value) && value >= 100 && value <= 599) {
      return value;
    }
  }
  const response = candidate.response;
  if (response && typeof response === "object") {
    const value = (response as Record<string, unknown>).status;
    if (typeof value === "number" && Number.isInteger(value) && value >= 100 && value <= 599) {
      return value;
    }
  }
  return undefined;
}

export function sanitizeRunnerFailure(
  error: unknown,
  model?: string,
): SanitizedRunnerFailure {
  if (error instanceof RunnerFailure) return error.failure;
  const knownMessage = error instanceof Error ? error.message : "";
  if (
    knownMessage === "invalid Pi runner request" ||
    knownMessage === "request is too large" ||
    error instanceof SyntaxError
  ) {
    return {
      type: "error",
      stage: "request",
      category: "invalid_request",
      error: "The Pi runner request was invalid.",
      model,
    };
  }
  const status = numericHttpStatus(error);
  if (status === 401 || status === 403) {
    return {
      type: "error",
      stage: "credential",
      category: "credential_rejected",
      error: "The provider rejected the configured credential.",
      model,
      status,
    };
  }
  if (status === 429) {
    return {
      type: "error",
      stage: "provider",
      category: "rate_limited",
      error: "The provider rate-limited this request.",
      model,
      status,
    };
  }
  if (status !== undefined && status >= 500) {
    return {
      type: "error",
      stage: "provider",
      category: "provider_unavailable",
      error: "The provider is temporarily unavailable.",
      model,
      status,
    };
  }
  if (status !== undefined) {
    return {
      type: "error",
      stage: "provider",
      category: "provider_rejected",
      error: "The provider rejected this request.",
      model,
      status,
    };
  }
  return {
    type: "error",
    stage: "provider",
    category: "provider_error",
    error: "The provider request failed.",
    model,
  };
}

function fail(failure: Omit<SanitizedRunnerFailure, "type">): never {
  throw new RunnerFailure({ type: "error", ...failure });
}

export function promptResponseModel(
  request: { provider: "faux" } | { provider: SupportedProvider; model: string },
): string {
  return request.provider === "faux" ? "faux" : request.model;
}

interface CatalogRequest {
  action: "catalog";
}

interface SelfTestRequest {
  action: "self_test";
}

interface LoginRequest {
  action: "login";
  provider: "openai-codex";
}

interface LivePromptRequest {
  action: "prompt";
  provider: SupportedProvider;
  model: string;
  credential: Credential;
  prompt: string;
  currentSource: string;
}

interface FauxPromptRequest {
  action: "prompt";
  provider: "faux";
  prompt: string;
  currentSource: string;
  candidateSource: string;
}

type PromptRequest = LivePromptRequest | FauxPromptRequest;
type RunnerRequest = CatalogRequest | SelfTestRequest | LoginRequest | PromptRequest;

function send(value: unknown): void {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

function isCredential(value: unknown): value is Credential {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Record<string, unknown>;
  if (candidate.type === "api_key") return typeof candidate.key === "string";
  return (
    candidate.type === "oauth" &&
    typeof candidate.access === "string" &&
    typeof candidate.refresh === "string" &&
    typeof candidate.expires === "number"
  );
}

function isSupportedProvider(value: unknown): value is SupportedProvider {
  return (
    value === "openai" ||
    value === "anthropic" ||
    value === "openai-codex" ||
    value === "openrouter"
  );
}

export function decodeRequest(raw: string): RunnerRequest {
  if (Buffer.byteLength(raw) > MAX_STDIO_REQUEST_BYTES) throw new Error("request is too large");
  const decoded = JSON.parse(raw) as Record<string, unknown>;
  if (decoded.action === "catalog") return { action: "catalog" };
  if (decoded.action === "self_test") return { action: "self_test" };
  if (decoded.action === "login" && decoded.provider === "openai-codex") {
    return { action: "login", provider: decoded.provider };
  }
  if (
    decoded.action === "prompt" &&
    decoded.provider === "faux" &&
    isBoundedPrompt(decoded.prompt) &&
    isBoundedSource(decoded.currentSource) &&
    isBoundedSource(decoded.candidateSource)
  ) {
    return {
      action: "prompt",
      provider: "faux",
      prompt: decoded.prompt,
      currentSource: decoded.currentSource,
      candidateSource: decoded.candidateSource,
    };
  }
  if (
    decoded.action !== "prompt" ||
    !isSupportedProvider(decoded.provider) ||
    typeof decoded.model !== "string" ||
    !decoded.model ||
    (decoded.provider === "openrouter" && decoded.model !== PINNED_OPENROUTER_MODEL) ||
    !isCredential(decoded.credential) ||
    !isBoundedPrompt(decoded.prompt) ||
    !isBoundedSource(decoded.currentSource)
  ) {
    throw new Error("invalid Pi runner request");
  }
  return {
    action: "prompt",
    provider: decoded.provider,
    model: decoded.model,
    credential: decoded.credential,
    prompt: decoded.prompt,
    currentSource: decoded.currentSource,
  };
}

async function readRequest(): Promise<RunnerRequest> {
  process.stdin.setEncoding("utf8");
  let raw = "";
  for await (const chunk of process.stdin) {
    raw += chunk;
    if (Buffer.byteLength(raw) > MAX_STDIO_REQUEST_BYTES) throw new Error("request is too large");
  }
  return decodeRequest(raw);
}

async function catalog(): Promise<void> {
  const defaults: Record<SupportedProvider, string> = {
    openai: "gpt-5.6-luna",
    anthropic: "claude-sonnet-4-6",
    "openai-codex": "gpt-5.6-sol",
    openrouter: PINNED_OPENROUTER_MODEL,
  };
  const providers = Object.entries(defaults).map(([provider, model]) => ({
    provider,
    model,
    available: Boolean(createProviderModels(provider as SupportedProvider).getModel(provider, model)),
  }));
  send({
    type: "catalog",
    node: process.version,
    platform: process.platform,
    arch: process.arch,
    providers,
  });
}

async function selfTest(): Promise<void> {
  const actions: string[] = [];
  const candidate = "return { api_version = 3, render = function() return { id = 'root' } end }";
  const backend: AuthoringBackend = {
    async request(request) {
      actions.push(request.action);
      if (request.action === "get_experience_context") {
        return { revision_id: "a".repeat(64), source: "active", schema_version: 3 };
      }
      if (request.action === "validate_experience") return { valid: true, schema_version: 3 };
      return { activated: true, revision_id: "b".repeat(64) };
    },
  };
  const agent = createFauxAgentRuntime(backend, "SOS Pi runner self-test", candidate);
  await agent.prompt("Run the bounded SOS Pi self-test");
  const expected = ["get_experience_context", "validate_experience", "submit_experience"];
  if (JSON.stringify(actions) !== JSON.stringify(expected)) {
    throw new Error("SOS Pi self-test used an unexpected tool sequence");
  }
  send({
    type: "self_test_complete",
    node: process.version,
    platform: process.platform,
    arch: process.arch,
    actions,
  });
}

function emitAuthEvent(event: AuthEvent): void {
  send({ type: "auth_event", event });
}

async function login(request: LoginRequest): Promise<void> {
  const credentials = new InMemoryCredentialStore();
  const models = createProviderModels(request.provider, credentials);
  await models.login(request.provider, "oauth", {
    async prompt(prompt: AuthPrompt): Promise<string> {
      if (prompt.type === "select") {
        const deviceCode = prompt.options.find((candidate) => candidate.id === "device_code");
        if (!deviceCode) throw new Error("Codex device-code login is unavailable");
        return deviceCode.id;
      }
      throw new Error("Codex device-code login requested unexpected user input");
    },
    notify: emitAuthEvent,
  });
  const credential = await credentials.read(request.provider);
  if (!credential || credential.type !== "oauth") {
    throw new Error("Codex login completed without an OAuth credential");
  }
  send({ type: "login_complete", provider: request.provider, credential });
}

function lastAssistantSummary(messages: AgentMessage[]): string {
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    if (message?.role !== "assistant") continue;
    const text = contentText(message.content).trim();
    if (!text) continue;
    return Buffer.from(text).subarray(0, MAX_SUMMARY_BYTES).toString("utf8");
  }
  return "Pi proposed a complete replacement experience for trusted validation.";
}

async function prompt(request: PromptRequest, systemPrompt: string): Promise<void> {
  const credentials = new InMemoryCredentialStore();
  const live = request.provider !== "faux";
  if (live) {
    try {
      await credentials.modify(request.provider, async () => request.credential);
    } catch {
      fail({
        stage: "credential",
        category: "protocol_error",
        error: "The credential could not be prepared for the provider.",
        model: request.model,
      });
    }
  }
  let validated: string | undefined;
  let submitted: string | undefined;
  const actions: string[] = [];
  const backend: AuthoringBackend = {
    async request(action) {
      if (action.action === "get_experience_context") {
        if (actions.length !== 0) {
          fail({
            stage: "protocol",
            category: "tool_sequence",
            error: "Pi used an invalid authoring tool sequence.",
            model: live ? request.model : "faux",
          });
        }
        actions.push(action.action);
        return {
          revision_id: "0".repeat(64),
          source: request.currentSource,
          schema_version: 3,
        };
      }
      if (Buffer.byteLength(action.source) > MAX_SOURCE_BYTES) {
        fail({
          stage: "validation",
          category: "invalid_candidate",
          error: "Pi proposed a candidate outside the bounded source size.",
          model: live ? request.model : "faux",
        });
      }
      if (action.action === "validate_experience") {
        if (actions.length !== 1 || actions[0] !== "get_experience_context") {
          fail({
            stage: "protocol",
            category: "tool_sequence",
            error: "Pi used an invalid authoring tool sequence.",
            model: live ? request.model : "faux",
          });
        }
        actions.push(action.action);
        validated = action.source;
        return {
          valid: true,
          pending_trusted_host_validation: true,
          schema_version: 3,
        };
      }
      if (actions.length !== 2 || actions[1] !== "validate_experience") {
        fail({
          stage: "protocol",
          category: "tool_sequence",
          error: "Pi used an invalid authoring tool sequence.",
          model: live ? request.model : "faux",
        });
      }
      if (validated !== action.source) {
        fail({
          stage: "validation",
          category: "invalid_candidate",
          error: "Pi submitted a candidate different from the validated source.",
          model: live ? request.model : "faux",
        });
      }
      actions.push(action.action);
      submitted = action.source;
      return { accepted: true, activated: false, pending_trusted_host_validation: true };
    },
  };
  const agent = live
    ? createAgentRuntime({
        backend,
        systemPrompt,
        provider: request.provider,
        model: request.model,
        credentials,
      })
    : createFauxAgentRuntime(
        backend,
        systemPrompt,
        request.candidateSource,
        "The candidate experience is staged for trusted host validation.",
      );
  try {
    await agent.prompt(request.prompt);
  } catch (error) {
    if (error instanceof RunnerFailure) throw error;
    throw new RunnerFailure(sanitizeRunnerFailure(error, live ? request.model : "faux"));
  }
  if (!submitted) {
    fail({
      stage: "protocol",
      category: "tool_sequence",
      error: "Pi completed without submitting a candidate experience.",
      model: live ? request.model : "faux",
    });
  }
  const expectedActions = ["get_experience_context", "validate_experience", "submit_experience"];
  if (JSON.stringify(actions) !== JSON.stringify(expectedActions)) {
    fail({
      stage: "protocol",
      category: "tool_sequence",
      error: "Pi completed without the bounded authoring tool sequence.",
      model: live ? request.model : "faux",
    });
  }
  const refreshed = live ? await credentials.read(request.provider) : undefined;
  if (live && !refreshed) {
    fail({
      stage: "credential",
      category: "protocol_error",
      error: "Pi completed without a refreshed provider credential.",
      model: request.model,
    });
  }
  send({
    type: "prompt_complete",
    provider: request.provider,
    model: promptResponseModel(request),
    source: submitted,
    summary: lastAssistantSummary(agent.state.messages),
    actions,
    ...(refreshed ? { credential: refreshed } : {}),
  });
}

export async function runStdio(documents: PromptDocuments): Promise<void> {
  // The registration name is historical; it statically includes Pi's OAuth
  // implementations so a single-file Node bundle can perform Codex login.
  registerBunOAuthFlows();
  const request = await readRequest();
  switch (request.action) {
    case "catalog":
      await catalog();
      break;
    case "self_test":
      await selfTest();
      break;
    case "login":
      await login(request);
      break;
    case "prompt":
      await prompt(request, await readSystemPrompt(documents));
      break;
  }
}

export function reportStdioFailure(error: unknown): void {
  send(sanitizeRunnerFailure(error));
  process.exitCode = 1;
}
