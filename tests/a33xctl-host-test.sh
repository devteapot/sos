#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ctl="$repo_root/tools/a33xctl"
mock_adb="$repo_root/tests/fixtures/a33xctl-mock-adb"
test_root="$(mktemp -d /tmp/sos-a33xctl-host-test.XXXXXX)"
trap 'rm -rf -- "$test_root"' EXIT
export A33XCTL_MOCK_INVOCATION_LOG="$test_root/adb-invocations.log"
export A33XCTL_MOCK_CREDENTIAL_STATE="$test_root/core-dev-state"
: >"$A33XCTL_MOCK_INVOCATION_LOG"
printf 'EMPTY\n' >"$A33XCTL_MOCK_CREDENTIAL_STATE"

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

# Exercise the exact target-files SYSTEM-property validator used by both Core
# inspectors. Every call runs under this script's strict nounset mode.
# shellcheck source=../tools/a33x/core-system-properties.sh
source "$repo_root/tools/a33x/core-system-properties.sh"

for variant in ordinary dev-credential; do
  case_root="$test_root/SYSTEM property $variant"
  target_files="$case_root/target files"
  temp_parent="$case_root/temporary files"
  mkdir -p "$target_files/SYSTEM" "$temp_parent"

  for invalid in missing empty weakened; do
    rm -f -- "$target_files/SYSTEM/build.prop"
    case "$invalid" in
      missing) ;;
      empty) : >"$target_files/SYSTEM/build.prop" ;;
      weakened)
        printf 'ro.build.type=userdebug\nro.debuggable=1\n' \
          >"$target_files/SYSTEM/build.prop"
        ;;
    esac
    if TMPDIR="$temp_parent" inspect_core_system_properties \
        "$target_files" core1 "$variant" >"$case_root/$invalid.out" \
        2>"$case_root/$invalid.err"; then
      printf '%s Core inspector accepted %s SYSTEM properties\n' \
        "$variant" "$invalid" >&2
      exit 1
    fi
    if [[ "$invalid" == missing || "$invalid" == empty ]]; then
      grep -F 'SYSTEM property data is missing or empty in target-files' \
        "$case_root/$invalid.err" >/dev/null
    else
      grep -F 'image weakened Lineage' "$case_root/$invalid.err" >/dev/null
    fi
    [[ -z "$(find "$temp_parent" -mindepth 1 -print -quit)" ]]
  done

  if [[ "$variant" == dev-credential ]]; then
    printf 'ro.build.type=user\nro.debuggable=0\n' \
      >"$target_files/SYSTEM/build.prop"
    if TMPDIR="$temp_parent" inspect_core_system_properties \
        "$target_files" core1 "$variant" >"$case_root/wrong-type.out" \
        2>"$case_root/wrong-type.err"; then
      printf 'Core-dev inspector accepted the wrong SYSTEM build type\n' >&2
      exit 1
    fi
    grep -F 'not the registered userdebug product' \
      "$case_root/wrong-type.err" >/dev/null
    [[ -z "$(find "$temp_parent" -mindepth 1 -print -quit)" ]]
  fi

  printf 'ro.build.type=userdebug\nro.debuggable=0\n' \
    >"$target_files/SYSTEM/build.prop"
  TMPDIR="$temp_parent" inspect_core_system_properties \
    "$target_files" core1 "$variant"
  [[ -z "$(find "$temp_parent" -mindepth 1 -print -quit)" ]]
done

A33XCTL_MOCK_PROBE_RESPONSE=lf A33XCTL_ADB="$mock_adb" \
  "$ctl" inspect-core1-readiness \
  --serial MOCKSERIAL \
  --expected-revision sos.core1dev.0123456789ab.cdef01234567 \
  >"$test_root/readiness.out"
grep -Fx 'core1_readiness=PASS' "$test_root/readiness.out" >/dev/null
grep -Fx 'native_lifecycle=PASS' "$test_root/readiness.out" >/dev/null

# Reproduce the observed r10 distinction explicitly: exec-out exits zero but
# exposes no bytes, while the production CLI's no-PTY shell transport carries
# the exact framed status.
A33XCTL_MOCK_ALLOW_EMPTY_EXEC_OUT=1 A33XCTL_MOCK_INVOCATION_LOG= \
  "$mock_adb" -s MOCKSERIAL exec-out \
  /system_ext/bin/sos-core-dev-credential probe </dev/null \
  >"$test_root/exec-out-empty.out" 2>"$test_root/exec-out-empty.err"
