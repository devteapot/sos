import assert from "node:assert/strict";
import fs from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import type { AuthoringBackend } from "../src/authoring.js";
import { createFauxAgentRuntime } from "../src/runtime.js";
import { startAgentServer } from "../src/server.js";

test("a prompt reaches Pi and uses only the bounded authoring flow", async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), "sos-agent-test-"));
  const socketPath = path.join(directory, "agent.sock");
  const actions: string[] = [];
  const backend: AuthoringBackend = {
    async request(request) {
      actions.push(request.action);
      if (request.action === "get_experience_context") {
        return { revision_id: "a".repeat(64), source: "active", schema_version: 1 };
      }
      if (request.action === "validate_experience") return { valid: true, schema_version: 1 };
      return { activated: true, revision_id: "b".repeat(64) };
    },
  };
  const source = "return { api_version = 3, render = function() return {} end }";
  const agent = createFauxAgentRuntime(backend, "test system prompt", source);
  const server = await startAgentServer({ socketPath, agent });

  const events = await exchange(socketPath, { action: "prompt", prompt: "Make it calmer" });
  assert.deepEqual(actions, [
    "get_experience_context",
    "validate_experience",
    "submit_experience",
  ]);
  assert.equal(events.at(-1)?.type, "completed");
  assert.ok(events.some((event) => event.type === "text_delta"));
  assert.deepEqual(
    events.filter((event) => event.type === "tool_start").map((event) => event.name),
    ["get_experience_context", "validate_experience", "submit_experience"],
  );

  await new Promise<void>((resolve) => server.close(() => resolve()));
  await fs.rm(directory, { recursive: true });
});

function exchange(socketPath: string, request: unknown): Promise<Record<string, unknown>[]> {
  return new Promise((resolve, reject) => {
    const events: Record<string, unknown>[] = [];
    let input = "";
    const socket = net.createConnection(socketPath);
    socket.setEncoding("utf8");
    socket.on("connect", () => socket.write(`${JSON.stringify(request)}\n`));
    socket.on("data", (chunk: string) => {
      input += chunk;
      while (input.includes("\n")) {
        const index = input.indexOf("\n");
        const line = input.slice(0, index);
        input = input.slice(index + 1);
        if (line) events.push(JSON.parse(line) as Record<string, unknown>);
      }
    });
    socket.on("error", reject);
    socket.on("end", () => resolve(events));
  });
}
