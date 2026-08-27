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

grep -F '/usr/local/libexec/sos-agent/dist/agent-runner.cjs|\' "$test_gate" >/dev/null

test_home="$test_root/home"
test_state="$test_root/state"
mkdir -p "$test_home" "$test_state"
HOME="$test_home" \
XDG_STATE_HOME="$test_state" \
SOS_AGENT_MAIN="$test_root/agent-runner.cjs" \
SOS_AGENT_FAKE_SOURCE="$test_repo_root/experiences/default.luau" \
  "$test_login" --offline >"$test_root/offline-login.txt"
test_config="$test_state/sos/agent/config.env"
[[ "$(stat -c %a "$test_config")" == 600 ]]
grep -Fx 'SOS_AGENT_PROVIDER=openai-codex' "$test_config" >/dev/null
grep -Fx 'SOS_AGENT_MODEL=faux' "$test_config" >/dev/null
grep -Fx "SOS_AGENT_FAKE_SOURCE=$test_repo_root/experiences/default.luau" \
  "$test_config" >/dev/null
HOME="$test_home" \
XDG_STATE_HOME="$test_state" \
SOS_AGENT_MAIN="$test_root/agent-runner.cjs" \
  "$test_login" --if-needed >"$test_root/offline-ready.txt"
grep -F 'sos_agent_login_ready provider=faux' "$test_root/offline-ready.txt" >/dev/null

test_gate_bin="$test_root/gate-bin"
test_gate_evidence="$test_root/gate-inhibitor-evidence"
test_gate_state="$test_root/gate-inhibitor-state.txt"
mkdir -p "$test_gate_bin" "$test_gate_evidence/environment"
for test_binary in sudo systemctl systemd-inhibit systemd-run sleep; do
  ln -s "$test_repo_root/tests/fixtures/linux-login-component-mock.py" \
    "$test_gate_bin/$test_binary"
done
(
  export PATH="$test_gate_bin:$PATH"
  export SOS_TEST_GATE_INHIBITOR_STATE="$test_gate_state"
  export SOS_TEST_GATE_SYSTEMD_RUN_ARGS_FILE="$test_root/gate-systemd-run-arguments.txt"
  # shellcheck source=../tools/linux-hardware-gate
  source "$test_gate"
  hardware_gate_start_awake_inhibitor "$test_gate_evidence"
  grep -Fx active "$test_gate_state" >/dev/null
  hardware_gate_collect_cleanup_unit="$hardware_gate_awake_unit"
  hardware_gate_collect_cleanup
  grep -Fx inactive "$test_gate_state" >/dev/null
  hardware_gate_start_awake_inhibitor "$test_gate_evidence"
  hardware_gate_stop_awake_inhibitor "$test_gate_evidence" "$hardware_gate_awake_unit"
)
grep -Fx -- '--unit=sos-linux-hardware-gate-awake.service' \
  "$test_root/gate-systemd-run-arguments.txt" >/dev/null
grep -Fx -- '--property=CollectMode=inactive-or-failed' \
  "$test_root/gate-systemd-run-arguments.txt" >/dev/null
grep -Fx -- '/usr/bin/systemd-inhibit' \
  "$test_root/gate-systemd-run-arguments.txt" >/dev/null
grep -Fx -- '--what=idle:sleep:handle-lid-switch' \
  "$test_root/gate-systemd-run-arguments.txt" >/dev/null
grep -Fx -- '--mode=block' "$test_root/gate-systemd-run-arguments.txt" >/dev/null
grep -Fx -- '/usr/bin/sleep' "$test_root/gate-systemd-run-arguments.txt" >/dev/null
grep -F 'SOS Linux hardware gate' \
  "$test_gate_evidence/environment/gate-inhibitor-prepared.txt" >/dev/null
grep -F 'sleep:idle:handle-lid-switch' \
  "$test_gate_evidence/gate-inhibitor-before-release.txt" >/dev/null