[[ ! -s "$test_root/exec-out-empty.out" && ! -s "$test_root/exec-out-empty.err" ]]

# The deployed r10 client emits the exact LF-terminated READY line. Exercise
# that byte framing through readiness above, permit only its CRLF transport
# equivalent, and reject every missing/ambiguous/binary variant.
A33XCTL_MOCK_PROBE_RESPONSE=crlf A33XCTL_ADB="$mock_adb" \
  "$ctl" inspect-core1-readiness --serial MOCKSERIAL \
  --expected-revision sos.core1dev.0123456789ab.cdef01234567 \
  >"$test_root/readiness-crlf.out"
grep -Fx 'core1_readiness=PASS' "$test_root/readiness-crlf.out" >/dev/null
for response_case in missing-newline double-newline prefix suffix whitespace \
    extra-output stderr-output nul wrong-status nonzero; do
  if A33XCTL_MOCK_PROBE_RESPONSE="$response_case" \
      A33XCTL_ADB="$mock_adb" "$ctl" inspect-core1-readiness \
      --serial MOCKSERIAL \
      --expected-revision sos.core1dev.0123456789ab.cdef01234567 \
      >"$test_root/probe-$response_case.out" \
      2>"$test_root/probe-$response_case.err"; then
    printf 'ambiguous Core-dev probe output passed: %s\n' "$response_case" >&2
    exit 1
  fi
  grep -F 'Core-dev probe failed: endpoint.protocol expected=v1 actual=mismatch' \
    "$test_root/probe-$response_case.err" >/dev/null
done
if A33XCTL_MOCK_SHELL_T_UNSUPPORTED=1 A33XCTL_ADB="$mock_adb" \
    "$ctl" inspect-core1-readiness --serial MOCKSERIAL \
    --expected-revision sos.core1dev.0123456789ab.cdef01234567 \
    >"$test_root/probe-shell-t-unsupported.out" \
    2>"$test_root/probe-shell-t-unsupported.err"; then
  printf 'Core-dev readiness passed without shell -T support\n' >&2
  exit 1
fi
grep -F 'Core-dev probe failed: ADB transport requires shell -T support' \
  "$test_root/probe-shell-t-unsupported.err" >/dev/null

grep -F 'property_is("ro.debuggable", "1")' \
  "$repo_root/apps/experience/src/core_dev_credential.rs" \
  "$repo_root/apps/experience/src/core_dev_product.rs" >/dev/null && {
  printf 'Core-dev tooling still requires broad Android debugging\n' >&2
  exit 1
}
if A33XCTL_ADB="$mock_adb" "$ctl" inspect-core1-readiness \
  --serial MOCKSERIAL --expected-revision sos.core1.wrong >/dev/null 2>&1; then
  printf 'wrong Core 1 revision unexpectedly passed\n' >&2
  exit 1
fi

A33XCTL_ADB="$mock_adb" "$ctl" inspect-core1-readiness \
  --serial MOCKORDINARY \
  --expected-revision sos.core1.0123456789ab.cdef01234567 \
  >"$test_root/ordinary-readiness.out"
grep -Fx 'core1_readiness=PASS' "$test_root/ordinary-readiness.out" >/dev/null

A33XCTL_MOCK_PROBE_RESPONSE=crlf A33XCTL_MOCK_CLEAR_RESPONSE=crlf \
  A33XCTL_ADB="$mock_adb" "$ctl" core1-dev-clear-openrouter-key \
  --serial MOCKSERIAL >"$test_root/dev-clear.out"
grep -Fx 'core1_dev_openrouter_key=CLEARED' "$test_root/dev-clear.out" >/dev/null
grep -Fx 'serial=MOCKSERIAL' "$test_root/dev-clear.out" >/dev/null
grep -Fx 'shell -T -- /system_ext/bin/sos-core-dev-credential status' \
  "$A33XCTL_MOCK_INVOCATION_LOG" >/dev/null
if A33XCTL_MOCK_STATUS_RESPONSE=configured A33XCTL_ADB="$mock_adb" \
    "$ctl" core1-dev-clear-openrouter-key --serial MOCKSERIAL \
    >"$test_root/dev-clear-state-mismatch.out" \
    2>"$test_root/dev-clear-state-mismatch.err"; then
  printf 'Core-dev clear passed without EMPTY status\n' >&2
  exit 1
