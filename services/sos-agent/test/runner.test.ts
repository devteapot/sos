import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { decodeRequest } from "../src/stdio-runner.js";
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
