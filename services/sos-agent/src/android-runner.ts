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
  createProviderModels,
  type SupportedProvider,
} from "./runtime.js";

const MAX_REQUEST_BYTES = 1024 * 1024;
const MAX_SOURCE_BYTES = 256 * 1024;
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

interface PromptRequest {
  action: "prompt";
  provider: SupportedProvider;
  model: string;
  credential: Credential;
  prompt: string;
  currentSource: string;
  systemPrompt: string;
}

type AndroidRequest = CatalogRequest | SelfTestRequest | LoginRequest | PromptRequest;

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

async function readRequest(): Promise<AndroidRequest> {
  process.stdin.setEncoding("utf8");
  let raw = "";
  for await (const chunk of process.stdin) {
    raw += chunk;
    if (Buffer.byteLength(raw) > MAX_REQUEST_BYTES) throw new Error("request is too large");
  }
  const decoded = JSON.parse(raw) as Record<string, unknown>;
  if (decoded.action === "catalog") return { action: "catalog" };
  if (decoded.action === "self_test") return { action: "self_test" };
  if (decoded.action === "login" && decoded.provider === "openai-codex") {
    return { action: "login", provider: decoded.provider };
  }
  if (
    decoded.action !== "prompt" ||
    !isSupportedProvider(decoded.provider) ||
    typeof decoded.model !== "string" ||
    !decoded.model ||
    !isCredential(decoded.credential) ||
    typeof decoded.prompt !== "string" ||
    !decoded.prompt.trim() ||
    typeof decoded.currentSource !== "string" ||
    !decoded.currentSource ||
    Buffer.byteLength(decoded.currentSource) > MAX_SOURCE_BYTES ||
    typeof decoded.systemPrompt !== "string" ||
    !decoded.systemPrompt
  ) {
    throw new Error("invalid Android Pi request");
  }
  return {
    action: "prompt",
    provider: decoded.provider,
    model: decoded.model,
    credential: decoded.credential,
    prompt: decoded.prompt,
    currentSource: decoded.currentSource,
    systemPrompt: decoded.systemPrompt,
  };
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
  const { createFauxAgentRuntime } = await import("./runtime.js");
  const agent = createFauxAgentRuntime(backend, "Android native Pi self-test", candidate);
  await agent.prompt("Run the bounded Android Pi self-test");
  const expected = ["get_experience_context", "validate_experience", "submit_experience"];
  if (JSON.stringify(actions) !== JSON.stringify(expected)) {
    throw new Error("Android Pi self-test used an unexpected tool sequence");
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

async function prompt(request: PromptRequest): Promise<void> {
  const credentials = new InMemoryCredentialStore();
  await credentials.modify(request.provider, async () => request.credential);
  let validated: string | undefined;
  let submitted: string | undefined;
  const backend: AuthoringBackend = {
    async request(action) {
      if (action.action === "get_experience_context") {
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
        validated = action.source;
        return {
          valid: true,
          pending_trusted_host_validation: true,
          schema_version: 3,
        };
      }
      if (validated !== action.source) {
        throw new Error("submitted source differs from the staged candidate");
      }
      submitted = action.source;
      return { accepted: true, activated: false, pending_trusted_host_validation: true };
    },
  };
  const agent = createAgentRuntime({
    backend,
    systemPrompt: request.systemPrompt,
    provider: request.provider,
    model: request.model,
    credentials,
  });
  await agent.prompt(request.prompt);
  if (!submitted) throw new Error("Pi completed without submitting a candidate experience");
  const refreshed = await credentials.read(request.provider);
  if (!refreshed) throw new Error("Pi completed without a provider credential");
  send({
    type: "prompt_complete",
    provider: request.provider,
    source: submitted,
    summary: lastAssistantSummary(agent.state.messages),
    credential: refreshed,
  });
}

async function main(): Promise<void> {
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
      await prompt(request);
      break;
  }
}

main().catch((error: Error) => {
  send({ type: "error", error: error.message || "Android Pi runner failed" });
  process.exitCode = 1;
});
