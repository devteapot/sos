import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  decodeRequest,
  PINNED_OPENROUTER_MODEL,
  promptResponseModel,
  sanitizeRunnerFailure,
} from "../src/stdio-runner.js";
import { buildSystemPrompt } from "../src/prompt-policy.js";

test("the package build removes obsolete Android-only runner outputs", async () => {
  const obsolete = (await fs.readdir(path.resolve("dist/src"))).filter((entry) =>
    entry.startsWith("android-runner."),
  );
  assert.deepEqual(obsolete, []);
  await assert.rejects(fs.access(path.resolve("dist/android-runner.cjs")));
});

test("the packaged runner applies the bounded faux Pi contract", async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), "sos-agent-runner-"));
  const api = path.join(directory, "experience-api.md");
  const primary = path.join(directory, "primary.luau");
  const secondary = path.join(directory, "secondary.luau");
  const candidate = "return { api_version = 3, render = function() return { id = 'next' } end }";
  try {
    await Promise.all([
      fs.writeFile(api, "# Test API\n"),
      fs.writeFile(primary, "return { api_version = 3 }\n"),
      fs.writeFile(secondary, "return { api_version = 3, alternate = true }\n"),
    ]);
    const response = await exchange(
      [
        path.resolve("dist/agent-runner.cjs"),
        "stdio",
        "--api-doc",
        api,
        "--example",
        primary,
        "--example-secondary",
        secondary,
      ],
      {
        action: "prompt",
        provider: "faux",
        prompt: "Make this calmer",
        currentSource: "return { api_version = 3 }",
        candidateSource: candidate,
      },
    );
    assert.equal(response.type, "prompt_complete");
    assert.equal(response.provider, "faux");
    assert.equal(response.model, "faux");
    assert.equal(response.source, candidate);
    assert.deepEqual(response.actions, [
      "get_experience_context",
      "validate_experience",
      "submit_experience",
    ]);
    assert.match(String(response.summary), /staged for trusted host validation/);
  } finally {
    await fs.rm(directory, { recursive: true });
  }
});

test("the shared request contract rejects oversized prompts", () => {
  assert.throws(
    () =>
      decodeRequest(
        JSON.stringify({
          action: "prompt",
          provider: "faux",
          prompt: "x".repeat(32 * 1024 + 1),
          currentSource: "active",
          candidateSource: "candidate",
        }),
      ),
    /invalid Pi runner request/,
  );
});

test("the bounded OpenRouter request accepts only the campaign model", () => {
  const request = {
    action: "prompt",
    provider: "openrouter",
    model: PINNED_OPENROUTER_MODEL,
    credential: { type: "api_key", key: "x" },
    prompt: "Make this calmer",
    currentSource: "return { api_version = 3 }",
  };
  assert.equal(
    (decodeRequest(JSON.stringify(request)) as { model: string }).model,
    "deepseek/deepseek-v4-flash-0731",
  );
  assert.equal(
    promptResponseModel({ provider: "openrouter", model: PINNED_OPENROUTER_MODEL }),
    "deepseek/deepseek-v4-flash-0731",
  );
  for (const model of [
    "deepseek/deepseek-v4-flash",
    "deepseek/deepseek-v4-flash-latest",
    "deepseek/deepseek-v4-flash-0731:free",
    "deepseek/deepseek-v4-flash-0731-extra",
    "openai/gpt-5.4-mini",
  ]) {
    assert.throws(
      () => decodeRequest(JSON.stringify({ ...request, model })),
      /invalid Pi runner request/,
    );
  }
});

test("runner failures expose only bounded categories and safe numeric status", () => {
  const secret = "sk-or-v1-do-not-surface";
  const failure = sanitizeRunnerFailure({
    status: 401,
    message: `Authorization: Bearer ${secret}`,
    response: { body: `raw provider body ${secret}` },
  }, PINNED_OPENROUTER_MODEL);
  assert.deepEqual(failure, {
    type: "error",
    stage: "credential",
    category: "credential_rejected",
    error: "The provider rejected the configured credential.",
    model: PINNED_OPENROUTER_MODEL,
    status: 401,
  });
  assert.ok(!JSON.stringify(failure).includes(secret));
});

test("the prompt policy rejects the actual combined document bytes", () => {
  assert.throws(
    () => buildSystemPrompt("é".repeat(512 * 1024), ["reference"]),
    /outside the bounded size/,
  );
});

function exchange(arguments_: string[], request: unknown): Promise<Record<string, unknown>> {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, arguments_, { stdio: ["pipe", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8").on("data", (chunk: string) => (stdout += chunk));
    child.stderr.setEncoding("utf8").on("data", (chunk: string) => (stderr += chunk));
    child.once("error", reject);
    child.once("close", (code) => {
      if (code !== 0) {
        reject(new Error(`runner exited ${code}: ${stderr}`));
        return;
      }
      const line = stdout.trim().split("\n").at(-1);
      if (!line) {
        reject(new Error("runner returned no response"));
        return;
      }
      resolve(JSON.parse(line) as Record<string, unknown>);
    });
    child.stdin.end(JSON.stringify(request));
  });
}
