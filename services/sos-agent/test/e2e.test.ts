import assert from "node:assert/strict";
import fs from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { createAuthoringTools, type AuthoringBackend } from "../src/authoring.js";
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
  const source = "return { api_version = 4, exports = { main = { render = function() return {} end } } }";
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

test("the shared tools reject a source that differs from the validated candidate", async () => {
  const actions: string[] = [];
  const backend: AuthoringBackend = {
    async request(request) {
      actions.push(request.action);
      return { ok: true };
    },
  };
  const tools = createAuthoringTools(backend);
  const signal = new AbortController().signal;
  await tools[0]!.execute("context", {}, signal);
  await tools[1]!.execute("validate", { source: "validated" }, signal);
  await assert.rejects(
    tools[2]!.execute("submit", { source: "different" }, signal),
    /exactly match the validated candidate/,
  );
  assert.deepEqual(actions, ["get_experience_context", "validate_experience"]);
});

test("the shared tools bind revision-local modules to the validated package", async () => {
  const requests: unknown[] = [];
  const backend: AuthoringBackend = {
    async request(request) {
      requests.push(request);
      return { ok: true };
    },
  };
  const tools = createAuthoringTools(backend);
  const signal = new AbortController().signal;
  const modules = [{ id: "stock.theme", source: "return { color = {} }" }];
  await tools[0]!.execute("context", {}, signal);
  await tools[1]!.execute("validate", { source: "validated", modules }, signal);
  await assert.rejects(
    tools[2]!.execute("submit", {
      source: "validated",
      modules: [{ ...modules[0], source: "return {}" }],
    }, signal),
    /exactly match the validated candidate/,
  );
  assert.equal(requests.length, 2);
  assert.deepEqual(requests[1], { action: "validate_experience", source: "validated", modules });
});

test("a structured invalid report keeps the tools in validation phase", async () => {
  const actions: string[] = [];
  const backend: AuthoringBackend = {
    async request(request) {
      actions.push(request.action);
      if (request.action === "validate_experience") {
        return { valid: false, report: { valid: false, scenarios: [] } };
      }
      return { ok: true };
    },
  };
  const tools = createAuthoringTools(backend);
  const signal = new AbortController().signal;
  await tools[0]!.execute("context", {}, signal);
  const result = await tools[1]!.execute("validate", { source: "invalid" }, signal);
  assert.match(JSON.stringify(result), /\"valid\":false/);
  await assert.rejects(
    tools[2]!.execute("submit", { source: "invalid" }, signal),
    /exactly match the validated candidate/,
  );
  await tools[1]!.execute("validate-again", { source: "still-invalid" }, signal);
  assert.deepEqual(actions, [
    "get_experience_context",
    "validate_experience",
    "validate_experience",
  ]);
});

test("derived authoring binds exact parents and the complete validated package", async () => {
  const requests: unknown[] = [];
  const backend: AuthoringBackend = {
    async request(request) {
      requests.push(request);
      return { valid: true };
    },
  };
  const tools = createAuthoringTools(backend);
  const signal = new AbortController().signal;
  const parents = [
    { experience_id: "agenda", revision_id: "a".repeat(64) },
    { experience_id: "media", revision_id: "b".repeat(64) },
  ];
  const candidate = {
    target_experience_id: "agenda-media-remix",
    parents,
    request: "Combine agenda and media",
    rationale: "The user requested one information architecture.",
    contract: { contract_version: 1, exports: {} },
    source: "return { api_version = 4, exports = {} }",
  };
  await tools[3]!.execute("parents", { parents }, signal);
  await tools[4]!.execute("validate", candidate, signal);
  await assert.rejects(
    tools[5]!.execute("submit", { ...candidate, source: `${candidate.source} ` }, signal),
    /exactly match the validated derived candidate/,
  );
  await tools[5]!.execute("submit", candidate, signal);
  assert.deepEqual(
    requests.map((request) => (request as { action: string }).action),
    [
      "get_derivation_context",
      "validate_derived_experience",
      "submit_derived_experience",
    ],
  );
});

test("composition authoring binds exact dependencies and the validated graph root", async () => {
  const requests: unknown[] = [];
  const backend: AuthoringBackend = {
    async request(request) {
      requests.push(request);
      return { valid: true };
    },
  };
  const tools = createAuthoringTools(backend);
  const signal = new AbortController().signal;
  const dependencies = [
    {
      alias: "agenda",
      experience_id: "agenda",
      revision_id: "a".repeat(64),
      export_id: "main",
      policy: "locked" as const,
      grant: { events: ["item_selected"] },
    },
    {
      alias: "media",
      experience_id: "media",
      revision_id: "b".repeat(64),
      export_id: "compact",
      policy: "tracked" as const,
    },
  ];
  const candidate = {
    target_experience_id: "dashboard",
    dependencies,
    contract: { contract_version: 1, exports: {} },
    source: "return { api_version = 4, exports = {} }",
  };
  await tools[6]!.execute("dependencies", { dependencies }, signal);
  await tools[7]!.execute("validate", candidate, signal);
  await assert.rejects(
    tools[8]!.execute("submit", { ...candidate, source: `${candidate.source} ` }, signal),
    /exactly match the validated composed candidate/,
  );
  await tools[8]!.execute("submit", candidate, signal);
  assert.deepEqual(
    requests.map((request) => (request as { action: string }).action),
    [
      "get_composition_context",
      "validate_composed_experience",
      "submit_composed_experience",
    ],
  );
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
