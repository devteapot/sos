import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  canonicalJson,
  decodeCanonicalPackageV4,
  decodeCanonicalResolvedGraphV1,
  decodeExperienceWireFixtureV1,
} from "../src/wire-model.js";

const fixtureText = readFileSync(
  new URL("../../../../tests/fixtures/experience-wire-v4.json", import.meta.url),
  "utf8",
);
const expected = JSON.parse(
  readFileSync(
    new URL("../../../../tests/fixtures/experience-wire-v4.expected.json", import.meta.url),
    "utf8",
  ),
) as Record<string, string>;

function sha256(value: string) {
  return createHash("sha256").update(value).digest("hex");
}

test("TypeScript decodes the shared v4 wire fixture with Rust identities", () => {
  const fixture = decodeExperienceWireFixtureV1(fixtureText);
  const contract = fixture.package.contract;
  assert.equal(sha256(canonicalJson(contract)), expected.contract_digest);
  assert.equal(sha256(canonicalJson(fixture.package)), expected.package_sha256);
  assert.equal(sha256(canonicalJson(fixture.appearance)), expected.appearance_sha256);
  assert.equal(sha256(canonicalJson(fixture.graph)), expected.graph_id);
  assert.equal(
    decodeCanonicalPackageV4(canonicalJson(fixture.package)).experience_id,
    "sos.reference.dashboard",
  );
  assert.equal(
    decodeCanonicalResolvedGraphV1(canonicalJson(fixture.graph)).root,
    "dashboard-main",
  );
});

test("TypeScript rejects non-canonical, unknown, and oversized package data", () => {
  const fixture = decodeExperienceWireFixtureV1(fixtureText);
  const canonical = canonicalJson(fixture.package);
  assert.throws(() => decodeCanonicalPackageV4(`${canonical}\n`), /not canonical/);
  assert.throws(
    () => decodeCanonicalPackageV4(canonicalJson({ ...fixture.package, unknown: true })),
    /unknown field/,
  );
  assert.throws(() => decodeCanonicalPackageV4(" ".repeat(256 * 1024 + 1)), /exceeds/);
});

test("TypeScript canonical JSON matches JCS number and UTF-16 key ordering", () => {
  assert.equal(
    canonicalJson([333333333.33333329, 1E30, 4.50, 2e-3, 1e-27]),
    "[333333333.3333333,1e+30,4.5,0.002,1e-27]",
  );
  assert.equal(
    canonicalJson({ "\ufb33": 7, "😀": 6, "€": 5, "ö": 4, "\u0080": 3, "1": 2, "\r": 1 }),
    "{\"\\r\":1,\"1\":2,\"\":3,\"ö\":4,\"€\":5,\"😀\":6,\"דּ\":7}",
  );
});