grep -Fx 'state=inactive' "$test_gate_evidence/gate-inhibitor-release.txt" >/dev/null

test_evidence="$test_root/evidence"
mkdir -p "$test_evidence/environment"
test_boot_id=12345678-1234-1234-1234-123456789abc
test_other_boot_id=87654321-4321-4321-4321-cba987654321
printf '%s\n' \
  'agent_mode=offline' \
  'boot_kind=installed' \
  'campaign_class=installed-workstation' \
  'not_installed_product=false' \
  "boot_id=$test_boot_id" >"$test_evidence/campaign.env"
printf 'boot_id=%s\n' "$test_boot_id" >"$test_evidence/collection.env"
printf '%s\n' \
  'completed significant DRM page flip output=eDP-1 recovery_view=true' \
  'sos_compositor_ready wayland_display=wayland-sos backend=drm evidence=drm_page_flip' \
  'linux_system_session_ready revision_id=1111 evidence=drm_page_flip' \
  'sos_login_agent_mode mode=offline' \
  'sos_login_agent_started pid=44 socket=/run/user/1000/sos/agent.sock' \
  'linux_system_session_component component=host pid=55 uid=1000' \
  'authenticated SOS compositor control connection pid=55 role=Shell' \
  'linux_system_session_component component=host pid=77 uid=1000' \
  'authenticated SOS compositor control connection pid=77 role=NativeApplication' \
  'GraphHostRestarted { graph_id: dashboard, failed_host_pid: 66, host_pid: 77 }' \
  'observed native compositor input input_class="keyboard"' \
  'observed native compositor input input_class="relative_pointer"' \
  'observed native compositor input input_class="pointer_button"' \
  'observed native compositor input input_class="touch"' \
  'presented armed shell revision revision_id="1111" evidence="drm_page_flip"' \
  'presented armed shell revision revision_id="2222" evidence="drm_page_flip"' \
  'linux_login_session_stopped reason=user_logout' >"$test_evidence/journal-user.txt"
printf '%s\n' \
  'Device:                  Integrated Keyboard' \
  'Capabilities:            keyboard' \
  'Device:                  Integrated Touchpad' \
  'Capabilities:            pointer gesture' \
  'Device:                  Integrated Touchscreen' \
  'Capabilities:            touch' >"$test_evidence/environment/libinput.txt"
: >"$test_evidence/journal-kernel.txt"
printf '2222\n' >"$test_evidence/stock-registry-revision.txt"
printf '2222\n' >"$test_evidence/stock-authority-revision.txt"
printf '{}\n' >"$test_evidence/authority.json"
printf 'active\n' >"$test_evidence/display-manager-active.txt"
printf '%s\n' \
  'SOS Linux hardware gate 0 root 123 systemd-inhibit sleep:idle:handle-lid-switch Prepared physical acceptance campaign block' \
  >"$test_evidence/environment/gate-inhibitor-prepared.txt"
cp -- "$test_evidence/environment/gate-inhibitor-prepared.txt" \
  "$test_evidence/gate-inhibitor-before-release.txt"
printf '%s\n' \
  'unit=sos-linux-hardware-gate-awake.service' \
  'state=inactive' >"$test_evidence/gate-inhibitor-release.txt"
"$test_gate" audit --evidence-dir "$test_evidence" >"$test_root/pass-audit.txt"
grep -Fx "criterion=same_boot result=PASS boot_id=$test_boot_id" \
  "$test_root/pass-audit.txt" >/dev/null
grep -Fx 'criterion=gate_awake_inhibitor result=PASS' \
  "$test_root/pass-audit.txt" >/dev/null
grep -Fx 'criterion=stable_host_lifecycle result=PASS shell_host_pids=1 application_host_pids=1' \
  "$test_root/pass-audit.txt" >/dev/null
