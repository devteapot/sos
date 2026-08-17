import fs from "node:fs/promises";
import nodePath from "node:path";
import {
  createModels,
  fauxAssistantMessage,
  fauxProvider,
  fauxToolCall,
  type Model,
  type Api,
  type AuthInteraction,
  type CredentialStore,
  type MutableModels,
} from "@earendil-works/pi-ai";
import { anthropicProvider } from "@earendil-works/pi-ai/providers/anthropic";
import { openaiCodexProvider } from "@earendil-works/pi-ai/providers/openai-codex";
import { openaiProvider } from "@earendil-works/pi-ai/providers/openai";
import { openrouterProvider } from "@earendil-works/pi-ai/providers/openrouter";
import { Agent, type AgentMessage } from "@earendil-works/pi-agent-core";
import { createAuthoringTools, type AuthoringBackend } from "./authoring.js";

export type SupportedProvider = "openai" | "anthropic" | "openai-codex" | "openrouter";

export interface AgentRuntimeOptions {
  backend: AuthoringBackend;
  systemPrompt: string;
  provider: SupportedProvider;
  model: string;
  apiKey?: string;
  credentials?: CredentialStore;
  messages?: AgentMessage[];
}

export function createAgentRuntime(options: AgentRuntimeOptions): Agent {
  const models = createProviderModels(options.provider, options.credentials);
  const model = models.getModel(options.provider, options.model);
  if (!model) {
    const known = models
      .getModels(options.provider)
      .map((candidate) => candidate.id)
      .join(", ");
    throw new Error(`unknown ${options.provider} model ${options.model}; available: ${known}`);
  }
  return new Agent({
    initialState: {
      systemPrompt: options.systemPrompt,
      model,
      tools: createAuthoringTools(options.backend),
      messages: options.messages ?? [],
    },
    streamFn: (requestModel, context, streamOptions) =>
      models.streamSimple(requestModel, context, {
        ...streamOptions,
        ...(options.apiKey ? { apiKey: options.apiKey } : {}),
      }),
    toolExecution: "sequential",
  });
}

export function createProviderModels(
  provider: SupportedProvider,
  credentials?: CredentialStore,
): MutableModels {
  const models = createModels(credentials ? { credentials } : undefined);
  switch (provider) {
    case "openai":
      models.setProvider(openaiProvider());
      break;
    case "anthropic":
      models.setProvider(anthropicProvider());
      break;
    case "openai-codex":
      models.setProvider(openaiCodexProvider());
      break;
    case "openrouter":
      models.setProvider(openrouterProvider());
      break;
  }
  return models;
}

export async function loginProvider(
  provider: SupportedProvider,
  credentials: CredentialStore,
  interaction: AuthInteraction,
): Promise<void> {
  const models = createProviderModels(provider, credentials);
  await models.login(provider, "oauth", interaction);
}

export function createFauxAgentRuntime(
  backend: AuthoringBackend,
  systemPrompt: string,
  candidateSource: string,
  completionText = "The candidate experience is active.",
): Agent {
  const faux = fauxProvider();
  const models = createModels();
  models.setProvider(faux.provider);
  faux.setResponses([
    fauxAssistantMessage(fauxToolCall("get_experience_context", {}, { id: "context-1" }), {
      stopReason: "toolUse",
    }),
    fauxAssistantMessage(
      fauxToolCall("validate_experience", { source: candidateSource }, { id: "validate-1" }),
      { stopReason: "toolUse" },
    ),
    fauxAssistantMessage(
      fauxToolCall("submit_experience", { source: candidateSource }, { id: "submit-1" }),
      { stopReason: "toolUse" },
    ),
    fauxAssistantMessage(completionText),
  ]);
  return new Agent({
    initialState: {
      systemPrompt,
      model: faux.getModel() as Model<Api>,
      tools: createAuthoringTools(backend),
    },
    streamFn: models.streamSimple.bind(models),
    toolExecution: "sequential",
  });
}

export async function loadMessages(path: string | undefined): Promise<AgentMessage[]> {
  if (!path) return [];
  try {
    return JSON.parse(await fs.readFile(path, "utf8")) as AgentMessage[];
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return [];
    throw error;
  }
}

export async function saveMessages(path: string | undefined, messages: AgentMessage[]): Promise<void> {
  if (!path) return;
  const temporary = `${path}.tmp-${process.pid}`;
  await fs.mkdir(nodePath.dirname(path), { recursive: true });
  await fs.writeFile(temporary, `${JSON.stringify(messages)}\n`, { mode: 0o600 });
  await fs.rename(temporary, path);
}
