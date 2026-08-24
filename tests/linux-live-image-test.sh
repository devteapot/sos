#!/usr/bin/env bash

set -euo pipefail

test_repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
test_image="$test_repo_root/tools/linux-live-image"
test_install="$test_repo_root/tools/install-linux-login-session"
test_root="$(mktemp -d -t sos-linux-live-image-test.XXXXXX)"
test_revision=abcdef1234567890abcdef1234567890abcdef12
test_namespace_dir=""
test_locked_dir=""

test_cleanup() {
  if [[ -n "$test_locked_dir" && -d "$test_locked_dir" ]]; then
    chmod 0700 "$test_locked_dir" 2>/dev/null || true
  fi
  if [[ -n "$test_namespace_dir" && -d "$test_namespace_dir" ]]; then
    rm -r -- "$test_namespace_dir"
  fi
  rm -r -- "$test_root"
}
trap test_cleanup EXIT

bash -n "$test_image"
bash -n "$test_install"
"$test_image" doctor --layout-only >"$test_root/doctor.txt"
grep -Fx 'linux_live_image_doctor=PASS mode=layout-only bake_ready=false' \
  "$test_root/doctor.txt" >/dev/null
if "$test_image" doctor >"$test_root/bake-doctor.txt"; then
  grep -Fx 'linux_live_image_doctor=PASS bake_ready=true' "$test_root/bake-doctor.txt" >/dev/null
else
  grep -Fx 'linux_live_image_doctor=FAIL bake_ready=false' "$test_root/bake-doctor.txt" >/dev/null
fi
grep -F 'nodejs' "$test_root/doctor.txt" >/dev/null
if grep -E 'libinput-devel|mesa-libgbm-devel|libseat-devel' "$test_root/doctor.txt" >/dev/null; then
  printf 'error: live image doctor advertised development packages as runtime deps\n' >&2
  exit 1
fi

"$test_image" format-identity \
  --source-revision "$test_revision" \
  --source-dirty false \
  --agent-mode offline \
  --fedora-release 44 \
  --build-host-release 44 \
  --base-iso-filename Fedora-Workstation-Live-x86_64-44-1.1.iso \
  --base-iso-bytes 2048 \
  --base-iso-sha256 0000000000000000000000000000000000000000000000000000000000000001 \
  --payload-relpath LiveOS/squashfs.img \
  --payload-bytes 1024 \
  --payload-sha256 0000000000000000000000000000000000000000000000000000000000000002 \
  --container-format erofs-rootfs \
  --baked-at-utc 2026-08-23T00:00:00Z \
  --output-iso-filename sos-fedora-workstation-live-abcdef123456.iso \
  --output-iso-bytes 4096 \
  --output-iso-sha256 0000000000000000000000000000000000000000000000000000000000000003 \
  >"$test_root/identity.env"
for test_key in \
  image_kind=live-boot \
  campaign_class=live-boot \
  not_installed_product=true \
  fedora_release=44 \
  build_host_release=44 \
  source_revision="$test_revision" \
  source_dirty=false \
  agent_mode=offline \
  payload_relpath=LiveOS/squashfs.img \
  payload_sha256=0000000000000000000000000000000000000000000000000000000000000002 \
  output_iso_sha256=0000000000000000000000000000000000000000000000000000000000000003; do
  grep -Fx "$test_key" "$test_root/identity.env" >/dev/null
done
if "$test_image" format-identity \
  --source-revision "$test_revision" \
  --source-dirty true \
  --agent-mode offline \
  --fedora-release 44 \
  --build-host-release 44 \
  --base-iso-filename Fedora-Workstation-Live-x86_64-44-1.1.iso \
  --base-iso-bytes 2048 \
  --base-iso-sha256 0000000000000000000000000000000000000000000000000000000000000001 \
  --payload-relpath LiveOS/squashfs.img \
  --container-format erofs-rootfs \
  >"$test_root/dirty-identity.txt" 2>&1; then
  printf 'error: format-identity accepted a dirty source revision\n' >&2
  exit 1
