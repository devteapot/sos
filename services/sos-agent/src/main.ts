#!/usr/bin/env node
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { createInterface } from "node:readline/promises";
import type { AuthEvent, AuthPrompt } from "@earendil-works/pi-ai";
import { JsonCredentialStore } from "./auth.js";
import { UnixAuthoringBackend } from "./authoring.js";
import {
  createAgentRuntime,
  createFauxAgentRuntime,
  loadMessages,
  loginProvider,
  type SupportedProvider,
} from "./runtime.js";
import { promptAgent, startAgentServer } from "./server.js";

function option(name: string): string | undefined {
  const index = process.argv.indexOf(name);
  return index < 0 ? undefined : process.argv[index + 1];
}

function required(name: string): string {
  const value = option(name);
  if (!value) throw new Error(`missing required option ${name}`);
  return value;
}

function enabled(name: string): boolean {
  return process.argv.includes(name);
}

function supportedProvider(value: string): SupportedProvider {
  if (value === "openai" || value === "anthropic" || value === "openai-codex") return value;
  throw new Error("SOS_AGENT_PROVIDER must be openai, anthropic, or openai-codex");
}

async function readSystemPrompt(apiPath: string, examples: string[]): Promise<string> {
  const documents = await Promise.all([apiPath, ...examples].map((file) => fs.readFile(file, "utf8")));
  return `You are the resident SOS experience author. You modify the currently running visual experience in response to the user's direct request.

Rules:
- Always call get_experience_context first.
- Return complete Luau module source, never a patch.
- Call validate_experience before submit_experience.
- Submit only the exact source that validated.
- Do not claim activation unless submit_experience succeeds.
- You have no shell, filesystem, process, or general network tools.
- Preserve the user's current intent and durable state unless they ask for a reset.
- Every revision must keep a visible Luau-authored agent conversation/composer that renders model.agent and emits agent.prompt. You may redesign and reposition it, but never replace it with a native widget or remove the user's way to request another change.

SOS experience API:
${documents[0]}

Reference experiences:
${documents.slice(1).join("\n\n---\n\n")}`;
}

async function credential(): Promise<string | undefined> {
  if (process.env.SOS_AGENT_API_KEY) return process.env.SOS_AGENT_API_KEY;
  const directory = process.env.CREDENTIALS_DIRECTORY;
  if (!directory) return undefined;
  try {
    return (await fs.readFile(path.join(directory, "agent-api-key"), "utf8")).trim();
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return undefined;
    throw error;
  }
}

function reportAuthEvent(event: AuthEvent): void {
  switch (event.type) {
    case "auth_url":
      process.stdout.write(`Open this URL in your browser:\n${event.url}\n`);
      if (event.instructions) process.stdout.write(`${event.instructions}\n`);
      break;
    case "device_code":
      process.stdout.write(`Open this URL in your browser:\n${event.verificationUri}\n`);
      process.stdout.write(`Enter code: ${event.userCode}\n`);
      break;
    case "info":
    case "progress":
      process.stdout.write(`${event.message}\n`);
      break;
  }
}

async function login(): Promise<void> {
  const provider = supportedProvider(
    option("--provider") ?? process.env.SOS_AGENT_PROVIDER ?? "openai-codex",
  );
  const credentialPath = required("--credentials");
  const deviceCode = enabled("--device-code");
  const controller = new AbortController();
  const cancel = () => controller.abort(new Error("login cancelled"));
  process.once("SIGINT", cancel);
  process.once("SIGTERM", cancel);
  const terminal = createInterface({ input: process.stdin, output: process.stdout });
  try {
    await loginProvider(provider, new JsonCredentialStore(credentialPath), {
      signal: controller.signal,
      async prompt(prompt: AuthPrompt): Promise<string> {
        if (prompt.type === "select") {
          if (deviceCode) {
            const choice = prompt.options.find((candidate) => candidate.id === "device_code");
            if (!choice) throw new Error(`${provider} does not offer device-code login`);
            return choice.id;
          }
          process.stdout.write(`${prompt.message}\n`);
          prompt.options.forEach((candidate, index) =>
            process.stdout.write(`  ${index + 1}. ${candidate.label}\n`),
          );
          const answer = Number.parseInt(
            await terminal.question(`Enter number (1-${prompt.options.length}): `, {
              signal: prompt.signal ?? controller.signal,
            }),
            10,
          );
          const choice = prompt.options[answer - 1];
          if (!choice) throw new Error("invalid login selection");
          return choice.id;
        }
        return terminal.question(
          `${prompt.message}${prompt.placeholder ? ` (${prompt.placeholder})` : ""}: `,
          { signal: prompt.signal ?? controller.signal },
        );
      },
      notify: reportAuthEvent,
    });
    process.stdout.write(`SOS agent credentials saved to ${credentialPath}\n`);
  } finally {
    terminal.close();
    process.off("SIGINT", cancel);
    process.off("SIGTERM", cancel);
  }
}

async function main(): Promise<void> {
  const command = process.argv[2];
  if (command === "login") {
    await login();
    return;
  }
  if (command === "prompt") {
    const exitCode = await promptAgent(required("--socket"), required("--request"));
    process.exitCode = exitCode;
    return;
  }
  if (command !== "serve") throw new Error("usage: sos-agent serve|login|prompt [options]");

  const socketPath = required("--socket");
  const backend = new UnixAuthoringBackend(required("--authoring-socket"));
  const systemPrompt = await readSystemPrompt(required("--api-doc"), [
    required("--example"),
    required("--example-secondary"),
  ]);
  const fakeSource = option("--fake-source") ?? process.env.SOS_AGENT_FAKE_SOURCE;
  const statePath = option("--state");
  const provider = supportedProvider(process.env.SOS_AGENT_PROVIDER ?? "openai");
  const credentialPath = option("--credentials");
  if (!fakeSource && provider === "openai-codex" && !credentialPath) {
    throw new Error("openai-codex requires --credentials with a prior sos-agent login");
  }
  const credentials = credentialPath ? new JsonCredentialStore(credentialPath) : undefined;
  if (!fakeSource && provider === "openai-codex" && !(await credentials?.read(provider))) {
    throw new Error(`openai-codex is not authenticated; run sos-agent login for ${credentialPath}`);
  }
  const apiKey = fakeSource || provider === "openai-codex" ? undefined : await credential();
  const agent = fakeSource
    ? createFauxAgentRuntime(backend, systemPrompt, await fs.readFile(fakeSource, "utf8"))
    : createAgentRuntime({
        backend,
        systemPrompt,
        provider,
        model: process.env.SOS_AGENT_MODEL ?? "",
        ...(apiKey ? { apiKey } : {}),
        ...(credentials ? { credentials } : {}),
        messages: await loadMessages(statePath),
      });
  const server = await startAgentServer({
    socketPath,
    agent,
    ...(statePath ? { statePath } : {}),
  });
  console.log(`sos_agent_listening socket=${socketPath} model=${fakeSource ? "faux" : agent.state.model.id}`);
  for (const signal of ["SIGINT", "SIGTERM"] as const) {
    process.once(signal, () => server.close());
  }
}

main().catch((error: Error) => {
  console.error(`sos_agent_failed error=${error.message}`);
  process.exitCode = 1;
});
