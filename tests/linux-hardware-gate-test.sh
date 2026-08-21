#!/usr/bin/env bash

set -euo pipefail

test_repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
test_gate="$test_repo_root/tools/linux-hardware-gate"
test_login="$test_repo_root/packaging/libexec/sos-agent-login"
test_root="$(mktemp -d -t sos-linux-hardware-test.XXXXXX)"

test_cleanup() {
  rm -r -- "$test_root"
}
trap test_cleanup EXIT

test_home="$test_root/home"
test_state="$test_root/state"
mkdir -p "$test_home" "$test_state"
HOME="$test_home" \
XDG_STATE_HOME="$test_state" \
SOS_AGENT_MAIN="$test_root/agent-runner.cjs" \
SOS_AGENT_FAKE_SOURCE="$test_repo_root/experiences/daily-flow.luau" \
  "$test_login" --offline >"$test_root/offline-login.txt"
test_config="$test_state/sos/agent/config.env"
[[ "$(stat -c %a "$test_config")" == 600 ]]
grep -Fx 'SOS_AGENT_PROVIDER=openai-codex' "$test_config" >/dev/null
grep -Fx 'SOS_AGENT_MODEL=faux' "$test_config" >/dev/null
grep -Fx "SOS_AGENT_FAKE_SOURCE=$test_repo_root/experiences/daily-flow.luau" \
  "$test_config" >/dev/null
HOME="$test_home" \
XDG_STATE_HOME="$test_state" \
SOS_AGENT_MAIN="$test_root/agent-runner.cjs" \
  "$test_login" --if-needed >"$test_root/offline-ready.txt"
grep -F 'sos_agent_login_ready provider=faux' "$test_root/offline-ready.txt" >/dev/null

test_evidence="$test_root/evidence"
mkdir "$test_evidence"
printf 'agent_mode=offline\n' >"$test_evidence/campaign.env"
printf '%s\n' \
  'completed significant DRM page flip output=eDP-1 recovery_view=true' \
  'sos_compositor_ready wayland_display=wayland-sos backend=drm evidence=drm_page_flip' \
  'linux_system_session_ready revision_id=1111 evidence=drm_page_flip' \
  'sos_login_agent_mode mode=offline' \
  'sos_login_agent_started pid=44 socket=/run/user/1000/sos/agent.sock' \
  'linux_system_session_component component=host pid=55 uid=1000' \
  'observed native compositor input input_class="keyboard"' \
  'observed native compositor input input_class="relative_pointer"' \
  'observed native compositor input input_class="pointer_button"' \
  'observed native compositor input input_class="touch"' \
  'presented armed shell revision revision_id="1111" evidence="drm_page_flip"' \
  'presented armed shell revision revision_id="2222" evidence="drm_page_flip"' \
  'linux_login_session_stopped reason=user_logout' >"$test_evidence/journal-user.txt"
: >"$test_evidence/journal-kernel.txt"
printf '2222\n' >"$test_evidence/current-revision.txt"
printf '2222\n' >"$test_evidence/authority-revision.txt"
printf 'active\n' >"$test_evidence/display-manager-active.txt"
"$test_gate" audit --evidence-dir "$test_evidence" >"$test_root/pass-audit.txt"
grep -Fx \
  'linux_hardware_gate_result=PASS evidence=drm_page_flip physical_input=keyboard,touchpad,touchscreen' \
  "$test_root/pass-audit.txt" >/dev/null

sed -i '/input_class="touch"/d' "$test_evidence/journal-user.txt"
if "$test_gate" audit --evidence-dir "$test_evidence" >"$test_root/fail-audit.txt"; then
  printf 'error: audit accepted evidence without physical touchscreen input\n' >&2
  exit 1
fi
grep -Fx 'criterion=touchscreen_input result=FAIL' "$test_root/fail-audit.txt" >/dev/null
printf '%s\n' \
  'observed native compositor input input_class="touch"' >>"$test_evidence/journal-user.txt"

"$test_gate" finalize-manifest --evidence-dir "$test_evidence"
"$test_gate" verify-manifest --evidence-dir "$test_evidence" \
  >"$test_root/manifest-pass.txt"
grep -F 'evidence_manifest_verified=PASS' "$test_root/manifest-pass.txt" >/dev/null
printf 'tampered\n' >>"$test_evidence/current-revision.txt"
if "$test_gate" verify-manifest --evidence-dir "$test_evidence" \
  >"$test_root/manifest-fail.txt" 2>&1; then
  printf 'error: manifest verification accepted tampered evidence\n' >&2
  exit 1
fi
grep -F 'manifested evidence size changed' "$test_root/manifest-fail.txt" >/dev/null

printf 'linux_hardware_gate_host_tests=PASS\n'
