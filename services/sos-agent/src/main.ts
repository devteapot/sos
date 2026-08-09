#!/usr/bin/env node
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { UnixAuthoringBackend } from "./authoring.js";
import { createAgentRuntime, createFauxAgentRuntime, loadMessages } from "./runtime.js";
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

async function main(): Promise<void> {
  const command = process.argv[2];
  if (command === "prompt") {
    const exitCode = await promptAgent(required("--socket"), required("--request"));
    process.exitCode = exitCode;
    return;
  }
  if (command !== "serve") throw new Error("usage: sos-agent serve|prompt [options]");

  const socketPath = required("--socket");
  const backend = new UnixAuthoringBackend(required("--authoring-socket"));
  const systemPrompt = await readSystemPrompt(required("--api-doc"), [
    required("--example"),
    required("--example-secondary"),
  ]);
  const fakeSource = option("--fake-source") ?? process.env.SOS_AGENT_FAKE_SOURCE;
  const statePath = option("--state");
  const provider = process.env.SOS_AGENT_PROVIDER ?? "openai";
  if (provider !== "openai" && provider !== "anthropic") {
    throw new Error("SOS_AGENT_PROVIDER must be openai or anthropic");
  }
  const apiKey = await credential();
  const agent = fakeSource
    ? createFauxAgentRuntime(backend, systemPrompt, await fs.readFile(fakeSource, "utf8"))
    : createAgentRuntime({
        backend,
        systemPrompt,
        provider,
        model: process.env.SOS_AGENT_MODEL ?? "",
        ...(apiKey ? { apiKey } : {}),
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