grep -Fx \
  'linux_hardware_gate_result=PASS evidence=drm_page_flip physical_input=keyboard,touchpad,touchscreen' \
  "$test_root/pass-audit.txt" >/dev/null
grep -Fx 'boot_kind=installed campaign_class=installed-workstation' \
  "$test_root/pass-audit.txt" >/dev/null
if grep -F 'not_installed_product=true' "$test_root/pass-audit.txt" >/dev/null; then
  printf 'error: installed-workstation audit labeled the campaign as a live non-product\n' >&2
  exit 1
fi

sed -i '/input_class="touch"/d' "$test_evidence/journal-user.txt"
if "$test_gate" audit --evidence-dir "$test_evidence" >"$test_root/fail-audit.txt"; then
  printf 'error: audit accepted evidence without physical touchscreen input\n' >&2
  exit 1
fi
grep -Fx 'criterion=touchscreen_input result=FAIL' "$test_root/fail-audit.txt" >/dev/null
printf '%s\n' \
  'observed native compositor input input_class="touch"' >>"$test_evidence/journal-user.txt"

rm -- "$test_evidence/gate-inhibitor-release.txt"
if "$test_gate" audit --evidence-dir "$test_evidence" >"$test_root/inhibitor-fail-audit.txt"; then
  printf 'error: audit accepted evidence without awake-inhibitor release proof\n' >&2
  exit 1
fi
grep -Fx 'criterion=gate_awake_inhibitor result=FAIL' \
  "$test_root/inhibitor-fail-audit.txt" >/dev/null
printf '%s\n' \
  'unit=sos-linux-hardware-gate-awake.service' \
  'state=inactive' >"$test_evidence/gate-inhibitor-release.txt"

printf '%s\n' \
  'libinput device added device_id="event99" device_name="Synthetic Gate Touch"' \
  >>"$test_evidence/journal-user.txt"
if "$test_gate" audit --evidence-dir "$test_evidence" \
  >"$test_root/unexpected-input-device-audit.txt"; then
  printf 'error: audit accepted an input device absent from the preparation inventory\n' >&2
  exit 1
fi
grep -Fx 'criterion=input_device_inventory result=FAIL unexpected_devices=1' \
  "$test_root/unexpected-input-device-audit.txt" >/dev/null
sed -i '/Synthetic Gate Touch/d' "$test_evidence/journal-user.txt"

printf 'boot_id=%s\n' "$test_other_boot_id" >"$test_evidence/collection.env"
if "$test_gate" audit --evidence-dir "$test_evidence" >"$test_root/cross-boot-audit.txt"; then
  printf 'error: audit accepted evidence collected after a different kernel boot\n' >&2
  exit 1
fi
grep -Fx \
  "criterion=same_boot result=FAIL prepared=$test_boot_id collected=$test_other_boot_id" \
  "$test_root/cross-boot-audit.txt" >/dev/null
printf 'boot_id=%s\n' "$test_boot_id" >"$test_evidence/collection.env"

LC_ALL=en_US.UTF-8 \
  "$test_gate" finalize-manifest --evidence-dir "$test_evidence"
cut -f 1 "$test_evidence/evidence-manifest.tsv" | LC_ALL=C sort -c
LC_ALL=C.UTF-8 \
  "$test_gate" verify-manifest --evidence-dir "$test_evidence" \
  >"$test_root/manifest-pass.txt"
grep -F 'evidence_manifest_verified=PASS' "$test_root/manifest-pass.txt" >/dev/null
printf 'tampered\n' >>"$test_evidence/stock-registry-revision.txt"
if "$test_gate" verify-manifest --evidence-dir "$test_evidence" \
  >"$test_root/manifest-fail.txt" 2>&1; then
  printf 'error: manifest verification accepted tampered evidence\n' >&2
  exit 1
fi
grep -F 'manifested evidence size changed' "$test_root/manifest-fail.txt" >/dev/null

