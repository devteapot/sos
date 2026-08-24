#!/usr/bin/env bash

set -euo pipefail

test_repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
test_session="$test_repo_root/packaging/libexec/sos-login-session"
test_mock_source="$test_repo_root/tests/fixtures/linux-login-component-mock.py"
test_root="$(mktemp -d -t sos-linux-login-test.XXXXXX)"

test_cleanup() {
  rm -r -- "$test_root"
}
trap test_cleanup EXIT

test_bin="$test_root/bin"
test_home="$test_root/home"
test_runtime="$test_root/runtime"
test_state="$test_root/state"
mkdir -p "$test_bin" "$test_home" "$test_runtime" "$test_state/sos/agent"
chmod 0700 "$test_runtime"
for test_binary in \
  sos-compositor \
  sos-experience-host \
  sos-provider-state-service \
  sos-revision-supervisor \
  sos-linux-session \
  sos-agent-authoring \
  node; do
  ln -s "$test_mock_source" "$test_bin/$test_binary"
done
touch "$test_root/agent-runner.cjs"
printf '%s\n' \
  'SOS_AGENT_PROVIDER=openai-codex' \
  'SOS_AGENT_MODEL=faux' \
  "SOS_AGENT_FAKE_SOURCE=$test_repo_root/experiences/daily-flow.luau" \
  >"$test_state/sos/agent/config.env"
chmod 0600 "$test_state/sos/agent/config.env"

PATH="$test_bin:$PATH" \
HOME="$test_home" \
XDG_RUNTIME_DIR="$test_runtime" \
XDG_STATE_HOME="$test_state" \
SOS_INSTALL_ROOT="$test_bin" \
SOS_AGENT_MAIN="$test_root/agent-runner.cjs" \
SOS_DEFAULT_EXPERIENCE="$test_repo_root/experiences/default.luau" \
SOS_TEST_AGENT_ARGS_FILE="$test_root/agent-arguments.txt" \
  "$test_session" >"$test_root/offline-session.txt" 2>&1

grep -Fx 'sos_login_agent_mode mode=offline' "$test_root/offline-session.txt" >/dev/null
grep -F 'sos_login_agent_started' "$test_root/offline-session.txt" >/dev/null
[[ "$(stat -c %a "$test_state/sos/output.json")" == 600 ]]
grep -Fx '{}' "$test_state/sos/output.json" >/dev/null
grep -Fx -- '--fake-source' "$test_root/agent-arguments.txt" >/dev/null
grep -Fx "$test_repo_root/experiences/daily-flow.luau" "$test_root/agent-arguments.txt" >/dev/null
if grep -Fx -- '--credentials' "$test_root/agent-arguments.txt" >/dev/null; then
  printf 'error: offline selectable session passed a credential path to the faux agent\n' >&2
  exit 1
fi

printf '%s\n' \
  'SOS_AGENT_PROVIDER=openai-codex' \
  'SOS_AGENT_MODEL=gpt-5.6-sol' >"$test_state/sos/agent/config.env"
if PATH="$test_bin:$PATH" \
  HOME="$test_home" \
  XDG_RUNTIME_DIR="$test_runtime" \
  XDG_STATE_HOME="$test_state" \
  SOS_INSTALL_ROOT="$test_bin" \
  SOS_AGENT_MAIN="$test_root/agent-runner.cjs" \
  SOS_DEFAULT_EXPERIENCE="$test_repo_root/experiences/default.luau" \
  SOS_TEST_AGENT_ARGS_FILE="$test_root/live-agent-arguments.txt" \
    "$test_session" >"$test_root/live-session.txt" 2>&1; then
  printf 'error: live selectable session started without credentials\n' >&2
  exit 1
fi
grep -F 'resident agent is not authenticated' "$test_root/live-session.txt" >/dev/null

printf 'linux_login_session_host_tests=PASS\n'