fi
grep -F 'clear status mismatch: expected=EMPTY actual=CONFIGURED' \
  "$test_root/dev-clear-state-mismatch.err" >/dev/null
if A33XCTL_MOCK_CLEAR_RESPONSE=extra-output A33XCTL_ADB="$mock_adb" \
    "$ctl" core1-dev-clear-openrouter-key --serial MOCKSERIAL \
    >"$test_root/dev-clear-extra.out" 2>"$test_root/dev-clear-extra.err"; then
  printf 'ambiguous Core-dev clear output passed\n' >&2
  exit 1
fi
grep -F 'Core returned an invalid development credential acknowledgement' \
  "$test_root/dev-clear-extra.err" >/dev/null
run_mock_set() {
  local response="$1" output="$2" shell_mode="${3:-normal}"
  A33XCTL_MOCK_SET_RESPONSE="$response" python3 - \
    "$ctl" "$mock_adb" "$response" "$shell_mode" >"$output" 2>&1 <<'PY'
import errno, os, pty, sys
pid, fd = pty.fork()
if pid == 0:
    env = dict(os.environ, A33XCTL_ADB=sys.argv[2])
    if sys.argv[4] == "xtrace":
        executable = "/usr/bin/bash"
        argv = ["bash", "-x", sys.argv[1], "core1-dev-set-openrouter-key", "--serial", "MOCKSERIAL"]
    else:
        executable = sys.argv[1]
        argv = [sys.argv[1], "core1-dev-set-openrouter-key", "--serial", "MOCKSERIAL"]
    os.execve(executable, argv, env)
output = b""
prompt = b"Paste the disposable OpenRouter key, then press Enter: "
while prompt not in output:
    output += os.read(fd, 4096)
if sys.argv[4] == "overlong":
    os.write(fd, b"x" * 513 + b"\n")
else:
    os.write(fd, b"mock-non-secret\n")
while True:
    try:
        output += os.read(fd, 4096)
    except OSError as error:
        if error.errno != errno.EIO:
            raise
        break
_, status = os.waitpid(pid, 0)
sys.stdout.buffer.write(output.replace(b"\r", b""))
raise SystemExit(os.waitstatus_to_exitcode(status))
PY
}

run_mock_set crlf "$test_root/dev-set.out" xtrace
grep -Fx 'core1_dev_openrouter_key=SET' "$test_root/dev-set.out" >/dev/null
grep -Fx 'serial=MOCKSERIAL' "$test_root/dev-set.out" >/dev/null
! grep -F 'mock-non-secret' "$test_root/dev-set.out" >/dev/null
[[ "$(<"$A33XCTL_MOCK_CREDENTIAL_STATE")" == CONFIGURED ]]
A33XCTL_ADB="$mock_adb" "$ctl" core1-dev-status-openrouter-key \
  --serial MOCKSERIAL >"$test_root/dev-status-configured.out"
grep -Fx 'core1_dev_openrouter_key=CONFIGURED' \
  "$test_root/dev-status-configured.out" >/dev/null
grep -Fx 'serial=MOCKSERIAL' "$test_root/dev-status-configured.out" >/dev/null
A33XCTL_MOCK_SMOKE_RESPONSE=crlf A33XCTL_ADB="$mock_adb" \
  "$ctl" core1-dev-submit-agent-smoke --serial MOCKSERIAL \
  >"$test_root/dev-agent-smoke.out"
grep -Fx 'core1_dev_agent_smoke=SUBMITTED' \
  "$test_root/dev-agent-smoke.out" >/dev/null
grep -Fx 'serial=MOCKSERIAL' "$test_root/dev-agent-smoke.out" >/dev/null
[[ "$(<"$A33XCTL_MOCK_CREDENTIAL_STATE")" == CONFIGURED ]]
grep -Fx 'shell -T -- /system_ext/bin/sos-core-dev-credential agent-smoke' \
  "$A33XCTL_MOCK_INVOCATION_LOG" >/dev/null

A33XCTL_ADB="$mock_adb" "$ctl" core1-dev-run-agent-smoke \
  --serial MOCKSERIAL >"$test_root/dev-agent-tunnel-smoke.out"