fi
if "$test_image" format-identity \
  --source-revision "$test_revision" \
  --source-dirty false \
  --agent-mode live \
  --fedora-release 44 \
  --build-host-release 44 \
  --base-iso-filename Fedora-Workstation-Live-x86_64-44-1.1.iso \
  --base-iso-bytes 2048 \
  --base-iso-sha256 0000000000000000000000000000000000000000000000000000000000000001 \
  --payload-relpath LiveOS/squashfs.img \
  --container-format erofs-rootfs \
  >"$test_root/live-agent-identity.txt" 2>&1; then
  printf 'error: format-identity accepted a live agent bake\n' >&2
  exit 1
fi

if "$test_image" format-identity \
  --source-revision "$test_revision" \
  --source-dirty false \
  --agent-mode offline \
  --fedora-release 44 \
  --build-host-release 44 \
  --base-iso-filename Fedora-Workstation-Live-x86_64-44-1.1.iso \
  --base-iso-bytes 2048 \
  --base-iso-sha256 0000000000000000000000000000000000000000000000000000000000000001 \
  --payload-relpath LiveOS/squashfs.img \
  --container-format squashfs-rootfs-img \
  >"$test_root/flat-identity.txt" 2>&1; then
  printf 'error: format-identity accepted a non-EROFS payload\n' >&2
  exit 1
fi
grep -F 'only metadata-preserving erofs-rootfs is supported' \
  "$test_root/flat-identity.txt" >/dev/null

if command -v fsck.erofs >/dev/null 2>&1 \
  && command -v mkfs.erofs >/dev/null 2>&1 \
  && mkfs.erofs --help 2>&1 | grep -F 'all-fragments' >/dev/null; then
  test_packed_source="$test_root/packed-source"
  test_packed_dest="$test_root/packed-rootfs"
  test_packed_image="$test_root/packed.erofs"
  mkdir "$test_packed_source" "$test_packed_dest"
  printf 'packed extraction regression\n' >"$test_packed_source/file"
  mkfs.erofs -zlzma -Eall-fragments \
    "$test_packed_image" "$test_packed_source" >/dev/null
  fsck.erofs --path=/ --extract="$test_packed_dest" --xattrs --preserve \
    "$test_packed_image" >/dev/null
  cmp "$test_packed_source/file" "$test_packed_dest/file"
fi

if command -v getfattr >/dev/null 2>&1 \
  && command -v rsync >/dev/null 2>&1 \
  && command -v setfattr >/dev/null 2>&1; then
  test_metadata_source="$test_root/metadata-source"
  test_metadata_dest="$test_root/metadata-dest"
  mkdir "$test_metadata_source" "$test_metadata_dest"
  printf 'metadata extraction regression\n' >"$test_metadata_source/file"
  chmod 0750 "$test_metadata_source/file"
  ln "$test_metadata_source/file" "$test_metadata_source/hardlink"
  setfattr -n user.sos_probe -v preserved "$test_metadata_source/file"
  test_metadata_source_label=""
  if command -v chcon >/dev/null 2>&1 \
    && command -v selinuxenabled >/dev/null 2>&1 \
    && selinuxenabled \
    && chcon system_u:object_r:fusefs_t:s0 "$test_metadata_source/file"; then
    test_metadata_source_label="$(stat -c %C "$test_metadata_source/file")"
  fi
  rsync -aHAXS --numeric-ids \
    --filter='-x security.selinux' \
    --filter='-x system.*' \
    "$test_metadata_source/" "$test_metadata_dest/"
  rsync -aHASni --numeric-ids \
    "$test_metadata_source/" "$test_metadata_dest/" \
    >"$test_root/metadata-audit.txt"
  [[ ! -s "$test_root/metadata-audit.txt" ]]
  for test_metadata_side in source dest; do
    test_metadata_dir="$test_metadata_source"
    [[ "$test_metadata_side" == source ]] || test_metadata_dir="$test_metadata_dest"
    (
      cd "$test_metadata_dir"
      getfattr -hRPd -m- .
    ) | awk '
      /^# file: / { path = substr($0, 9); next }
      /^(user|trusted|security)\./ && \
        !/^security\.selinux=/ && \
        !/^trusted\.SGI_ACL_/ {
        print path "\t" $0
      }
    ' | LC_ALL=C sort >"$test_root/metadata-$test_metadata_side-xattrs.txt"
  done
  cmp "$test_root/metadata-source-xattrs.txt" "$test_root/metadata-dest-xattrs.txt"
  cmp "$test_metadata_source/file" "$test_metadata_dest/file"
  [[ "$(stat -c %a "$test_metadata_dest/file")" == 750 ]]
  [[ "$(stat -c %i "$test_metadata_dest/file")" \
    == "$(stat -c %i "$test_metadata_dest/hardlink")" ]]
  [[ "$(getfattr -n user.sos_probe --only-values "$test_metadata_dest/file")" \
    == preserved ]]
  if [[ -n "$test_metadata_source_label" ]]; then
    [[ "$(stat -c %C "$test_metadata_dest/file")" \
      != "$test_metadata_source_label" ]]
  fi
