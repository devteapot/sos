import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  Response as NodeFetchResponse,
  type RequestInit as NodeFetchRequestInit,
} from "node-fetch";
import {
  decodeRequest,
  observeProviderFetch,
  PINNED_OPENROUTER_MODEL,
  preflightProviderDns,
  promptResponseModel,
  sanitizeRunnerFailure,
  stdioFailureEnvelope,
} from "../src/stdio-runner.js";
import {
  CORE_DEV_OPENROUTER_PROXY,
  fixedCoreDevProxyFetch,
} from "../src/core-dev-proxy.js";
import { nodeProviderFetchWithOptions } from "../src/provider-fetch.js";
import { buildSystemPrompt } from "../src/prompt-policy.js";

test("the package build removes obsolete Android-only runner outputs", async () => {
  const obsolete = (await fs.readdir(path.resolve("dist/src"))).filter((entry) =>
    entry.startsWith("android-runner."),
  );
  assert.deepEqual(obsolete, []);
  await assert.rejects(fs.access(path.resolve("dist/android-runner.cjs")));
});

test("only the Core-dev bundle contains the fixed CONNECT proxy", async () => {
  const [ordinary, coreDev] = await Promise.all([
    fs.readFile(path.resolve("dist/agent-runner.cjs"), "utf8"),
    fs.readFile(path.resolve("dist/agent-runner-core-dev.cjs"), "utf8"),
  ]);
  for (const marker of ["http://127.0.0.1:37173", "HttpsProxyAgent"]) {
    assert.equal(ordinary.includes(marker), false);
    assert.equal(coreDev.includes(marker), true);
  }
  for (const bundle of [ordinary, coreDev]) {
    assert.equal(bundle.includes("nodeProviderFetch"), true);
    assert.equal(bundle.includes("WebAssembly"), false);
  }
  assert.equal(ordinary.includes(PINNED_OPENROUTER_MODEL), true);
  assert.equal(coreDev.includes(PINNED_OPENROUTER_MODEL), true);
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
    assert.equal(response.protocol_version, 2);
    assert.equal(response.terminal, "completed");
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

test("the Core-dev proxy contract accepts one fixed loopback value only", async () => {
  const request = {
    action: "prompt",
    provider: "openrouter",
    model: PINNED_OPENROUTER_MODEL,
    credential: { type: "api_key", key: "synthetic-never-log" },
    prompt: "fixed",
    currentSource: "return { api_version = 3 }",
    coreDevProxy: CORE_DEV_OPENROUTER_PROXY,
  };
  assert.equal(CORE_DEV_OPENROUTER_PROXY, "http://127.0.0.1:37173");
  assert.throws(() => decodeRequest(JSON.stringify(request)), /invalid Pi runner request/);

  const arguments_ = [
    "stdio",
    "--api-doc",
    "/nonexistent/core-dev-proxy-test-api",
    "--example",
    "/nonexistent/core-dev-proxy-test-primary",
    "--example-secondary",
    "/nonexistent/core-dev-proxy-test-secondary",
  ];
  const devTerminal = await exchangeTerminal(
    [path.resolve("dist/agent-runner-core-dev.cjs"), ...arguments_],
    request,
  );
  assert.equal(devTerminal.category, "unknown");
  const ordinaryTerminal = await exchangeTerminal(
    [path.resolve("dist/agent-runner.cjs"), ...arguments_],
    request,
  );
  assert.equal(ordinaryTerminal.category, "invalid_request");
  for (const coreDevProxy of [
    "http://127.0.0.1:37174",
    "http://0.0.0.0:37173",
    "http://example.test:37173",
    "https://127.0.0.1:37173",
  ]) {
    const terminal = await exchangeTerminal(
      [path.resolve("dist/agent-runner-core-dev.cjs"), ...arguments_],
      { ...request, coreDevProxy },
    );
    assert.equal(terminal.category, "invalid_request");
  }

  let agentPresent = false;
  const proxied: typeof globalThis.fetch = (input, init) =>
    fixedCoreDevProxyFetch(input, init, async (_input, init) => {
      agentPresent = Boolean((init as NodeFetchRequestInit | undefined)?.agent);
      return new NodeFetchResponse(null, { status: 204 });
    });
  await proxied("https://openrouter.ai/api/v1/chat/completions", {
    method: "POST",
    body: "synthetic-body-never-log",
  });
  assert.equal(agentPresent, true);
  assert.throws(
    () => proxied("https://api.openai.com/v1/chat/completions"),
    /invalid Pi runner request/,
  );
});

test("the shared JITless transport preserves requests and exposes a Web response stream", async () => {
  let receivedUrl = "";
  let receivedInit: NodeFetchRequestInit | undefined;
  const response = await nodeProviderFetchWithOptions(
    new Request("https://openrouter.ai/api/v1/chat/completions", {
      method: "POST",
      headers: { authorization: "Bearer synthetic-never-log" },
      body: "synthetic-body-never-log",
    }),
    undefined,
    { redirect: "error" },
    async (url, init) => {
      receivedUrl = url;
      receivedInit = init;
      return new NodeFetchResponse("data: synthetic-stream\n\n", {
        status: 200,
        headers: { "content-type": "text/event-stream" },
      });
    },
  );
  assert.equal(receivedUrl, "https://openrouter.ai/api/v1/chat/completions");
  assert.equal(receivedInit?.method, "POST");
  assert.equal(receivedInit?.redirect, "error");
  assert.equal(Buffer.from(receivedInit?.body as Buffer).toString(), "synthetic-body-never-log");
  assert.equal(response.headers.get("content-type"), "text/event-stream");
  assert.equal(typeof response.body?.getReader, "function");
  assert.equal(await response.text(), "data: synthetic-stream\n\n");
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

test("structured resolver, connect, TLS, and unknown failures have distinct safe categories", () => {
  const secret = "sk-or-v1-never-emit-this";
  const dns = sanitizeRunnerFailure(
    Object.assign(new Error(`provider-controlled ${secret}`), { code: "EAI_AGAIN" }),
    PINNED_OPENROUTER_MODEL,
    "dns",
  );
  const connection = sanitizeRunnerFailure(
    new TypeError("fetch failed", { cause: { code: "ECONNREFUSED", secret } }),
    PINNED_OPENROUTER_MODEL,
  );
  const tls = sanitizeRunnerFailure(
    Object.assign(new Error(`malicious ENOTFOUND ${secret}`), {
      code: "UNABLE_TO_VERIFY_LEAF_SIGNATURE",
    }),
    PINNED_OPENROUTER_MODEL,
  );
  const unknown = sanitizeRunnerFailure(
    new Error(`provider body says getaddrinfo EAI_AGAIN ECONNREFUSED ${secret}`),
    PINNED_OPENROUTER_MODEL,
  );
  assert.deepEqual(
    [dns.category, connection.category, tls.category, unknown.category],
    ["dns_resolution", "connect_refused", "tls_failure", "unknown"],
  );
  assert.ok(!JSON.stringify(dns).includes(secret));
  assert.ok(!JSON.stringify(connection).includes(secret));
  assert.ok(!JSON.stringify(tls).includes(secret));
  assert.ok(!JSON.stringify(unknown).includes(secret));
});

test("network failure codes and HTTP status are exhaustively classified without content", () => {
  const cases = [
    ["dns", "ETIMEDOUT", "dns_timeout"],
    ["dns", "EPERM", "dns_proxy_unavailable"],
    ["provider", "ENOTFOUND", "dns_resolution"],
    ["provider", "UND_ERR_CONNECT_TIMEOUT", "connect_timeout"],
    ["provider", "ECONNREFUSED", "connect_refused"],
    ["provider", "ECONNRESET", "connect_reset"],
    ["provider", "ENETUNREACH", "network_unreachable"],
    ["provider", "ERR_TLS_CERT_ALTNAME_INVALID", "tls_failure"],
  ] as const;
  for (const [context, code, category] of cases) {
    assert.equal(
      sanitizeRunnerFailure({ code, message: "attacker text" }, PINNED_OPENROUTER_MODEL, context)
        .category,
      category,
    );
  }
  assert.deepEqual(
    [400, 401, 403, 429, 503].map((status) => {
      const failure = sanitizeRunnerFailure({ status, body: "untrusted" });
      return [failure.category, failure.status];
    }),
    [
      ["provider_rejected", 400],
      ["credential_rejected", 401],
      ["credential_rejected", 403],
      ["rate_limited", 429],
      ["provider_unavailable", 503],
    ],
  );
});

test("the fetch observer retains only structured transport or numeric HTTP evidence", async () => {
  const observation: { failure?: ReturnType<typeof sanitizeRunnerFailure> } = {};
  const observed = observeProviderFetch(
    PINNED_OPENROUTER_MODEL,
    observation,
    async () => new Response("malicious provider body ENOTFOUND", { status: 429 }),
  );
  await observed("https://openrouter.ai/api/v1/chat/completions");
  assert.equal(observation.failure?.category, "rate_limited");
  assert.equal(observation.failure?.status, 429);
  assert.ok(!JSON.stringify(observation).includes("malicious"));
});

test("an r11-style invisible DNS failure now has exactly one bounded terminal envelope", () => {
  const terminal = stdioFailureEnvelope(
    Object.assign(new Error("visible UI text is not protocol evidence"), { code: "EAI_AGAIN" }),
  );
  assert.equal(terminal.protocol_version, 2);
  assert.equal(terminal.terminal, "failed");
  assert.equal(terminal.category, "dns_resolution");
  assert.deepEqual(Object.keys(terminal).sort(), [
    "category",
    "error",
    "model",
    "protocol_version",
    "stage",
    "terminal",
    "type",
  ]);
});

test("OpenRouter DNS startup fails before provider or tool-sequence processing", async () => {
  const failure = await preflightProviderDns(
    "openrouter",
    PINNED_OPENROUTER_MODEL,
    async () => {
      throw Object.assign(new Error("untrusted detail"), { code: "EPERM" });
    },
  );
  assert.deepEqual(failure, {
    type: "error",
    stage: "transport",
    category: "dns_proxy_unavailable",
    error: "The Android DNS proxy was unavailable.",
    model: PINNED_OPENROUTER_MODEL,
  });
});

test("OpenRouter DNS startup uses exactly the fixed hostname and continues on success", async () => {
  const hostnames: string[] = [];
  const failure = await preflightProviderDns(
    "openrouter",
    PINNED_OPENROUTER_MODEL,
    async (hostname) => hostnames.push(hostname),
  );
  assert.equal(failure, undefined);
  assert.deepEqual(hostnames, ["openrouter.ai"]);

  await preflightProviderDns("openai", "unused", async (hostname) => hostnames.push(hostname));
  assert.deepEqual(hostnames, ["openrouter.ai"]);
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

function exchangeTerminal(arguments_: string[], request: unknown): Promise<Record<string, unknown>> {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, arguments_, { stdio: ["pipe", "pipe", "pipe"] });
    let stdout = "";
    child.stdout.setEncoding("utf8").on("data", (chunk: string) => (stdout += chunk));
    child.once("error", reject);
    child.once("close", () => {
      const line = stdout.trim().split("\n").at(-1);
      if (!line) {
        reject(new Error("runner returned no terminal response"));
        return;
      }
      resolve(JSON.parse(line) as Record<string, unknown>);
    });
    child.stdin.end(JSON.stringify(request));
  });
}