grep -Fx 'core1_dev_agent_smoke=COMPLETED' \
  "$test_root/dev-agent-tunnel-smoke.out" >/dev/null
tunnel_evidence="$(sed -n 's/^evidence_root=//p' "$test_root/dev-agent-tunnel-smoke.out")"
case "$tunnel_evidence" in
  "$repo_root/artifacts/device-gates/core-dev-smoke-"*) ;;
  *) fail "Core-dev smoke returned an unsafe evidence path" ;;
esac
[[ -s "$tunnel_evidence/framebuffer.png" ]]
grep -F 'transport=adb_reverse_connect' \
  "$tunnel_evidence/safe-lifecycle.txt" >/dev/null
grep -F 'core_ui_attempt event=terminal' \
  "$tunnel_evidence/safe-lifecycle.txt" >/dev/null
grep -E 'android_agent_activation_commit .*phase=committed authority=system' \
  "$tunnel_evidence/safe-lifecycle.txt" >/dev/null
grep -Fx 'core1_dev_openrouter_key=CONFIGURED' \
  "$tunnel_evidence/status-after.txt" >/dev/null
"$ctl" evidence-manifest-verify --root "$tunnel_evidence" \
  --manifest "$tunnel_evidence/manifest.tsv" >/dev/null
grep -E '^reverse tcp:37173 tcp:[0-9]+$' \
  "$A33XCTL_MOCK_INVOCATION_LOG" >/dev/null
grep -Fx 'reverse --remove tcp:37173' "$A33XCTL_MOCK_INVOCATION_LOG" >/dev/null
[[ "$(<"$A33XCTL_MOCK_CREDENTIAL_STATE")" == CONFIGURED ]]
rm -rf -- "$tunnel_evidence"

if A33XCTL_MOCK_SMOKE_RESPONSE=extra-output A33XCTL_ADB="$mock_adb" \
    "$ctl" core1-dev-run-agent-smoke --serial MOCKSERIAL \
    >"$test_root/dev-agent-tunnel-failure.out" \
    2>"$test_root/dev-agent-tunnel-failure.err"; then
  fail "Core-dev tunnel smoke accepted a rejected submit"
fi
failed_tunnel_evidence="$(sed -n 's/^evidence_root=//p' \
  "$test_root/dev-agent-tunnel-failure.out")"
case "$failed_tunnel_evidence" in
  "$repo_root/artifacts/device-gates/core-dev-smoke-"*) ;;
  *) fail "failed Core-dev smoke returned an unsafe evidence path" ;;
esac
grep -Fx 'core1_dev_openrouter_key=CONFIGURED' \
  "$failed_tunnel_evidence/status-after.txt" >/dev/null
grep -Fx 'reverse --remove tcp:37173' "$A33XCTL_MOCK_INVOCATION_LOG" >/dev/null
[[ "$(<"$A33XCTL_MOCK_CREDENTIAL_STATE")" == CONFIGURED ]]
rm -rf -- "$failed_tunnel_evidence"

signal_marker="$test_root/signal-started"
: >"$signal_marker"
A33XCTL_MOCK_SMOKE_NO_TERMINAL=1 A33XCTL_ADB="$mock_adb" \
  "$ctl" core1-dev-run-agent-smoke --serial MOCKSERIAL \
  >"$test_root/dev-agent-tunnel-signal.out" \
  2>"$test_root/dev-agent-tunnel-signal.err" &
signal_pid=$!
for _ in {1..50}; do
  signal_evidence="$(find "$repo_root/artifacts/device-gates" -maxdepth 1 \
    -type d -name 'core-dev-smoke-*-mockserial' -newer "$signal_marker" \
    -print -quit)"
  [[ -n "$signal_evidence" && -f "$signal_evidence/reverse-setup.txt" ]] && break
  sleep 0.02
done
[[ -n "${signal_evidence:-}" ]]
kill -TERM "$signal_pid"
if wait "$signal_pid"; then
  fail "signaled Core-dev smoke unexpectedly succeeded"
