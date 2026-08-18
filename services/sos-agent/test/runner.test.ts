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

test("the packaged Linux login uses the embedded Codex OAuth flow", async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), "sos-agent-linux-login-"));
  const home = path.join(directory, "home");
  const bin = path.join(directory, "bin");
  const mockFetch = path.join(directory, "mock-fetch.mjs");
  const credentialPath = path.join(home, ".local/state/sos/agent/auth.json");
  const configPath = path.join(home, ".local/state/sos/agent/config.env");
  try {
    await Promise.all([fs.mkdir(home), fs.mkdir(bin)]);
    await fs.writeFile(
      path.join(bin, "id"),
      "#!/bin/sh\nif [ \"$1\" = -u ]; then printf '1000\\n'; else exec /usr/bin/id \"$@\"; fi\n",
      { mode: 0o755 },
    );
    await fs.writeFile(
      mockFetch,
      [
        "const encoded = (value) => Buffer.from(JSON.stringify(value)).toString('base64url');",
        "const accessToken = `${encoded({ alg: 'none' })}.${encoded({ 'https://api.openai.com/auth': { chatgpt_account_id: 'account-test' } })}.signature`;",
        "globalThis.fetch = async (input) => {",
        "  const url = String(input);",
        "  if (url.endsWith('/api/accounts/deviceauth/usercode')) return Response.json({ device_auth_id: 'device-test', user_code: 'CODE-TEST', interval: 0 });",
        "  if (url.endsWith('/api/accounts/deviceauth/token')) return Response.json({ authorization_code: 'authorization-test', code_verifier: 'verifier-test' });",
        "  if (url.endsWith('/oauth/token')) return Response.json({ access_token: accessToken, refresh_token: 'refresh-test', expires_in: 3600 });",
        "  throw new Error(`unexpected OAuth request: ${url}`);",
        "};",
        "",
      ].join("\n"),
    );
    const environment: NodeJS.ProcessEnv = {
      ...process.env,
      HOME: home,
      NODE_OPTIONS: `--import=${mockFetch}`,
      PATH: `${bin}${path.delimiter}${process.env.PATH ?? ""}`,
      SOS_AGENT_MAIN: path.resolve("dist/agent-runner.cjs"),
    };
    delete environment.XDG_STATE_HOME;

    const result = await runChild(
      "bash",
      [path.resolve("../../packaging/libexec/sos-agent-login"), "--if-needed"],
      environment,
    );
    assert.equal(result.code, 0, result.stderr);
    assert.match(result.stdout, /sos_agent_login_preflight credentials=absent config=absent/);
    assert.match(result.stdout, /Open this URL in your browser:/);
    assert.match(result.stdout, /Enter code: CODE-TEST/);
    assert.doesNotMatch(`${result.stdout}\n${result.stderr}`, /endsWith/);
    const document = JSON.parse(await fs.readFile(credentialPath, "utf8")) as Record<
      string,
      Record<string, unknown>
    >;
    const credential = document["openai-codex"];
    assert.equal(credential?.type, "oauth");
    assert.match(String(credential?.access), /^[^.]+\.[^.]+\.signature$/);
    assert.equal(credential?.refresh, "refresh-test");
    assert.equal(typeof credential?.expires, "number");
    assert.equal(credential?.accountId, "account-test");
    assert.equal(
      await fs.readFile(configPath, "utf8"),
      "SOS_AGENT_PROVIDER=openai-codex\nSOS_AGENT_MODEL=gpt-5.6-sol\n",
    );
    assert.equal((await fs.stat(credentialPath)).mode & 0o777, 0o600);
    assert.equal((await fs.stat(configPath)).mode & 0o777, 0o600);

    const ready = await runChild(
      "bash",
      [path.resolve("../../packaging/libexec/sos-agent-login"), "--if-needed"],
      environment,
    );
    assert.equal(ready.code, 0, ready.stderr);
    assert.match(ready.stdout, /sos_agent_credential_ready provider=openai-codex/);
    assert.match(ready.stdout, /sos_agent_login_ready credentials=/);
    assert.doesNotMatch(ready.stdout, /Enter code:/);

    await fs.writeFile(configPath, "SOS_AGENT_PROVIDER=openai-codex\nSOS_AGENT_MODEL=stale\n");
    const repaired = await runChild(
      "bash",
      [path.resolve("../../packaging/libexec/sos-agent-login"), "--if-needed"],
      environment,
    );
    assert.equal(repaired.code, 0, repaired.stderr);
    assert.match(
      repaired.stdout,
      /sos_agent_login_preflight credentials=preserved config=invalid action=write-config/,
    );
    assert.doesNotMatch(repaired.stdout, /Enter code:/);
    assert.equal(
      await fs.readFile(configPath, "utf8"),
      "SOS_AGENT_PROVIDER=openai-codex\nSOS_AGENT_MODEL=gpt-5.6-sol\n",
    );
  } finally {
    await fs.rm(directory, { recursive: true });
  }
});