fi

mkdir "$test_root/bin"
# The single-quoted expansions belong to the generated mock, not this test process.
# shellcheck disable=SC2016
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'if [[ "${1:-}" == --ls && "${TEST_PAYLOAD_LAYOUT:-}" != erofs-root ]]; then' \
  '  exit 1' \
  'fi' \
  'exit 0' >"$test_root/bin/dump.erofs"
chmod 0755 "$test_root/bin/dump.erofs"
# Simulate sudo crossing a root-owned, non-traversable directory boundary. The
# wrapper temporarily grants its owning test process access, runs the real
# command, and restores the boundary. An ordinary [[ -f path ]] cannot pass.
# shellcheck disable=SC2016
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  '[[ -n "${TEST_SUDO_UNLOCK_DIR:-}" && -n "${TEST_SUDO_LOG:-}" ]] || exec "$@"' \
  'printf "%s\\n" "$*" >>"$TEST_SUDO_LOG"' \
  'chmod 0700 "$TEST_SUDO_UNLOCK_DIR"' \
  'set +e' \
  '"$@"' \
  'status=$?' \
  'set -e' \
  'chmod 000 "$TEST_SUDO_UNLOCK_DIR"' \
  'exit "$status"' >"$test_root/bin/sudo"
chmod 0755 "$test_root/bin/sudo"
: >"$test_root/payload.img"
PATH="$test_root/bin:$PATH" TEST_PAYLOAD_LAYOUT=erofs-root \
  "$test_image" check-payload --payload "$test_root/payload.img" \
  >"$test_root/erofs-payload.txt"
grep -F 'container_format=erofs-rootfs' "$test_root/erofs-payload.txt" >/dev/null
if PATH="$test_root/bin:$PATH" TEST_PAYLOAD_LAYOUT=not-rootfs \
  "$test_image" check-payload --payload "$test_root/payload.img" \
  >"$test_root/not-rootfs-payload.txt" 2>&1; then
  printf 'error: check-payload accepted EROFS without a Fedora rootfs\n' >&2
  exit 1
fi
grep -F 'EROFS payload is not a flat Fedora root filesystem' \
  "$test_root/not-rootfs-payload.txt" >/dev/null

"$test_image" write-offline-user-state --home-root "$test_root/skel" \
  >"$test_root/skel.txt"
grep -Fx 'SOS_AGENT_MODEL=faux' "$test_root/skel/.local/state/sos/agent/config.env" >/dev/null
grep -Fx 'SOS_AGENT_FAKE_SOURCE=/usr/share/sos/experiences/daily-flow.luau' \
  "$test_root/skel/.local/state/sos/agent/config.env" >/dev/null