fi
grep -Fx 'reverse --remove tcp:37173' "$A33XCTL_MOCK_INVOCATION_LOG" >/dev/null
[[ "$(<"$A33XCTL_MOCK_CREDENTIAL_STATE")" == CONFIGURED ]]
rm -rf -- "$signal_evidence"
if A33XCTL_MOCK_SMOKE_RESPONSE=extra-output A33XCTL_ADB="$mock_adb" \
    "$ctl" core1-dev-submit-agent-smoke --serial MOCKSERIAL \
    >"$test_root/dev-agent-smoke-extra.out" \
    2>"$test_root/dev-agent-smoke-extra.err"; then
  printf 'ambiguous Core-dev smoke output passed\n' >&2
  exit 1
fi
grep -F 'Core rejected the fixed development agent smoke submit' \
  "$test_root/dev-agent-smoke-extra.err" >/dev/null
[[ "$(<"$A33XCTL_MOCK_CREDENTIAL_STATE")" == CONFIGURED ]]
printf 'EMPTY\n' >"$A33XCTL_MOCK_CREDENTIAL_STATE"
if A33XCTL_ADB="$mock_adb" "$ctl" core1-dev-submit-agent-smoke \
    --serial MOCKSERIAL >"$test_root/dev-agent-smoke-empty.out" \
    2>"$test_root/dev-agent-smoke-empty.err"; then
  printf 'Core-dev smoke passed without configured credential\n' >&2
  exit 1
fi
grep -F 'smoke requires credential state CONFIGURED; actual=EMPTY' \
  "$test_root/dev-agent-smoke-empty.err" >/dev/null
printf 'CONFIGURED\n' >"$A33XCTL_MOCK_CREDENTIAL_STATE"
if A33XCTL_MOCK_STATUS_RESPONSE=empty \
    run_mock_set lf "$test_root/dev-set-state-mismatch.out"; then
  printf 'Core-dev set passed without CONFIGURED status\n' >&2
  exit 1
fi
grep -F 'set status mismatch: expected=CONFIGURED actual=EMPTY' \
  "$test_root/dev-set-state-mismatch.out" >/dev/null
if run_mock_set extra-output "$test_root/dev-set-extra.out"; then
  printf 'ambiguous Core-dev set output passed\n' >&2
  exit 1
fi
grep -F 'credential state is unknown, so run core1-dev-clear-openrouter-key' \
  "$test_root/dev-set-extra.out" >/dev/null
set_invocations_before="$(grep -Fc \
  'shell -T -- /system_ext/bin/sos-core-dev-credential set' \
  "$A33XCTL_MOCK_INVOCATION_LOG")"
if run_mock_set lf "$test_root/dev-set-overlong.out" overlong; then
  printf 'overlong Core-dev credential input passed\n' >&2
  exit 1
fi
grep -F 'credential input exceeds 512 bytes' \
  "$test_root/dev-set-overlong.out" >/dev/null
! grep -F 'xxxxxxxx' "$test_root/dev-set-overlong.out" >/dev/null
set_invocations_after="$(grep -Fc \
  'shell -T -- /system_ext/bin/sos-core-dev-credential set' \
  "$A33XCTL_MOCK_INVOCATION_LOG")"
[[ "$set_invocations_after" == "$set_invocations_before" ]]
A33XCTL_MOCK_STATUS_RESPONSE=crlf A33XCTL_ADB="$mock_adb" \
  "$ctl" core1-dev-status-openrouter-key --serial MOCKSERIAL \
  >"$test_root/dev-status-crlf.out"
grep -Fx 'core1_dev_openrouter_key=CONFIGURED' \
  "$test_root/dev-status-crlf.out" >/dev/null
for response_case in missing-newline double-newline prefix suffix whitespace \
    extra-output stderr-output nul wrong-status nonzero; do
  if A33XCTL_MOCK_STATUS_RESPONSE="$response_case" A33XCTL_ADB="$mock_adb" \
      "$ctl" core1-dev-status-openrouter-key --serial MOCKSERIAL \
      >"$test_root/dev-status-$response_case.out" \
      2>"$test_root/dev-status-$response_case.err"; then
    printf 'ambiguous Core-dev status output passed: %s\n' "$response_case" >&2
    exit 1
  fi
  grep -F 'Core returned an invalid development credential status' \
    "$test_root/dev-status-$response_case.err" >/dev/null
done
if A33XCTL_ADB="$mock_adb" "$ctl" core1-dev-clear-openrouter-key \
  --serial MOCKORDINARY >/dev/null 2>&1; then
  printf 'ordinary Core unexpectedly accepted the development credential client\n' >&2
  exit 1
