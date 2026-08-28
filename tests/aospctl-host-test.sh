#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ctl="$repo_root/tools/aospctl"
test_root="$(mktemp -d -t sos-aospctl-host-test.XXXXXX)"

cleanup() {
  rm -r -- "$test_root"
}
trap cleanup EXIT

jq -e '.experience_id == "sos.stock.mobile" and .role == "shell"' \
  "$repo_root/experiences/mobile.package.json" >/dev/null
grep -F 'src: "prebuilts/mobile.luau"' \
  "$repo_root/aosp/device/sos/cuttlefish/Android.bp" >/dev/null
! grep -F 'prebuilts/default.luau' \
  "$repo_root/aosp/device/sos/cuttlefish/Android.bp" >/dev/null

state_root="$test_root/state"
aosp_root="$test_root/aosp"
mock_bin="$test_root/bin"
mkdir -p "$state_root" "$aosp_root/out/host/linux-x86/bin" "$mock_bin"
printf '101\n' >"$state_root/authority-pid"
printf '301\n' >"$state_root/home-pid"
printf 'stock\n' >"$state_root/mode"
ln -s "$repo_root/tests/fixtures/aospctl-mock-adb" \
  "$aosp_root/out/host/linux-x86/bin/adb"

cat >"$mock_bin/podman" <<'MOCK_PODMAN'
#!/usr/bin/env bash
if [[ "${1:-}" == container && "${2:-}" == exists ]]; then
  exit 1
fi
printf 'unexpected mock podman invocation: %s\n' "$*" >&2
exit 2
MOCK_PODMAN
chmod 0755 "$mock_bin/podman"

PATH="$mock_bin:$PATH" \
SOS_AOSP_ROOT="$aosp_root" \
SOS_TEST_AOSP_STATE="$state_root" \
SOS_CUTTLEFISH_SERIAL=127.0.0.1:6520 \
  "$ctl" verify-sos >"$test_root/verdict.txt"

grep -Fx 'boot_completed=1' "$test_root/verdict.txt" >/dev/null
grep -Fx 'home=dev.sos.experience/.SosHomeActivity' \
  "$test_root/verdict.txt" >/dev/null
grep -Fx 'abi=x86_64' "$test_root/verdict.txt" >/dev/null
grep -Fx 'selinux=Enforcing' "$test_root/verdict.txt" >/dev/null
grep -Fx 'authority_pid_before=101' "$test_root/verdict.txt" >/dev/null
grep -Fx 'authority_pid_after=202' "$test_root/verdict.txt" >/dev/null
grep -Fx 'home_pid_before=301' "$test_root/verdict.txt" >/dev/null
grep -Fx 'home_pid_after=302' "$test_root/verdict.txt" >/dev/null
grep -Fx 'bootstrap_graph=stock-graph' "$test_root/verdict.txt" >/dev/null
grep -Fx 'activated_graph=dashboard-graph' "$test_root/verdict.txt" >/dev/null
grep -Fx 'activated_experience=sos.example.dashboard' \
  "$test_root/verdict.txt" >/dev/null
grep -Fx 'adb_reverse=none' "$test_root/verdict.txt" >/dev/null

grep -F $'shell\treadlink\t/data/misc/sos/revisions/active/sos.stock.mobile/current' \
  "$state_root/requests.tsv" >/dev/null
grep -F $'shell\treadlink\t/data/misc/sos/revisions/active/sos.example.dashboard/current' \
  "$state_root/requests.tsv" >/dev/null
grep -F 'sos://experience/present/sos.example.dashboard' \
  "$state_root/requests.tsv" >/dev/null
if grep -F $'readlink\t/data/misc/sos/revisions/current' \
  "$state_root/requests.tsv" >/dev/null; then
  printf 'legacy singleton revision pointer was queried\n' >&2
  exit 1
fi

printf '101\n' >"$state_root/authority-pid"
printf '301\n' >"$state_root/home-pid"
printf 'stock\n' >"$state_root/mode"
if PATH="$mock_bin:$PATH" \
  SOS_AOSP_ROOT="$aosp_root" \
  SOS_TEST_AOSP_STATE="$state_root" \
  SOS_TEST_AOSP_BREAK=home-kills-authority \
  SOS_CUTTLEFISH_SERIAL=127.0.0.1:6520 \
    "$ctl" verify-sos >"$test_root/broken-verdict.txt" 2>&1; then
  printf 'Cuttlefish verifier accepted an authority restart caused by HOME recovery\n' >&2
  exit 1
fi
grep -F 'authority process changed when only HOME was killed' \
  "$test_root/broken-verdict.txt" >/dev/null

printf 'aospctl_host_test=PASS\n'
