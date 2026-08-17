#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ctl="$repo_root/tools/a33xctl"
mock_adb="$repo_root/tests/fixtures/a33xctl-mock-adb"
test_root="$(mktemp -d /tmp/sos-a33xctl-host-test.XXXXXX)"
trap 'rm -rf -- "$test_root"' EXIT

A33XCTL_ADB="$mock_adb" "$ctl" inspect-core1-readiness \
  --serial MOCKSERIAL \
  --expected-revision sos.core1.test.revision \
  >"$test_root/readiness.out"
grep -Fx 'core1_readiness=PASS' "$test_root/readiness.out" >/dev/null
grep -Fx 'native_lifecycle=PASS' "$test_root/readiness.out" >/dev/null
if A33XCTL_ADB="$mock_adb" "$ctl" inspect-core1-readiness \
  --serial MOCKSERIAL --expected-revision sos.core1.wrong >/dev/null 2>&1; then
  printf 'wrong Core 1 revision unexpectedly passed\n' >&2
  exit 1
fi

mkdir "$test_root/evidence"
printf 'bravo\n' >"$test_root/evidence/b.txt"
printf 'alpha\n' >"$test_root/evidence/a.txt"
printf 'unfinished\n' >"$test_root/evidence/ignored.partial"
manifest="$test_root/evidence/final-manifest.tsv"
"$ctl" evidence-manifest-generate --root "$test_root/evidence" --output "$manifest" >/dev/null
[[ "$(cut -f1 "$manifest")" == $'a.txt\nb.txt' ]]
first_identity="$(sha256sum "$manifest")"
"$ctl" evidence-manifest-verify --root "$test_root/evidence" --manifest "$manifest" >/dev/null
"$ctl" evidence-manifest-generate --root "$test_root/evidence" --output "$manifest" >/dev/null
[[ "$(sha256sum "$manifest")" == "$first_identity" ]]

printf 'changed\n' >>"$test_root/evidence/a.txt"
if "$ctl" evidence-manifest-verify --root "$test_root/evidence" \
  --manifest "$manifest" >/dev/null 2>&1; then
  printf 'modified evidence unexpectedly passed manifest verification\n' >&2
  exit 1
fi

printf 'a33xctl_host_test=PASS\n'
