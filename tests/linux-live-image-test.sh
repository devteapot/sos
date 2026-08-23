#!/usr/bin/env bash

set -euo pipefail

test_repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
test_image="$test_repo_root/tools/linux-live-image"
test_install="$test_repo_root/tools/install-linux-login-session"
test_root="$(mktemp -d -t sos-linux-live-image-test.XXXXXX)"

test_cleanup() {
  rm -r -- "$test_root"
}
trap test_cleanup EXIT

bash -n "$test_image"
bash -n "$test_install"
"$test_image" doctor >"$test_root/doctor.txt"
grep -Fx 'linux_live_image_doctor=PASS' "$test_root/doctor.txt" >/dev/null
grep -F 'nodejs' "$test_root/doctor.txt" >/dev/null
if grep -E 'libinput-devel|mesa-libgbm-devel|libseat-devel' "$test_root/doctor.txt" >/dev/null; then
  printf 'error: live image doctor advertised development packages as runtime deps\n' >&2
  exit 1
fi

"$test_image" format-identity \
  --source-revision abcdef123456 \
  --source-dirty false \
  --agent-mode offline \
  --base-iso-filename Fedora-Workstation-Live-x86_64-44-1.1.iso \
  --base-iso-bytes 2048 \
  --base-iso-sha256 0000000000000000000000000000000000000000000000000000000000000001 \
  --payload-relpath LiveOS/squashfs.img \
  --payload-bytes 1024 \
  --payload-sha256 0000000000000000000000000000000000000000000000000000000000000002 \
  --container-format squashfs-rootfs-img \
  --baked-at-utc 2026-08-23T00:00:00Z \
  --output-iso-filename sos-fedora-workstation-live-abcdef123456.iso \
  --output-iso-bytes 4096 \
  --output-iso-sha256 0000000000000000000000000000000000000000000000000000000000000003 \
  >"$test_root/identity.env"
for test_key in \
  image_kind=live-boot \
  campaign_class=live-boot \
  not_installed_product=true \
  source_revision=abcdef123456 \
  source_dirty=false \
  agent_mode=offline \
  payload_relpath=LiveOS/squashfs.img \
  payload_sha256=0000000000000000000000000000000000000000000000000000000000000002 \
  output_iso_sha256=0000000000000000000000000000000000000000000000000000000000000003; do
  grep -Fx "$test_key" "$test_root/identity.env" >/dev/null
done
if "$test_image" format-identity \
  --source-revision abcdef123456 \
  --source-dirty true \
  --agent-mode offline \
  --base-iso-filename Fedora-Workstation-Live-x86_64-44-1.1.iso \
  --base-iso-bytes 2048 \
  --base-iso-sha256 0000000000000000000000000000000000000000000000000000000000000001 \
  --payload-relpath LiveOS/squashfs.img \
  --container-format flat-squashfs \
  >"$test_root/dirty-identity.txt" 2>&1; then
  printf 'error: format-identity accepted a dirty source revision\n' >&2
  exit 1
fi
if "$test_image" format-identity \
  --source-revision abcdef123456 \
  --source-dirty false \
  --agent-mode live \
  --base-iso-filename Fedora-Workstation-Live-x86_64-44-1.1.iso \
  --base-iso-bytes 2048 \
  --base-iso-sha256 0000000000000000000000000000000000000000000000000000000000000001 \
  --payload-relpath LiveOS/squashfs.img \
  --container-format erofs \
  >"$test_root/live-agent-identity.txt" 2>&1; then
  printf 'error: format-identity accepted a live agent bake\n' >&2
  exit 1
fi

"$test_image" write-offline-user-state --home-root "$test_root/skel" \
  >"$test_root/skel.txt"
grep -Fx 'SOS_AGENT_MODEL=faux' "$test_root/skel/.local/state/sos/agent/config.env" >/dev/null
grep -Fx 'SOS_AGENT_FAKE_SOURCE=/usr/share/sos/experiences/daily-flow.luau' \
  "$test_root/skel/.local/state/sos/agent/config.env" >/dev/null
grep -Fx '{}' "$test_root/skel/.local/state/sos/output.json" >/dev/null
[[ "$(stat -c %a "$test_root/skel/.local/state/sos/agent/config.env")" == 600 ]]