fi

for rejection in \
  'MOCKWRONGREV|ro.build.version.incremental' \
  'MOCKWRONGTYPE|ro.build.type' \
  'MOCKWRONGMARKER|ro.sos.build_variant'; do
  serial="${rejection%%|*}"
  marker="${rejection#*|}"
  revision=sos.core1dev.0123456789ab.cdef01234567
  [[ "$serial" != MOCKWRONGREV ]] || revision=sos.core1dev.not-a-digest.cdef01234567
  if A33XCTL_ADB="$mock_adb" "$ctl" inspect-core1-readiness \
      --serial "$serial" --expected-revision "$revision" \
      >"$test_root/$serial.out" 2>"$test_root/$serial.err"; then
    printf 'invalid Core-dev contract unexpectedly passed readiness: %s\n' "$serial" >&2
    exit 1
  fi
  grep -F "Core product marker mismatch: $marker expected=" \
    "$test_root/$serial.err" >/dev/null
  if A33XCTL_ADB="$mock_adb" "$ctl" core1-dev-clear-openrouter-key \
      --serial "$serial" >"$test_root/$serial-clear.out" \
      2>"$test_root/$serial-clear.err"; then
    printf 'invalid Core-dev contract unexpectedly passed clear: %s\n' "$serial" >&2
    exit 1
  fi
  grep -F "Core product marker mismatch: $marker expected=" \
    "$test_root/$serial-clear.err" >/dev/null
done

for rejection in \
  'MOCKMISSINGCLIENT|client.executable expected=present actual=missing' \
  'MOCKDENIEDCLIENT|client.execution expected=allowed actual=selinux-denied' \
  'MOCKENDPOINTDOWN|endpoint.availability expected=ready actual=unavailable' \
  'MOCKWRONGPEER|endpoint.peer_product expected=accepted actual=rejected' \
  'MOCKPROTOCOL|endpoint.status expected=v1-known actual=mismatch' \
  'MOCKBADMAGIC|endpoint.magic expected=SOSK actual=mismatch' \
  'MOCKBADVERSION|endpoint.version expected=v1 actual=mismatch' \
  'MOCKBADSTATUS|endpoint.status expected=v1-known actual=mismatch' \
  'MOCKSHORTIO|endpoint.io expected=complete actual=short'; do
  serial="${rejection%%|*}"
  category="${rejection#*|}"
  if A33XCTL_ADB="$mock_adb" "$ctl" inspect-core1-readiness \
      --serial "$serial" \
      --expected-revision sos.core1dev.0123456789ab.cdef01234567 \
      >"$test_root/$serial.out" 2>"$test_root/$serial.err"; then
    printf 'failed Core-dev handshake unexpectedly passed readiness: %s\n' "$serial" >&2
    exit 1
  fi
  grep -F "Core-dev probe failed: $category" "$test_root/$serial.err" >/dev/null
  if A33XCTL_ADB="$mock_adb" "$ctl" core1-dev-clear-openrouter-key \
      --serial "$serial" >"$test_root/$serial-clear.out" \
      2>"$test_root/$serial-clear.err"; then
    printf 'failed Core-dev handshake unexpectedly passed clear: %s\n' "$serial" >&2
    exit 1
  fi
  grep -F "Core-dev probe failed: $category" \
    "$test_root/$serial-clear.err" >/dev/null
done

grep -F 'read -r -s -n 513 secret' "$ctl" >/dev/null
grep -F 'core_dev_status_file_matches "$stdout_file" "$expected_status"' \
  "$ctl" >/dev/null
grep -F 'shell -T -- \' "$ctl" >/dev/null
! grep -F 'exec-out \' "$ctl" >/dev/null
! grep -E 'adb.*(OPENROUTER|credential|key).*\$' "$ctl" >/dev/null
grep -Fx 'shell -T -- /system_ext/bin/sos-core-dev-credential set' \
  "$A33XCTL_MOCK_INVOCATION_LOG" >/dev/null
grep -Fx 'shell -T -- /system_ext/bin/sos-core-dev-credential clear' \
  "$A33XCTL_MOCK_INVOCATION_LOG" >/dev/null
grep -Fx 'shell -T -- /system_ext/bin/sos-core-dev-credential status' \
  "$A33XCTL_MOCK_INVOCATION_LOG" >/dev/null