grep -Fx '{}' "$test_root/skel/.local/state/sos/output.json" >/dev/null
[[ "$(stat -c %a "$test_root/skel/.local/state/sos/agent/config.env")" == 600 ]]

# Calling the user-state helper from a bake must not replace the active rootfs
# used by the surrounding staging function.
(
  set -- write-offline-user-state --home-root "$test_root/source-skel"
  # shellcheck source=/dev/null
  source "$test_image" >/dev/null
  live_image_rootfs="$test_root/rootfs-sentinel"
  live_image_write_offline_user_state \
    --home-root "$test_root/nested-skel" >/dev/null
  [[ "$live_image_rootfs" == "$test_root/rootfs-sentinel" ]]
)

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
  'ID=fedora' \
  'VERSION_ID=44' \
  'VARIANT_ID=workstation' >"$test_rootfs/etc/os-release"
printf '%s\n' \
  "source_revision=$test_revision" \
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

test_locked_dir="$test_rootfs/etc/skel/.local"
test_locked_config="$test_locked_dir/state/sos/agent/config.env"
test_sudo_log="$test_root/sudo.log"
chmod 0600 "$test_locked_config"
chmod 000 "$test_locked_dir"
if [[ -f "$test_locked_config" ]]; then
  printf 'error: locked private config remained visible to an ordinary file test\n' >&2
  exit 1
fi
PATH="$test_root/bin:$PATH" \
  TEST_SUDO_UNLOCK_DIR="$test_locked_dir" \
  TEST_SUDO_LOG="$test_sudo_log" \
  "$test_image" check-rootfs --root "$test_rootfs" \
  >"$test_root/check-privileged-config.txt"
chmod 0700 "$test_locked_dir"
grep -F 'linux_live_image_rootfs_checked=PASS' \
  "$test_root/check-privileged-config.txt" >/dev/null
grep -Fx "test -f $test_locked_config" "$test_sudo_log" >/dev/null
grep -Fx \
  "grep -Fx SOS_AGENT_FAKE_SOURCE=/usr/share/sos/experiences/daily-flow.luau $test_locked_config" \
  "$test_sudo_log" >/dev/null

