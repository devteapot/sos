import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import type { Credential, OAuthCredential } from "@earendil-works/pi-ai";
import { JsonCredentialStore } from "../src/auth.js";
import { createProviderModels } from "../src/runtime.js";

function oauth(generation: number): OAuthCredential {
  return {
    type: "oauth",
    access: `access-${generation}`,
    refresh: `refresh-${generation}`,
    expires: 2_000_000_000_000,
    generation,
  };
}

function generation(credential: Credential | undefined): number {
  return credential?.type === "oauth" && typeof credential.generation === "number"
    ? credential.generation
    : 0;
}

test("JSON credentials persist atomically without exposing secrets in metadata", async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), "sos-agent-auth-"));
  const credentialPath = path.join(directory, "auth.json");
  try {
    const store = new JsonCredentialStore(credentialPath);
    await store.modify("openai-codex", async () => oauth(1));

    assert.deepEqual(await store.read("openai-codex"), oauth(1));
    assert.deepEqual(await store.list(), [{ providerId: "openai-codex", type: "oauth" }]);
    assert.equal((await fs.stat(credentialPath)).mode & 0o777, 0o600);
    assert.deepEqual(JSON.parse(await fs.readFile(credentialPath, "utf8")), {
      "openai-codex": oauth(1),
    });

    await store.delete("openai-codex");
    assert.equal(await store.read("openai-codex"), undefined);
  } finally {
    await fs.rm(directory, { recursive: true });
  }
});

test("separate store instances serialize refresh-style read-modify-write operations", async () => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), "sos-agent-auth-lock-"));
  const credentialPath = path.join(directory, "auth.json");
  try {
    const first = new JsonCredentialStore(credentialPath);
    const second = new JsonCredentialStore(credentialPath);
    await first.modify("openai-codex", async () => oauth(0));

    await Promise.all(
      [first, second, first, second].map((store) =>
        store.modify("openai-codex", async (current) => {
          await new Promise((resolve) => setTimeout(resolve, 10));
          return oauth(generation(current) + 1);
        }),
      ),
    );

    assert.equal(generation(await first.read("openai-codex")), 4);
    assert.deepEqual(
      (await fs.readdir(directory)).filter((entry) => entry.includes(".lock")),
      [],
    );
  } finally {
    await fs.rm(directory, { recursive: true });
  }
});

test("the SOS runtime registers Pi's subscription-backed Codex provider", () => {
  const models = createProviderModels("openai-codex");
  const provider = models.getProvider("openai-codex");

  assert.equal(provider?.auth.oauth?.isSubscription, true);
  assert.ok(models.getModel("openai-codex", "gpt-5.6-sol"));
});