test("failed Linux login cleans new empty state and preserves existing credentials", async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), "sos-agent-login-state-"));
  const bin = path.join(directory, "bin");
  const main = path.join(directory, "agent-runner.cjs");
  try {
    await fs.mkdir(bin);
    await Promise.all([
      fs.writeFile(
        path.join(bin, "id"),
        "#!/bin/sh\nif [ \"$1\" = -u ]; then printf '1000\\n'; else exec /usr/bin/id \"$@\"; fi\n",
        { mode: 0o755 },
      ),
      fs.writeFile(
        path.join(bin, "node"),
        [
          "#!/bin/sh",
          "if [ \"${2:-}\" = credential-status ]; then exit 0; fi",
          "printf 'sos_agent_failed error=mock authentication rejected\\n' >&2",
          "exit 23",
          "",
        ].join("\n"),
        { mode: 0o755 },
      ),
      fs.writeFile(main, "// readable mock runner\n"),
    ]);
    const launcher = path.resolve("../../packaging/libexec/sos-agent-login");
    const baseEnvironment: NodeJS.ProcessEnv = {
      ...process.env,
      // Debian 13 runs Bash 5.2; retain its documented compatibility level on newer hosts.
      BASH_COMPAT: "5.2",
      PATH: `${bin}${path.delimiter}${process.env.PATH ?? ""}`,
      SOS_AGENT_MAIN: main,
    };
    delete baseEnvironment.XDG_STATE_HOME;

    const freshHome = path.join(directory, "fresh-home");
    await fs.mkdir(freshHome);
    const fresh = await runChild("bash", [launcher, "--if-needed"], {
      ...baseEnvironment,
      HOME: freshHome,
    });
    assert.equal(fresh.code, 23);
    assert.equal(
      fresh.stderr,
      "sos_agent_failed error=mock authentication rejected\n" +
        "sos_agent_login_incomplete credentials=absent config=absent state_dir=absent " +
        "retry=/usr/local/libexec/sos/sos-agent-login\n",
    );
    await assert.rejects(
      fs.access(path.join(freshHome, ".local/state/sos/agent")),
      (error: NodeJS.ErrnoException) => error.code === "ENOENT",
    );
    await assert.rejects(
      fs.access(path.join(freshHome, ".local/state")),
      (error: NodeJS.ErrnoException) => error.code === "ENOENT",
    );

    const emptyHome = path.join(directory, "empty-home");
    const emptyState = path.join(emptyHome, ".local/state/sos/agent");
    await fs.mkdir(emptyState, { recursive: true, mode: 0o700 });
    const empty = await runChild("bash", [launcher, "--if-needed"], {
      ...baseEnvironment,
      HOME: emptyHome,
    });
    assert.equal(empty.code, 23);
    assert.equal(
      empty.stderr,
      "sos_agent_failed error=mock authentication rejected\n" +
        "sos_agent_login_incomplete credentials=absent config=absent state_dir=preserved " +
        "retry=/usr/local/libexec/sos/sos-agent-login\n",
    );
    assert.equal((await fs.stat(emptyState)).isDirectory(), true);
    assert.deepEqual(await fs.readdir(emptyState), []);

    const existingHome = path.join(directory, "existing-home");
    const existingState = path.join(existingHome, ".local/state/sos/agent");
    const existingCredential = path.join(existingState, "auth.json");
    const existingConfig = path.join(existingState, "config.env");
    const preservedCredential = '{"openai-codex":{"type":"oauth","access":"existing"}}\n';
    const preservedConfig =
      "SOS_AGENT_PROVIDER=openai-codex\nSOS_AGENT_MODEL=gpt-5.6-sol\n";
    await fs.mkdir(existingState, { recursive: true, mode: 0o700 });
    await Promise.all([
      fs.writeFile(existingCredential, preservedCredential, { mode: 0o600 }),
      fs.writeFile(existingConfig, preservedConfig, { mode: 0o600 }),
    ]);
    const existing = await runChild("bash", [launcher], {
      ...baseEnvironment,
      HOME: existingHome,
    });
    assert.equal(existing.code, 23);
    assert.equal(
      existing.stderr,
      "sos_agent_failed error=mock authentication rejected\n" +
        "sos_agent_login_incomplete credentials=preserved config=preserved " +
        "state_dir=preserved retry=/usr/local/libexec/sos/sos-agent-login\n",
    );
    assert.equal(await fs.readFile(existingCredential, "utf8"), preservedCredential);
    assert.equal(await fs.readFile(existingConfig, "utf8"), preservedConfig);
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

function runChild(
  command: string,
  arguments_: string[],
  env: NodeJS.ProcessEnv,
): Promise<{ code: number | null; stdout: string; stderr: string }> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, arguments_, { env, stdio: ["ignore", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8").on("data", (chunk: string) => (stdout += chunk));
    child.stderr.setEncoding("utf8").on("data", (chunk: string) => (stderr += chunk));
    child.once("error", reject);
    child.once("close", (code) => resolve({ code, stdout, stderr }));
  });
}
