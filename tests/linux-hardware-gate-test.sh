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
grep -Fx "criterion=same_boot result=PASS boot_id=$test_boot_id" \
  "$test_root/pass-audit.txt" >/dev/null
grep -Fx \
  'linux_hardware_gate_result=PASS evidence=drm_page_flip physical_input=keyboard,touchpad,touchscreen' \
  "$test_root/pass-audit.txt" >/dev/null
grep -Fx 'boot_kind=installed campaign_class=installed-workstation' \
  "$test_root/pass-audit.txt" >/dev/null
if grep -F 'not_installed_product=true' "$test_root/pass-audit.txt" >/dev/null; then
  printf 'error: installed-workstation audit labeled the campaign live-boot product=false\n' >&2
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

printf 'boot_id=%s\n' "$test_other_boot_id" >"$test_evidence/collection.env"
if "$test_gate" audit --evidence-dir "$test_evidence" >"$test_root/cross-boot-audit.txt"; then
  printf 'error: audit accepted evidence collected after a different kernel boot\n' >&2
  exit 1
fi
grep -Fx \
  "criterion=same_boot result=FAIL prepared=$test_boot_id collected=$test_other_boot_id" \
  "$test_root/cross-boot-audit.txt" >/dev/null
printf 'boot_id=%s\n' "$test_boot_id" >"$test_evidence/collection.env"

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

printf '2222\n' >"$test_evidence/current-revision.txt"
printf '%s\n' \
  'agent_mode=offline' \
  'boot_kind=live-boot' \
  'campaign_class=live-boot' \
  'not_installed_product=true' >"$test_evidence/campaign.env"
printf '%s\n' \
  'observed native compositor input input_class="touch"' >>"$test_evidence/journal-user.txt"
"$test_gate" audit --evidence-dir "$test_evidence" >"$test_root/live-audit.txt"
grep -Fx \
  'linux_hardware_gate_result=PASS evidence=drm_page_flip physical_input=keyboard,touchpad,touchscreen' \
  "$test_root/live-audit.txt" >/dev/null
grep -Fx 'boot_kind=live-boot campaign_class=live-boot' "$test_root/live-audit.txt" >/dev/null
grep -Fx 'not_installed_product=true' "$test_root/live-audit.txt" >/dev/null

sed -i '/input_class="touch"/d' "$test_evidence/journal-user.txt"
if "$test_gate" audit --evidence-dir "$test_evidence" >"$test_root/live-fail-audit.txt"; then
  printf 'error: live-boot audit accepted evidence without physical touchscreen input\n' >&2
  exit 1
fi
grep -Fx 'criterion=touchscreen_input result=FAIL' "$test_root/live-fail-audit.txt" >/dev/null
grep -Fx 'boot_kind=live-boot campaign_class=live-boot' "$test_root/live-fail-audit.txt" >/dev/null

test_sysroot="$test_root/sysroot"
mkdir -p \
  "$test_sysroot/usr/share/doc/sos" \
  "$test_sysroot/run/initramfs/live" \
  "$test_sysroot/proc/sys/kernel/random"
printf '12345678-1234-1234-1234-123456789abc\n' \
  >"$test_sysroot/proc/sys/kernel/random/boot_id"
printf '%s\n' \
  'image_kind=live-boot' \
  'campaign_class=live-boot' \
  'not_installed_product=true' \
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
grep -Fx 'boot_kind=live-boot' "$test_root/classify-live.txt" >/dev/null
grep -Fx 'not_installed_product=true' "$test_root/classify-live.txt" >/dev/null
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
  printf 'error: classify-boot accepted live-boot identity without a live overlay\n' >&2
  exit 1
fi
grep -F 'do not collect it as live-boot or installed product' "$test_root/classify-stale.txt" >/dev/null

rm -f -- "$test_sysroot/usr/share/doc/sos/image-identity.env"
mkdir -p "$test_sysroot/run/initramfs/live"
if "$test_gate" classify-boot --sysroot "$test_sysroot" >"$test_root/classify-stock.txt" 2>&1; then
  printf 'error: classify-boot accepted stock live media without SOS identity\n' >&2
  exit 1
fi
grep -F 'stock live media is not a hardware-gate image' "$test_root/classify-stock.txt" >/dev/null

rm -r -- "$test_sysroot/run/initramfs/live"
"$test_gate" classify-boot --sysroot "$test_sysroot" >"$test_root/classify-installed.txt"
grep -Fx 'boot_kind=installed' "$test_root/classify-installed.txt" >/dev/null
grep -Fx 'campaign_class=installed-workstation' "$test_root/classify-installed.txt" >/dev/null

for test_label in live-boot 'not an installed product' image-identity squashfs; do
  grep -F "$test_label" "$test_repo_root/docs/linux-hardware-gate.md" >/dev/null
  grep -F "$test_label" "$test_repo_root/docs/linux-live-image.md" >/dev/null
done
grep -F 'boot_kind=live-boot' "$test_gate" >/dev/null
grep -F 'not_installed_product=true' "$test_gate" >/dev/null
grep -F 'image-identity.env' "$test_gate" >/dev/null
grep -F 'payload_sha256' "$test_gate" >/dev/null
grep -F 'boot_id=' "$test_gate" >/dev/null
grep -F '/usr/local/libexec/sos/linux-hardware-gate collect' "$test_gate" >/dev/null

printf 'linux_hardware_gate_host_tests=PASS\n'