test_rootfs="$test_root/rootfs"
mkdir -p \
  "$test_rootfs/usr/local/libexec/sos" \
  "$test_rootfs/usr/local/libexec/sos-agent/dist" \
  "$test_rootfs/usr/share/wayland-sessions" \
  "$test_rootfs/usr/share/sos/experiences" \
  "$test_rootfs/usr/share/doc/sos" \
  "$test_rootfs/usr/lib/systemd/system" \
  "$test_rootfs/etc/skel" \
  "$test_rootfs/home/liveuser"
: >"$test_rootfs/usr/local/libexec/sos/sos-login-session"
: >"$test_rootfs/usr/local/libexec/sos/sos-agent-login"
: >"$test_rootfs/usr/local/libexec/sos/linux-hardware-gate"
: >"$test_rootfs/usr/local/libexec/sos-agent/dist/agent-runner.cjs"
: >"$test_rootfs/usr/share/wayland-sessions/sos.desktop"
: >"$test_rootfs/usr/share/sos/experiences/daily-flow.luau"
: >"$test_rootfs/usr/lib/systemd/system/gdm.service"
printf '%s\n' \
  'source_revision=abcdef123456' \
  'source_dirty=false' \
  'agent_mode=offline' >"$test_rootfs/usr/share/doc/sos/install-metadata.env"
: >"$test_rootfs/usr/share/doc/sos/install-manifest.tsv"
cp -- "$test_root/identity.env" "$test_rootfs/usr/share/doc/sos/image-identity.env"
"$test_image" write-offline-user-state --home-root "$test_rootfs/etc/skel" >/dev/null
"$test_image" write-offline-user-state --home-root "$test_rootfs/home/liveuser" >/dev/null
mkdir -p "$test_rootfs/etc/systemd/system"
ln -s graphical.target "$test_rootfs/usr/lib/systemd/system/default.target"
"$test_image" check-rootfs --root "$test_rootfs" >"$test_root/check-pass.txt"
grep -F 'linux_live_image_rootfs_checked=PASS' "$test_root/check-pass.txt" >/dev/null
grep -F 'boot_kind=live-boot' "$test_root/check-pass.txt" >/dev/null
grep -F 'not_installed_product=true' "$test_root/check-pass.txt" >/dev/null

ln -sf sos-session.target "$test_rootfs/etc/systemd/system/default.target"
if "$test_image" check-rootfs --root "$test_rootfs" >"$test_root/check-appliance.txt" 2>&1; then
  printf 'error: check-rootfs accepted a boot-owned appliance default target\n' >&2
  exit 1
fi
grep -F 'boot-owned appliance target' "$test_root/check-appliance.txt" >/dev/null
rm -f -- "$test_rootfs/etc/systemd/system/default.target"

rm -f -- "$test_rootfs/usr/share/doc/sos/image-identity.env"
if "$test_image" check-rootfs --root "$test_rootfs" >"$test_root/check-stock.txt" 2>&1; then
  printf 'error: check-rootfs accepted a rootfs without SOS live identity\n' >&2
  exit 1
fi

"$test_install" 2>"$test_root/install-usage.txt" || true
grep -F -- '--destdir ROOT' "$test_root/install-usage.txt" >/dev/null
grep -F -- '--offline' "$test_install" >/dev/null
grep -F 'image destroot staging accepts only the offline agent' "$test_install" >/dev/null

for test_doc in \
  "$test_repo_root/docs/linux-live-image.md" \
  "$test_repo_root/docs/linux-hardware-gate.md" \
  "$test_repo_root/README.md"; do
  grep -F 'live-boot' "$test_doc" >/dev/null
  grep -E 'not an installed product|not_installed_product' "$test_doc" >/dev/null
done
grep -F 'lorax' "$test_repo_root/docs/linux-live-image.md" >/dev/null
grep -F 'install-linux-login-session' "$test_repo_root/docs/linux-live-image.md" >/dev/null
grep -F 'same-boot' "$test_repo_root/docs/linux-hardware-gate.md" >/dev/null
grep -F 'recovery_page_flip' "$test_repo_root/tools/linux-hardware-gate" >/dev/null
grep -F 'completed significant DRM page flip.*recovery_view=true' \
  "$test_repo_root/tools/linux-hardware-gate" >/dev/null

printf 'linux_live_image_host_tests=PASS\n'