printf '2222\n' >"$test_evidence/stock-registry-revision.txt"
printf '%s\n' \
  'agent_mode=offline' \
  'boot_kind=development-live' \
  'campaign_class=development-live' \
  'not_installed_product=true' \
  'promotion_eligible=false' \
  "boot_id=$test_boot_id" >"$test_evidence/campaign.env"
printf '%s\n' \
  'observed native compositor input input_class="touch"' >>"$test_evidence/journal-user.txt"
"$test_gate" audit --evidence-dir "$test_evidence" >"$test_root/live-audit.txt"
grep -Fx \
  'linux_hardware_gate_result=DIAGNOSTIC_PASS promotion_eligible=false evidence=drm_page_flip physical_input=keyboard,touchpad,touchscreen' \
  "$test_root/live-audit.txt" >/dev/null
grep -Fx 'boot_kind=development-live campaign_class=development-live' \
  "$test_root/live-audit.txt" >/dev/null
grep -Fx 'not_installed_product=true' "$test_root/live-audit.txt" >/dev/null
grep -Fx 'promotion_eligible=false' "$test_root/live-audit.txt" >/dev/null

sed -i '/input_class="touch"/d' "$test_evidence/journal-user.txt"
if "$test_gate" audit --evidence-dir "$test_evidence" >"$test_root/live-fail-audit.txt"; then
  printf 'error: development-live audit accepted evidence without physical touchscreen input\n' >&2
  exit 1
fi
grep -Fx 'criterion=touchscreen_input result=FAIL' "$test_root/live-fail-audit.txt" >/dev/null
grep -Fx 'linux_hardware_gate_result=DIAGNOSTIC_FAIL promotion_eligible=false' \
  "$test_root/live-fail-audit.txt" >/dev/null
grep -Fx 'boot_kind=development-live campaign_class=development-live' \
  "$test_root/live-fail-audit.txt" >/dev/null

test_sysroot="$test_root/sysroot"
mkdir -p \
  "$test_sysroot/usr/share/doc/sos" \
  "$test_sysroot/run/initramfs/live" \
  "$test_sysroot/proc/sys/kernel/random"
printf '12345678-1234-1234-1234-123456789abc\n' \
  >"$test_sysroot/proc/sys/kernel/random/boot_id"
printf '%s\n' \
  'image_kind=development-live' \
  'campaign_class=development-live' \
  'not_installed_product=true' \
  'promotion_eligible=false' \
  'mutable_runtime=true' \
  'ssh_enabled=true' \
  'container_format=erofs-rootfs' \
  'fedora_release=44' \
  'build_host_release=44' \
  'source_revision=abc123' \
  'source_dirty=false' \
  'agent_mode=offline' \
  'base_iso_filename=Fedora-Workstation-Live-x86_64-44-1.1.iso' \
  'base_iso_bytes=2048' \
  'base_iso_sha256=0000000000000000000000000000000000000000000000000000000000000001' \
  'payload_relpath=LiveOS/squashfs.img' \
  'baked_at_utc=2026-08-24T00:00:00Z' \
  >"$test_sysroot/usr/share/doc/sos/image-identity.env"
if "$test_gate" classify-boot --sysroot "$test_sysroot" \
  >"$test_root/classify-missing-media-identity.txt" 2>&1; then
  printf 'error: classify-boot accepted live media without its ISO-level identity\n' >&2
  exit 1
fi
grep -F 'missing the ISO-level image identity' \
  "$test_root/classify-missing-media-identity.txt" >/dev/null
cp -- "$test_sysroot/usr/share/doc/sos/image-identity.env" \
  "$test_sysroot/run/initramfs/live/sos-image-identity.env"
printf '%s\n' \
  'payload_bytes=1024' \
  'payload_sha256=0000000000000000000000000000000000000000000000000000000000000002' \
  >>"$test_sysroot/run/initramfs/live/sos-image-identity.env"