# When subordinate-ID user namespaces and a setuid-capable workspace are
# available, repeat the check with an actual namespace-root-owned 0600 fixture
# and a namespace-root setuid command trampoline standing in for sudo.
test_user="$(id -un)"
test_uid="$(id -u)"
test_gid="$(id -g)"
test_subuid_start="$(awk -F: -v user="$test_user" '$1 == user && $3 >= 1001 { print $2; exit }' /etc/subuid 2>/dev/null || true)"
test_subgid_start="$(awk -F: -v user="$test_user" '$1 == user && $3 >= 1001 { print $2; exit }' /etc/subgid 2>/dev/null || true)"
if [[ "$test_uid" -ne 0 && -n "$test_subuid_start" && -n "$test_subgid_start" ]] \
  && command -v unshare >/dev/null 2>&1 \
  && command -v setpriv >/dev/null 2>&1 \
  && command -v findmnt >/dev/null 2>&1 \
  && ! findmnt -T "$test_repo_root" -no OPTIONS | tr ',' '\n' | grep -Fx nosuid >/dev/null; then
  mkdir -p "$test_repo_root/.cache"
  test_namespace_dir="$(mktemp -d -p "$test_repo_root/.cache" sos-root-owned-config.XXXXXX)"
  chmod 0755 "$test_namespace_dir"
  # The variables in this script belong to the namespace process.
  # shellcheck disable=SC2016
  unshare \
    --map-users="0:$test_uid:1" \
    --map-users="1:$test_subuid_start:65536" \
    --map-groups="0:$test_gid:1" \
    --map-groups="1:$test_subgid_start:65536" \
    bash -c '
      set -euo pipefail
      rootfs="$1"
      image_root="$2"
      helper_dir="$3"
      result="$4"
      env_binary="$5"
      cleanup_namespace_fixture() {
        chown -R 0:0 "$rootfs/home/liveuser/.local" 2>/dev/null || true
        rm -f -- "$helper_dir/sudo"
      }
      trap cleanup_namespace_fixture EXIT
      chmod 0755 "$(dirname "$rootfs")"
      find "$rootfs" -type d -exec chmod 0755 {} +
      find "$rootfs/etc/skel/.local" -type d -exec chmod 0700 {} +
      chmod 0600 "$rootfs/etc/skel/.local/state/sos/agent/config.env"
      chown -R 1000:1000 "$rootfs/home/liveuser/.local"
      cp -- "$env_binary" "$helper_dir/sudo"
      chown 0:1000 "$helper_dir" "$helper_dir/sudo"
      chmod 0710 "$helper_dir"
      chmod 4750 "$helper_dir/sudo"
      exec 8<"$helper_dir"
      exec 9<"$image_root"
      setpriv --reuid=1000 --regid=1000 --clear-groups \
        env PATH="/proc/self/fd/8:/usr/bin:/bin" \
        /proc/self/fd/9/tools/linux-live-image check-rootfs --root "$rootfs" \
        >"$result"
      grep -F "linux_live_image_rootfs_checked=PASS" "$result" >/dev/null
    ' bash \
    "$test_rootfs" \
    "$test_repo_root" \
    "$test_namespace_dir" \
    "$test_root/check-root-owned-config.txt" \
    "$(command -v env)"
  rm -r -- "$test_namespace_dir"
  test_namespace_dir=""
fi

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
grep -F 'sudo chmod -R u=rwX,go=rX' "$test_install" >/dev/null
grep -F 'installed artifact is not readable:' "$test_install" >/dev/null
"$test_image" 2>"$test_root/image-usage.txt" || true
grep -F -- '--source-sha256 SHA256' "$test_root/image-usage.txt" >/dev/null
grep -F 'rootfs extraction destination is not empty' "$test_image" >/dev/null
grep -F "sudo mount -t erofs -o loop,ro \"\$payload\" \"\$mountpoint\"" \
  "$test_image" >/dev/null
grep -F 'sudo rsync -aHAXS --numeric-ids' "$test_image" >/dev/null
grep -F 'sudo rsync -aHASni --numeric-ids' "$test_image" >/dev/null
grep -F 'rootfs metadata audit differs after copy' "$test_image" >/dev/null
grep -F 'rootfs xattr audit differs after copy' "$test_image" >/dev/null
grep -F 'sudo getfattr -hRPd -m- .' "$test_image" >/dev/null
grep -F '!/^trusted\.SGI_ACL_/' "$test_image" >/dev/null
grep -F -- "--filter='-x security.selinux'" "$test_image" >/dev/null
grep -F -- "--filter='-x system.*'" "$test_image" >/dev/null
grep -F "sudo umount -- \"\$mountpoint\"" "$test_image" >/dev/null
grep -F 'sudo setfiles -F -r' "$test_image" >/dev/null
grep -F "sudo fsck.erofs \"\$output\"" "$test_image" >/dev/null
grep -F 'implantisomd5 --force' "$test_image" >/dev/null
grep -F "checkisomd5 \"\$output_iso\"" "$test_image" >/dev/null

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
grep -F -- '--source-sha256' "$test_repo_root/docs/linux-live-image.md" >/dev/null
grep -F 'embedded media' "$test_repo_root/docs/linux-live-image.md" >/dev/null
grep -F 'recovery_page_flip' "$test_repo_root/tools/linux-hardware-gate" >/dev/null
grep -F 'completed significant DRM page flip.*recovery_view=true' \
  "$test_repo_root/tools/linux-hardware-gate" >/dev/null

printf 'linux_live_image_host_tests=PASS\n'