grep -Fx 'shell -T -- /system_ext/bin/sos-core-dev-credential probe' \
  "$A33XCTL_MOCK_INVOCATION_LOG" >/dev/null
! grep -E '(^| )exec-out( |$)|(^| )-t(t)?( |$)' \
  "$A33XCTL_MOCK_INVOCATION_LOG" >/dev/null
! grep -R -F 'mock-non-secret' "$test_root" >/dev/null

ordinary_product="$repo_root/aosp/device/sos/a33x/lineage_sos_core1_a33x.mk"
dev_product="$repo_root/aosp/device/sos/a33x/lineage_sos_core1_dev_a33x.mk"
ordinary_expansion="$(
  make -s -f "$ordinary_product" 'inherit-product=' TARGET_BUILD_VARIANT=userdebug \
    --eval 'sos-print:;@printf "%s|%s|%s\n" "$(PRODUCT_NAME)" "$(PRODUCT_PACKAGES)" "$(PRODUCT_SYSTEM_EXT_PROPERTIES)"' \
    sos-print
)"
dev_expansion="$(
  make -s -f "$dev_product" 'inherit-product=' TARGET_BUILD_VARIANT=userdebug \
    --eval 'sos-print:;@printf "%s|%s|%s\n" "$(PRODUCT_NAME)" "$(PRODUCT_PACKAGES)" "$(PRODUCT_SYSTEM_EXT_PROPERTIES)"' \
    sos-print
)"
[[ "$ordinary_expansion" == \
  'lineage_sos_core1_a33x|sos-agent-runner|ro.sos.build_variant=core1-ordinary' ]]
[[ "$dev_expansion" == \
  'lineage_sos_core1_dev_a33x|sos-core-dev-credential sos-node-core-dev sos-agent-runner-core-dev|ro.sos.build_variant=core1-dev-credential ro.sos.dev_credential=1' ]]
if make -s -f "$dev_product" 'inherit-product=' TARGET_BUILD_VARIANT=user \
    --eval 'sos-print:;@:' sos-print >/dev/null 2>&1; then
  printf 'Core-dev product unexpectedly permitted a user build\n' >&2
  exit 1
fi
if make -s -f "$dev_product" 'inherit-product=' TARGET_BUILD_VARIANT=eng \
    --eval 'sos-print:;@:' sos-print >/dev/null 2>&1; then
  printf 'Core-dev product unexpectedly permitted an eng build\n' >&2
  exit 1
fi
grep -F 'target_device="$LINEAGE_SOS_CORE1_DEV_DEVICE"' "$ctl" >/dev/null
grep -F 'container env "BUILD_NUMBER=$build_number" bash -lc' "$ctl" >/dev/null
grep -F '[[ -f "$client" ]]' "$ctl" >/dev/null
grep -F 'system_ext/bin/sos-core-dev-credential 0 2000 755 capabilities=0x0' \
  "$ctl" >/dev/null
grep -F 'sos_core_dev_credential_exec:s0' "$ctl" >/dev/null
grep -F 'CORE_DEV_PROXY_DEVICE_PORT=37173' "$ctl" >/dev/null
grep -F 'CORE_DEV_PROXY_AUTHORITY=openrouter.ai:443' "$ctl" >/dev/null
! grep -E 'core1-dev-run-agent-smoke.*(prompt|provider|proxy|model|key)' "$ctl" >/dev/null
! grep -E 'sos-node-core-dev|sos-core-dev-credential|sos-agent-runner-core-dev|ro\.sos\.dev_credential' \
  "$ordinary_product" >/dev/null
grep -Fx '    sos-agent-runner' "$ordinary_product" >/dev/null
grep -Fx '    sos-node-core-dev \' "$dev_product" >/dev/null
grep -Fx '    sos-agent-runner-core-dev' "$dev_product" >/dev/null
[[ "$(grep -Fc '    filename: "agent-runner.cjs",' \
  "$repo_root/aosp/device/sos/a33x/Android.bp")" -eq 1 ]]
[[ "$(grep -Fc '    filename: "agent-runner-core-dev.cjs",' \
  "$repo_root/aosp/device/sos/a33x/Android.bp")" -eq 1 ]]
! grep -F 'overrides: ["sos-agent-runner"]' \
  "$repo_root/aosp/device/sos/a33x/Android.bp" >/dev/null

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
