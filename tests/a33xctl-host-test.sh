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

printf 'compat artifact\n' >"$test_root/compat.ota.zip"
printf 'core artifact\n' >"$test_root/core.ota.zip"
for product in compat1 core1; do
  revision="sos.$product.test.revision"
  artifact="$test_root/${product%1}.ota.zip"
  campaign="$test_root/$product-campaign"
  for stage in stock dashboard appearance child-failure child-timeout recovered \
    ime-accessibility host-restart authority-restart authored rollback; do
    A33XCTL_ADB="$mock_adb" "$ctl" capture-v4-composition-stage \
      --product "$product" \
      --serial MOCKSERIAL \
      --expected-revision "$revision" \
      --artifact "$artifact" \
      --root "$campaign" \
      --stage "$stage" \
      >"$test_root/$product-$stage.out"
  done
  "$ctl" audit-v4-composition-campaign --root "$campaign" \
    >"$test_root/$product-audit.out"
  grep -Fx 'v4_composition_campaign=PASS' "$test_root/$product-audit.out" >/dev/null
  "$ctl" evidence-manifest-verify --root "$campaign" \
    --manifest "$campaign/manifest.tsv" >/dev/null
done

cp -a "$test_root/compat1-campaign" "$test_root/bad-pid-campaign"
sed -i 's/^authority_pid=400$/authority_pid=300/' \
  "$test_root/bad-pid-campaign/stages/authority-restart/pids.env"
if "$ctl" audit-v4-composition-campaign \
  --root "$test_root/bad-pid-campaign" >/dev/null 2>&1; then
  printf 'non-isolated authority recovery unexpectedly passed\n' >&2
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