"$test_gate" classify-boot --sysroot "$test_sysroot" >"$test_root/classify-live.txt"
grep -Fx 'boot_kind=development-live' "$test_root/classify-live.txt" >/dev/null
grep -Fx 'not_installed_product=true' "$test_root/classify-live.txt" >/dev/null
grep -Fx 'promotion_eligible=false' "$test_root/classify-live.txt" >/dev/null
grep -Fx 'boot_id=12345678-1234-1234-1234-123456789abc' \
  "$test_root/classify-live.txt" >/dev/null
grep -Fx 'live_overlay=present' "$test_root/classify-live.txt" >/dev/null

sed -i 's/source_revision=abc123/source_revision=wrong/' \
  "$test_sysroot/run/initramfs/live/sos-image-identity.env"
if "$test_gate" classify-boot --sysroot "$test_sysroot" \
  >"$test_root/classify-identity-mismatch.txt" 2>&1; then
  printf 'error: classify-boot accepted mismatched rootfs and media identities\n' >&2
  exit 1
fi
grep -F 'rootfs and ISO-level image identities disagree' \
  "$test_root/classify-identity-mismatch.txt" >/dev/null

rm -r -- "$test_sysroot/run/initramfs/live"
if "$test_gate" classify-boot --sysroot "$test_sysroot" >"$test_root/classify-stale.txt" 2>&1; then
  printf 'error: classify-boot accepted development-live identity without a live overlay\n' >&2
  exit 1
fi
grep -F 'do not collect it as development-live or an installed product' \
  "$test_root/classify-stale.txt" >/dev/null

rm -f -- "$test_sysroot/usr/share/doc/sos/image-identity.env"
mkdir -p "$test_sysroot/run/initramfs/live"
if "$test_gate" classify-boot --sysroot "$test_sysroot" >"$test_root/classify-stock.txt" 2>&1; then
  printf 'error: classify-boot accepted stock live media without SOS identity\n' >&2
  exit 1
fi
grep -F 'stock live media is not a development image' "$test_root/classify-stock.txt" >/dev/null

rm -r -- "$test_sysroot/run/initramfs/live"
"$test_gate" classify-boot --sysroot "$test_sysroot" >"$test_root/classify-installed.txt"
grep -Fx 'boot_kind=installed' "$test_root/classify-installed.txt" >/dev/null
grep -Fx 'campaign_class=installed-workstation' "$test_root/classify-installed.txt" >/dev/null

for test_label in development-live 'not an installed product' image-identity squashfs; do
  grep -F "$test_label" "$test_repo_root/docs/linux-hardware-gate.md" >/dev/null
  grep -F "$test_label" "$test_repo_root/docs/linux-live-image.md" >/dev/null
done
grep -F 'boot_kind=development-live' "$test_gate" >/dev/null
grep -F 'not_installed_product=true' "$test_gate" >/dev/null
grep -F 'DIAGNOSTIC_PASS promotion_eligible=false' "$test_gate" >/dev/null
grep -F 'image-identity.env' "$test_gate" >/dev/null
grep -F 'payload_sha256' "$test_gate" >/dev/null
grep -F 'boot_id=' "$test_gate" >/dev/null
grep -F '/usr/local/libexec/sos/linux-hardware-gate collect' "$test_gate" >/dev/null
grep -F 'retired_baked_artifacts' "$test_gate" >/dev/null
grep -F '/usr/share/sos/experiences/daily-flow.luau' "$test_gate" >/dev/null
for test_development_path in \
  /usr/local/libexec/sos/sos-agent-login \
  /usr/share/sos/experiences/default.package.json \
  /usr/share/sos/experiences/modules/stock-theme.luau \
  /usr/share/doc/sos/sos-agent.md \
  /usr/share/doc/sos/linux-stable-host.md \
  /etc/xdg/monitors.xml; do
  grep -F "$test_development_path" "$test_gate" >/dev/null
done

printf 'linux_hardware_gate_host_tests=PASS\n'
