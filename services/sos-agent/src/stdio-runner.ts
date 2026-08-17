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
    openrouter: "openai/gpt-5.4-mini",
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
  if (live) await credentials.modify(request.provider, async () => request.credential);
  let validated: string | undefined;
  let submitted: string | undefined;
  const actions: string[] = [];
  const backend: AuthoringBackend = {
    async request(action) {
      if (action.action === "get_experience_context") {
        if (actions.length !== 0) throw new Error("experience context must be the first tool call");
        actions.push(action.action);
        return {
          revision_id: "0".repeat(64),
          source: request.currentSource,
          schema_version: 3,
        };
      }
      if (Buffer.byteLength(action.source) > MAX_SOURCE_BYTES) {
        throw new Error("candidate source is too large");
      }
      if (action.action === "validate_experience") {
        if (actions.length !== 1 || actions[0] !== "get_experience_context") {
          throw new Error("candidate validation must follow experience context");
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
        throw new Error("candidate submission must follow validation");
      }
      if (validated !== action.source) {
        throw new Error("submitted source differs from the staged candidate");
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
  await agent.prompt(request.prompt);
  if (!submitted) throw new Error("Pi completed without submitting a candidate experience");
  const expectedActions = ["get_experience_context", "validate_experience", "submit_experience"];
  if (JSON.stringify(actions) !== JSON.stringify(expectedActions)) {
    throw new Error("Pi completed without the bounded authoring tool sequence");
  }
  const refreshed = live ? await credentials.read(request.provider) : undefined;
  if (live && !refreshed) throw new Error("Pi completed without a provider credential");
  send({
    type: "prompt_complete",
    provider: request.provider,
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

export function reportStdioFailure(error: Error): void {
  send({ type: "error", error: error.message || "SOS Pi runner failed" });
  process.exitCode = 1;
}
