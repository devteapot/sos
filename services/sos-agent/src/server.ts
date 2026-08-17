import fs from "node:fs/promises";
import net from "node:net";
import type { Agent, AgentEvent } from "@earendil-works/pi-agent-core";
import { saveMessages } from "./runtime.js";
import { isBoundedPrompt, MAX_PROMPT_BYTES } from "./contract.js";

const MAX_PROMPT_REQUEST_BYTES = MAX_PROMPT_BYTES + 1024;

interface PromptRequest {
  action: "prompt";
  prompt: string;
}

export interface AgentServerOptions {
  socketPath: string;
  statePath?: string;
  agent: Agent;
}

function send(socket: net.Socket, event: unknown): void {
  socket.write(`${JSON.stringify(event)}\n`);
}

function emitAgentEvent(socket: net.Socket, event: AgentEvent): void {
  if (event.type === "message_update" && event.assistantMessageEvent.type === "text_delta") {
    send(socket, { type: "text_delta", delta: event.assistantMessageEvent.delta });
  } else if (event.type === "tool_execution_start") {
    send(socket, { type: "tool_start", id: event.toolCallId, name: event.toolName });
  } else if (event.type === "tool_execution_end") {
    send(socket, {
      type: "tool_end",
      id: event.toolCallId,
      name: event.toolName,
      ok: !event.isError,
      details: event.result.details,
    });
  }
}

export async function startAgentServer(options: AgentServerOptions): Promise<net.Server> {
  await prepareSocket(options.socketPath);
  const server = net.createServer((socket) => {
    socket.setEncoding("utf8");
    let input = "";
    let handled = false;
    socket.on("data", (chunk: string) => {
      if (handled) return;
      input += chunk;
      if (Buffer.byteLength(input) > MAX_PROMPT_REQUEST_BYTES) {
        handled = true;
        send(socket, { type: "failed", error: "prompt request is too large" });
        socket.end();
        return;
      }
      const newline = input.indexOf("\n");
      if (newline < 0) return;
      handled = true;
      void runPrompt(options, socket, input.slice(0, newline));
    });
  });
  server.on("close", () => void fs.unlink(options.socketPath).catch(() => undefined));
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(options.socketPath, () => {
      server.off("error", reject);
      resolve();
    });
  });
  await fs.chmod(options.socketPath, 0o660);
  return server;
}

async function prepareSocket(socketPath: string): Promise<void> {
  let metadata;
  try {
    metadata = await fs.lstat(socketPath);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return;
    throw error;
  }
  if (!metadata.isSocket()) throw new Error(`refusing to replace non-socket ${socketPath}`);
  const active = await new Promise<boolean>((resolve, reject) => {
    const probe = net.createConnection(socketPath);
    probe.once("connect", () => {
      probe.destroy();
      resolve(true);
    });
    probe.once("error", (error: NodeJS.ErrnoException) => {
      if (error.code === "ECONNREFUSED" || error.code === "ENOENT") resolve(false);
      else reject(error);
    });
  });
  if (active) throw new Error(`an active agent already owns ${socketPath}`);
  await fs.unlink(socketPath).catch((error: NodeJS.ErrnoException) => {
    if (error.code !== "ENOENT") throw error;
  });
}

async function runPrompt(options: AgentServerOptions, socket: net.Socket, line: string): Promise<void> {
  if (options.agent.state.isStreaming) {
    send(socket, { type: "failed", error: "the agent is already handling a prompt" });
    socket.end();
    return;
  }
  let request: PromptRequest;
  try {
    request = JSON.parse(line) as PromptRequest;
    if (request.action !== "prompt" || !isBoundedPrompt(request.prompt)) {
      throw new Error("expected a non-empty prompt request");
    }
  } catch (error) {
    send(socket, { type: "failed", error: (error as Error).message });
    socket.end();
    return;
  }

  send(socket, { type: "accepted" });
  const unsubscribe = options.agent.subscribe((event) => emitAgentEvent(socket, event));
  try {
    await options.agent.prompt(request.prompt);
    await saveMessages(options.statePath, options.agent.state.messages);
    send(socket, { type: "completed" });
  } catch (error) {
    send(socket, { type: "failed", error: (error as Error).message });
  } finally {
    unsubscribe();
    socket.end();
  }
}

export async function promptAgent(socketPath: string, prompt: string): Promise<number> {
  return new Promise((resolve, reject) => {
    const socket = net.createConnection(socketPath);
    let input = "";
    let exitCode = 1;
    socket.setEncoding("utf8");
    socket.on("connect", () => socket.write(`${JSON.stringify({ action: "prompt", prompt })}\n`));
    socket.on("data", (chunk: string) => {
      input += chunk;
      while (input.includes("\n")) {
        const index = input.indexOf("\n");
        const line = input.slice(0, index);
        input = input.slice(index + 1);
        if (!line) continue;
        const event = JSON.parse(line) as Record<string, unknown>;
        if (event.type === "text_delta") process.stdout.write(String(event.delta));
        if (event.type === "tool_start") process.stderr.write(`\n[${String(event.name)}]\n`);
        if (event.type === "completed") exitCode = 0;
        if (event.type === "failed") process.stderr.write(`sos-agent: ${String(event.error)}\n`);
      }
    });
    socket.on("error", reject);
    socket.on("end", () => {
      if (exitCode === 0) process.stdout.write("\n");
      resolve(exitCode);
    });
  });
}
