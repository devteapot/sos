#!/usr/bin/env bash

set -euo pipefail

test_repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
test_image="$test_repo_root/tools/linux-live-image"
test_deploy="$test_repo_root/tools/linux-live-deploy"
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
bash -n "$test_deploy"
bash -n "$test_install"
python3 - "$test_repo_root/packaging/xdg/framework12-pikvm-monitors.xml" <<'PY'
import sys
import xml.etree.ElementTree as ET

root = ET.parse(sys.argv[1]).getroot()
assert root.tag == "monitors" and root.attrib == {"version": "2"}
assert root.find("policy") is None
logical_monitors = root.findall("./configuration/logicalmonitor")
assert len(logical_monitors) == 1
monitors = logical_monitors[0].findall("./monitor")
assert [monitor.findtext("./monitorspec/connector") for monitor in monitors] == [
    "DP-1",
    "eDP-1",
]
assert all(monitor.findtext("./mode/width") == "1920" for monitor in monitors)
assert all(monitor.findtext("./mode/height") == "1080" for monitor in monitors)
PY
"$test_deploy" components >"$test_root/deploy-components.txt"
for test_component in \
  compositor experience-host provider supervisor session authoring provider-probe \
  login-session session-target session-shutdown-target hardware-gate stock-base api-doc \
  display-defaults; do
  grep -E "^${test_component}[[:space:]]+/" \
    "$test_root/deploy-components.txt" >/dev/null
done
if "$test_deploy" deploy --target root@example.test --component compositor \
  >"$test_root/deploy-unsafe-target.txt" 2>&1; then
  printf 'error: development deploy accepted a non-liveuser SSH target\n' >&2
  exit 1
fi
grep -F 'target must be liveuser@HOST' "$test_root/deploy-unsafe-target.txt" >/dev/null
if "$test_deploy" deploy --target liveuser@example.test --component unknown \
  >"$test_root/deploy-unknown-component.txt" 2>&1; then
  printf 'error: development deploy accepted an unknown component\n' >&2
  exit 1
fi
grep -F 'unknown component: unknown' "$test_root/deploy-unknown-component.txt" >/dev/null

test_deploy_bin="$test_root/deploy-bin"
test_deploy_remote="$test_root/deploy-remote"
test_deploy_target="$test_root/deploy-target"
test_deploy_state="$test_root/deploy-stage-path"
mkdir -p "$test_deploy_bin" "$test_deploy_remote" "$test_deploy_target/release"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'binary=""' \
  'while [[ "$#" -gt 0 ]]; do' \
  '  if [[ "$1" == --bin ]]; then binary="$2"; shift 2; else shift; fi' \
  'done' \
  '[[ -n "$binary" ]]' \
  'mkdir -p "$CARGO_TARGET_DIR/release"' \
  'printf "mock binary %s\\n" "$binary" >"$CARGO_TARGET_DIR/release/$binary"' \
  'chmod 0755 "$CARGO_TARGET_DIR/release/$binary"' \
  >"$test_deploy_bin/cargo"
chmod 0755 "$test_deploy_bin/cargo"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'if [[ "${1:-}" == -tt ]]; then shift; fi' \
  'while [[ "${1:-}" == -o ]]; do shift 2; done' \
  'if [[ "${1:-}" == -O ]]; then exit 0; fi' \
  'target="$1"; shift' \
  'command="$*"' \
  '[[ "$target" == liveuser@mock-target ]]' \
  'case "$command" in' \
  '  true) exit 0 ;;' \
  '  *"pgrep -f"*) exit 0 ;;' \
  '  "cat /usr/share/doc/sos/image-identity.env"|"cat '\''/usr/share/doc/sos/image-identity.env'\''")' \
  '    printf "%s\\n" image_kind=development-live promotion_eligible=false mutable_runtime=true source_revision=1111111111111111111111111111111111111111' \
  '    ;;' \
  '  "umask 077; mktemp -d -p /tmp sos-development-deploy.XXXXXX")' \
  '    mktemp -d -p /tmp sos-development-deploy.XXXXXX' \
  '    ;;' \
  '  "set -euo pipefail;"*)' \
  '    stage="$(cat "$TEST_DEPLOY_STATE")"' \
  '    mkdir -p "$TEST_DEPLOY_REMOTE/usr/local/libexec/sos" "$TEST_DEPLOY_REMOTE/usr/local/lib/systemd/user" "$TEST_DEPLOY_REMOTE/usr/share/doc/sos" "$TEST_DEPLOY_REMOTE/usr/share/sos/experiences" "$TEST_DEPLOY_REMOTE/etc/xdg"' \
  '    for source in "$stage"/sos-*; do cp -- "$source" "$TEST_DEPLOY_REMOTE/usr/local/libexec/sos/$(basename "$source")"; done' \
  '    [[ ! -f "$stage/linux-hardware-gate" ]] || cp -- "$stage/linux-hardware-gate" "$TEST_DEPLOY_REMOTE/usr/local/libexec/sos/"' \
  '    [[ ! -f "$stage/sos-session.target" ]] || cp -- "$stage/sos-session.target" "$TEST_DEPLOY_REMOTE/usr/local/lib/systemd/user/"' \
  '    [[ ! -f "$stage/sos-session-shutdown.target" ]] || cp -- "$stage/sos-session-shutdown.target" "$TEST_DEPLOY_REMOTE/usr/local/lib/systemd/user/"' \
  '    [[ ! -f "$stage/default.luau" ]] || cp -- "$stage/default.luau" "$TEST_DEPLOY_REMOTE/usr/share/sos/experiences/"' \
  '    [[ ! -f "$stage/experience-api.md" ]] || cp -- "$stage/experience-api.md" "$TEST_DEPLOY_REMOTE/usr/share/doc/sos/"' \
  '    [[ ! -f "$stage/monitors.xml" ]] || cp -- "$stage/monitors.xml" "$TEST_DEPLOY_REMOTE/etc/xdg/"' \
  '    cp -- "$stage/development-deployment.env" "$TEST_DEPLOY_REMOTE/usr/share/doc/sos/"' \
  '    cp -- "$stage/development-deployment-manifest.tsv" "$TEST_DEPLOY_REMOTE/usr/share/doc/sos/"' \
  '    rm -r -- "$stage"' \
  '    ;;' \
  '  "sha256sum "*)' \
  '    path="${command:11:-1}"' \
  '    sha256sum "$TEST_DEPLOY_REMOTE$path" | sed "s|$TEST_DEPLOY_REMOTE||"' \
  '    ;;' \
  '  "rm -r -- "*)' \
  '    path="${command:10:-1}"' \
  '    [[ ! -e "$path" ]] || rm -r -- "$path"' \
  '    ;;' \
  '  *) printf "unexpected mock SSH command: %s\\n" "$command" >&2; exit 1 ;;' \
  'esac' \
  >"$test_deploy_bin/ssh"
chmod 0755 "$test_deploy_bin/ssh"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'while [[ "${1:-}" == -o ]]; do shift 2; done' \
  'sources=()' \
  'while [[ "$#" -gt 1 ]]; do sources+=("$1"); shift; done' \
  'destination="${1#*:}"' \
  'cp -- "${sources[@]}" "$destination/"' \
  'printf "%s\\n" "$destination" >"$TEST_DEPLOY_STATE"' \
  >"$test_deploy_bin/scp"
chmod 0755 "$test_deploy_bin/scp"
PATH="$test_deploy_bin:$PATH" \
CARGO_TARGET_DIR="$test_deploy_target" \
TEST_DEPLOY_REMOTE="$test_deploy_remote" \
TEST_DEPLOY_STATE="$test_deploy_state" \
SOS_DEVELOPMENT_DEPLOY_ARTIFACTS_DIR="$test_root/deploy-artifacts" \
  "$test_deploy" deploy \
    --target liveuser@mock-target \
    --component experience-host \
    --component compositor \
    --component provider-probe \
    --component login-session \
    --component session-target \
    --component session-shutdown-target \
    --component hardware-gate \
    --component stock-base \
    --component api-doc \
    --component display-defaults \
    >"$test_root/deploy-pass.txt"
grep -F 'linux_development_live_deployed=PASS' "$test_root/deploy-pass.txt" >/dev/null
grep -F 'promotion_eligible=false' "$test_root/deploy-pass.txt" >/dev/null
for test_binary in \
  sos-experience-host sos-compositor sos-linux-provider-probe sos-login-session linux-hardware-gate; do
  [[ -x "$test_deploy_remote/usr/local/libexec/sos/$test_binary" ]]
done
[[ -f "$test_deploy_remote/usr/share/sos/experiences/default.luau" ]]
[[ -f "$test_deploy_remote/usr/share/doc/sos/experience-api.md" ]]
cmp -s \
  "$test_repo_root/packaging/xdg/framework12-pikvm-monitors.xml" \
  "$test_deploy_remote/etc/xdg/monitors.xml"
[[ -f "$test_deploy_remote/usr/local/lib/systemd/user/sos-session.target" ]]
[[ -f "$test_deploy_remote/usr/local/lib/systemd/user/sos-session-shutdown.target" ]]
test_deployment_metadata="$test_deploy_remote/usr/share/doc/sos/development-deployment.env"
test_deployment_manifest="$test_deploy_remote/usr/share/doc/sos/development-deployment-manifest.tsv"
grep -Fx 'image_kind=development-live' "$test_deployment_metadata" >/dev/null
grep -Fx 'promotion_eligible=false' "$test_deployment_metadata" >/dev/null
[[ "$(wc -l <"$test_deployment_manifest")" -eq 10 ]]
while IFS=$'\t' read -r test_path test_bytes test_sha; do
  [[ "$(stat -c %s "$test_deploy_remote$test_path")" == "$test_bytes" ]]
  [[ "$(sha256sum "$test_deploy_remote$test_path" | cut -d ' ' -f 1)" == "$test_sha" ]]
done <"$test_deployment_manifest"
"$test_image" doctor --layout-only >"$test_root/doctor.txt"
grep -Fx 'linux_live_image_doctor=PASS mode=layout-only bake_ready=false' \
  "$test_root/doctor.txt" >/dev/null
if "$test_image" doctor >"$test_root/bake-doctor.txt"; then
  grep -Fx 'linux_live_image_doctor=PASS bake_ready=true' "$test_root/bake-doctor.txt" >/dev/null
else
  grep -Fx 'linux_live_image_doctor=FAIL bake_ready=false' "$test_root/bake-doctor.txt" >/dev/null
fi
grep -F 'nodejs' "$test_root/doctor.txt" >/dev/null
grep -F 'openssh-server' "$test_root/doctor.txt" >/dev/null
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
  --wifi-autoconnect true \
  --baked-at-utc 2026-08-23T00:00:00Z \
  --output-iso-filename sos-development-live-abcdef123456.iso \
  --output-iso-bytes 4096 \
  --output-iso-sha256 0000000000000000000000000000000000000000000000000000000000000003 \
  >"$test_root/identity.env"
for test_key in \
  image_kind=development-live \
  campaign_class=development-live \
  not_installed_product=true \
  promotion_eligible=false \
  mutable_runtime=true \
  ssh_enabled=true \
  wifi_autoconnect=true \
  network_credentials_embedded=true \
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
  --container-format erofs-rootfs \
  --baked-at-utc 2026-08-23T00:00:00Z \
  >"$test_root/identity-no-wifi.env"
grep -Fx 'wifi_autoconnect=false' "$test_root/identity-no-wifi.env" >/dev/null
grep -Fx 'network_credentials_embedded=false' \
  "$test_root/identity-no-wifi.env" >/dev/null
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

  if command -v strings >/dev/null 2>&1 \
    && mkfs.erofs --help 2>&1 | grep -F -- '--file-contexts' >/dev/null; then
    test_label_source="$test_root/label-source"
    test_label_dest="$test_root/label-dest"
    test_label_image="$test_root/label.erofs"
    test_file_contexts="$test_root/file_contexts"
    mkdir "$test_label_source" "$test_label_dest"
    printf 'SELinux label regression\n' >"$test_label_source/probe"
    printf '/probe -- system_u:object_r:bin_t:s0\n' >"$test_file_contexts"
    mkfs.erofs --file-contexts="$test_file_contexts" \
      "$test_label_image" "$test_label_source" >/dev/null
    fsck.erofs --xattrs "$test_label_image" >/dev/null
    strings "$test_label_image" | \
      grep -Fx 'system_u:object_r:bin_t:s0' >/dev/null
    if command -v selinuxenabled >/dev/null 2>&1 && selinuxenabled; then
      fsck.erofs --extract="$test_label_dest" --xattrs \
        --no-preserve-owner --no-preserve-perms "$test_label_image" >/dev/null
      [[ "$(getfattr -h -n security.selinux --only-values \
        "$test_label_dest/probe")" == system_u:object_r:bin_t:s0 ]]
    fi
  fi
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
  'if [[ "${1:-}" == stat && "$*" == *"/var/lib/livesys/livesys-session-extra" ]]; then' \
  '  printf "root:root:700\\n"' \
  '  exit 0' \
  'fi' \
  'if [[ "${1:-}" == stat && "$*" == *"/etc/NetworkManager/system-connections/60-sos-development-live.nmconnection" ]]; then' \
  '  if [[ "$*" == *"%U:%G:%a"* ]]; then printf "root:root:600\\n"; else printf "600\\n"; fi' \
  '  exit 0' \
  'fi' \
  'if [[ "${1:-}" == stat && "$*" == *"/etc/NetworkManager/system-connections" ]]; then' \
  '  printf "root:root:700\\n"' \
  '  exit 0' \
  'fi' \
  'if [[ -z "${TEST_SUDO_UNLOCK_DIR:-}" || -z "${TEST_SUDO_LOG:-}" ]]; then' \
  '  if [[ "${1:-}" == install ]]; then' \
  '    shift' \
  '    filtered=()' \
  '    while [[ "$#" -gt 0 ]]; do' \
  '      case "$1" in -o|-g) shift 2 ;; *) filtered+=("$1"); shift ;; esac' \
  '    done' \
  '    exec install "${filtered[@]}"' \
  '  fi' \
  '  exec "$@"' \
  'fi' \
  'printf "%s\\n" "$*" >>"$TEST_SUDO_LOG"' \
  'chmod 0700 "$TEST_SUDO_UNLOCK_DIR"' \
  'set +e' \
  '"$@"' \
  'status=$?' \
  'set -e' \
  'chmod 000 "$TEST_SUDO_UNLOCK_DIR"' \
  'exit "$status"' >"$test_root/bin/sudo"
chmod 0755 "$test_root/bin/sudo"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  '[[ "$*" == "passwd -6 -stdin" ]]' \
  'IFS= read -r password' \
  '[[ -n "$password" ]]' \
  'printf "\\x24%s\\x24%s\\x24%s\\n" 6 development-salt development-hash' \
  >"$test_root/bin/openssl"
chmod 0755 "$test_root/bin/openssl"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'printf "%s\\n" "$*" >>"$TEST_FIREWALL_LOG"' \
  '[[ " $* " == *" --service=ssh "* ]]' \
  >"$test_root/bin/firewall-offline-cmd"
chmod 0755 "$test_root/bin/firewall-offline-cmd"
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
  "$test_rootfs/usr/bin" \
  "$test_rootfs/usr/libexec/livesys" \
  "$test_rootfs/usr/share/wayland-sessions" \
  "$test_rootfs/usr/share/sos/experiences" \
  "$test_rootfs/usr/share/doc/sos" \
  "$test_rootfs/usr/lib/systemd/system" \
  "$test_rootfs/usr/lib/firewalld" \
  "$test_rootfs/etc/skel" \
  "$test_rootfs/etc/gdm" \
  "$test_rootfs/etc/xdg" \
  "$test_rootfs/etc/ssh" \
  "$test_rootfs/etc/firewalld" \
  "$test_rootfs/home/liveuser"
: >"$test_rootfs/usr/local/libexec/sos/sos-login-session"
: >"$test_rootfs/usr/local/libexec/sos/sos-agent-login"
: >"$test_rootfs/usr/local/libexec/sos/linux-hardware-gate"
: >"$test_rootfs/usr/local/libexec/sos-agent/dist/agent-runner.cjs"
: >"$test_rootfs/usr/share/wayland-sessions/sos.desktop"
: >"$test_rootfs/usr/share/sos/experiences/daily-flow.luau"
cp -- "$test_repo_root/packaging/xdg/framework12-pikvm-monitors.xml" \
  "$test_rootfs/etc/xdg/monitors.xml"
: >"$test_rootfs/usr/lib/systemd/system/gdm.service"
: >"$test_rootfs/usr/lib/systemd/system/sshd.service"
: >"$test_rootfs/usr/lib/systemd/system/NetworkManager.service"
: >"$test_rootfs/usr/bin/systemctl"
chmod 0755 "$test_rootfs/usr/bin/systemctl"
printf '%s\n' \
  'root:x:0:0:root:/root:/bin/bash' >"$test_rootfs/etc/passwd"
printf '%s\n' \
  'root:*:20000:0:99999:7:::' >"$test_rootfs/etc/shadow"
printf '%s\n' \
  '#!/usr/bin/sh' \
  'useradd ${USERADDARGS:+"$USERADDARGS"} -c "Live System User" liveuser' \
  '. /var/lib/livesys/livesys-session-extra' \
  >"$test_rootfs/usr/libexec/livesys/livesys-main"
printf '%s\n' \
  '[daemon]' \
  'AutomaticLoginEnable=True' \
  'AutomaticLogin=liveuser' >"$test_rootfs/etc/gdm/custom.conf"
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
printf 'development-password\n' >"$test_root/liveuser-password"
test_network_profile="$test_root/development-wifi.nmconnection"
printf '%s\n' \
  '[connection]' \
  'id=SOS development Wi-Fi' \
  'uuid=11111111-2222-4333-8444-555555555555' \
  'type=wifi' \
  'autoconnect=true' \
  '' \
  '[wifi]' \
  'mode=infrastructure' \
  'ssid=Test Network' \
  '' \
  '[wifi-security]' \
  'key-mgmt=wpa-psk' \
  'psk=test-network-password' \
  '' \
  '[ipv4]' \
  'method=auto' \
  '' \
  '[ipv6]' \
  'method=auto' >"$test_network_profile"
chmod 0644 "$test_network_profile"
: >"$test_root/firewall.log"
if PATH="$test_root/bin:$PATH" TEST_FIREWALL_LOG="$test_root/firewall.log" \
  "$test_image" configure-development-access \
    --root "$test_rootfs" \
    --password-file "$test_root/liveuser-password" \
    --networkmanager-profile-file "$test_network_profile" \
    >"$test_root/configure-public-network-profile.txt" 2>&1; then
  printf 'error: development access accepted a public network profile\n' >&2
  exit 1
fi
grep -F 'must not be accessible by group or other users' \
  "$test_root/configure-public-network-profile.txt" >/dev/null
chmod 0600 "$test_network_profile"
"$test_image" check-networkmanager-profile \
  --profile-file "$test_network_profile" \
  >"$test_root/check-network-profile.txt"
grep -Fx \
  'linux_live_image_network_profile_checked=PASS wifi_autoconnect=true network_credentials_embedded=true' \
  "$test_root/check-network-profile.txt" >/dev/null
ln -s development-wifi.nmconnection "$test_root/symlink-network-profile"
if PATH="$test_root/bin:$PATH" TEST_FIREWALL_LOG="$test_root/firewall.log" \
  "$test_image" configure-development-access \
    --root "$test_rootfs" \
    --password-file "$test_root/liveuser-password" \
    --networkmanager-profile-file "$test_root/symlink-network-profile" \
    >"$test_root/configure-symlink-network-profile.txt" 2>&1; then
  printf 'error: development access accepted a symlink network profile\n' >&2
  exit 1
fi
grep -F 'readable regular file, not a symlink' \
  "$test_root/configure-symlink-network-profile.txt" >/dev/null
sed 's/autoconnect=true/autoconnect=false/' "$test_network_profile" \
  >"$test_root/disabled-network-profile.nmconnection"
chmod 0600 "$test_root/disabled-network-profile.nmconnection"
if PATH="$test_root/bin:$PATH" TEST_FIREWALL_LOG="$test_root/firewall.log" \
  "$test_image" configure-development-access \
    --root "$test_rootfs" \
    --password-file "$test_root/liveuser-password" \
    --networkmanager-profile-file "$test_root/disabled-network-profile.nmconnection" \
    >"$test_root/configure-disabled-network-profile.txt" 2>&1; then
  printf 'error: development access accepted disabled Wi-Fi autoconnect\n' >&2
  exit 1
fi
grep -F 'NetworkManager profile must enable autoconnect' \
  "$test_root/configure-disabled-network-profile.txt" >/dev/null
mkdir -p "$test_rootfs/etc/systemd/system/multi-user.target.wants"
ln -s /usr/lib/systemd/system/sshd.service \
  "$test_rootfs/etc/systemd/system/multi-user.target.wants/sshd.service"
if PATH="$test_root/bin:$PATH" TEST_FIREWALL_LOG="$test_root/firewall.log" \
  "$test_image" configure-development-access \
    --root "$test_rootfs" \
    --password-file "$test_root/liveuser-password" \
    --networkmanager-profile-file "$test_network_profile" \
    >"$test_root/configure-premature-ssh.txt" 2>&1; then
  printf 'error: development access accepted pre-provisioning SSH enablement\n' >&2
  exit 1
fi
grep -F 'source image already enables sshd before liveuser provisioning' \
  "$test_root/configure-premature-ssh.txt" >/dev/null
rm -f -- "$test_rootfs/etc/systemd/system/multi-user.target.wants/sshd.service"
PATH="$test_root/bin:$PATH" TEST_FIREWALL_LOG="$test_root/firewall.log" \
  "$test_image" configure-development-access \
    --root "$test_rootfs" \
    --password-file "$test_root/liveuser-password" \
    --networkmanager-profile-file "$test_network_profile" \
    >"$test_root/configure-development-access.txt"
grep -F 'linux_live_image_development_access=PASS' \
  "$test_root/configure-development-access.txt" >/dev/null
grep -F 'wifi_autoconnect=true' \
  "$test_root/configure-development-access.txt" >/dev/null
grep -F -- '--service=ssh' "$test_root/firewall.log" >/dev/null
grep -Fx 'PermitRootLogin no' \
  "$test_rootfs/etc/ssh/sshd_config.d/60-sos-development-live.conf" >/dev/null
test_livesys_hook="$test_rootfs/var/lib/livesys/livesys-session-extra"
[[ "$(stat -c %a "$test_livesys_hook")" == 700 ]]
sh -n "$test_livesys_hook"
grep -Fx "usermod --password '\$6\$development-salt\$development-hash' liveuser || exit 1" \
  "$test_livesys_hook" >/dev/null
grep -Fx 'passwd --lock root >/dev/null || exit 1' "$test_livesys_hook" >/dev/null
grep -Fx 'AutomaticLoginEnable=False' "$test_livesys_hook" >/dev/null
grep -Fx "cat > /etc/gdm/custom.conf <<'SOS_DEVELOPMENT_GDM' || exit 1" \
  "$test_livesys_hook" >/dev/null
grep -Fx 'systemctl enable --now sshd.service >/dev/null || exit 1' \
  "$test_livesys_hook" >/dev/null
grep -Fx 'ssh_activation=livesys-session-extra-final-action' \
  "$test_rootfs/usr/share/doc/sos/development-access.env" >/dev/null
grep -Fx 'wifi_autoconnect=true' \
  "$test_rootfs/usr/share/doc/sos/development-access.env" >/dev/null
grep -Fx 'network_credentials_embedded=true' \
  "$test_rootfs/usr/share/doc/sos/development-access.env" >/dev/null
test_installed_network_profile="$test_rootfs/etc/NetworkManager/system-connections/60-sos-development-live.nmconnection"
[[ "$(stat -c %a "$test_installed_network_profile")" == 600 ]]
grep -Fx 'ssid=Test Network' "$test_installed_network_profile" >/dev/null
[[ ! -e "$test_rootfs/etc/systemd/system/multi-user.target.wants/sshd.service" ]]
[[ ! -L "$test_rootfs/etc/systemd/system/multi-user.target.wants/sshd.service" ]]
[[ ! -e "$test_rootfs/etc/systemd/system/sshd.service.d/60-sos-development-live.conf" ]]
printf 'one\ntwo\n' >"$test_root/two-line-password"
if PATH="$test_root/bin:$PATH" TEST_FIREWALL_LOG="$test_root/firewall.log" \
  "$test_image" configure-development-access \
    --root "$test_rootfs" \
    --password-file "$test_root/two-line-password" \
    --networkmanager-profile-file "$test_network_profile" \
    >"$test_root/two-line-password.txt" 2>&1; then
  printf 'error: development access accepted a multi-line password file\n' >&2
  exit 1
fi
grep -F 'must contain exactly one line' "$test_root/two-line-password.txt" >/dev/null
ln -s liveuser-password "$test_root/symlink-password"
if PATH="$test_root/bin:$PATH" TEST_FIREWALL_LOG="$test_root/firewall.log" \
  "$test_image" configure-development-access \
    --root "$test_rootfs" \
    --password-file "$test_root/symlink-password" \
    --networkmanager-profile-file "$test_network_profile" \
    >"$test_root/symlink-password.txt" 2>&1; then
  printf 'error: development access accepted a symlink password file\n' >&2
  exit 1
fi
grep -F 'regular file, not a symlink' "$test_root/symlink-password.txt" >/dev/null
PATH="$test_root/bin:$PATH" \
  "$test_image" check-rootfs --root "$test_rootfs" >"$test_root/check-pass.txt"
grep -F 'linux_live_image_rootfs_checked=PASS' "$test_root/check-pass.txt" >/dev/null
grep -F 'boot_kind=development-live' "$test_root/check-pass.txt" >/dev/null
grep -F 'promotion_eligible=false' "$test_root/check-pass.txt" >/dev/null
grep -F 'not_installed_product=true' "$test_root/check-pass.txt" >/dev/null

cp -- "$test_installed_network_profile" "$test_root/installed-network-profile.saved"
sed -i 's/^psk=.*/psk=/' "$test_installed_network_profile"
if PATH="$test_root/bin:$PATH" \
  "$test_image" check-rootfs --root "$test_rootfs" \
  >"$test_root/check-network-secret.txt" 2>&1; then
  printf 'error: check-rootfs accepted a network profile without a PSK\n' >&2
  exit 1
fi
grep -F 'must contain a boot-time Wi-Fi PSK' \
  "$test_root/check-network-secret.txt" >/dev/null
cp -- "$test_root/installed-network-profile.saved" "$test_installed_network_profile"

printf '%s\n' '# activation must remain the final hook action' >>"$test_livesys_hook"
if PATH="$test_root/bin:$PATH" \
  "$test_image" check-rootfs --root "$test_rootfs" \
  >"$test_root/check-premature-ssh.txt" 2>&1; then
  printf 'error: check-rootfs accepted SSH activation before the final hook action\n' >&2
  exit 1
fi
grep -F 'does not activate SSH as its final action' \
  "$test_root/check-premature-ssh.txt" >/dev/null
sed -i '$d' "$test_livesys_hook"

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
      chmod 0700 "$rootfs/etc/NetworkManager/system-connections"
      chmod 0600 "$rootfs/etc/NetworkManager/system-connections/60-sos-development-live.nmconnection"
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
grep -F -- '--liveuser-password-file FILE' "$test_root/image-usage.txt" >/dev/null
grep -F -- '--networkmanager-profile-file FILE' "$test_root/image-usage.txt" >/dev/null
grep -F -- 'check-networkmanager-profile --profile-file FILE' \
  "$test_root/image-usage.txt" >/dev/null
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
grep -F 'sudo setfiles -n -m -c "$policy"' "$test_image" >/dev/null
grep -F -- '--file-contexts="$file_contexts"' "$test_image" >/dev/null
grep -F 'sudo fsck.erofs --xattrs "$output"' "$test_image" >/dev/null
grep -F 'implantisomd5 --force' "$test_image" >/dev/null
grep -F "checkisomd5 \"\$output_iso\"" "$test_image" >/dev/null

for test_doc in \
  "$test_repo_root/docs/linux-live-image.md" \
  "$test_repo_root/docs/linux-hardware-gate.md" \
  "$test_repo_root/README.md"; do
  grep -F 'development-live' "$test_doc" >/dev/null
  grep -E 'not an installed product|not_installed_product' "$test_doc" >/dev/null
done
grep -F 'promotion_eligible=false' "$test_repo_root/docs/linux-live-image.md" >/dev/null
grep -F 'future `release`' "$test_repo_root/docs/linux-live-image.md" >/dev/null
grep -F 'tools/linux-live-deploy' "$test_repo_root/docs/linux-live-image.md" >/dev/null
grep -F 'network_credentials_embedded=true' \
  "$test_repo_root/docs/linux-live-image.md" >/dev/null
grep -F "target is not a mutable, non-promotable development-live image" \
  "$test_deploy" >/dev/null
grep -F "log out of SOS before deploying" "$test_deploy" >/dev/null
grep -F 'development-deployment-manifest.tsv' "$test_deploy" >/dev/null
grep -F 'lorax' "$test_repo_root/docs/linux-live-image.md" >/dev/null
grep -F 'install-linux-login-session' "$test_repo_root/docs/linux-live-image.md" >/dev/null
grep -F 'same-boot' "$test_repo_root/docs/linux-hardware-gate.md" >/dev/null
grep -F -- '--source-sha256' "$test_repo_root/docs/linux-live-image.md" >/dev/null
grep -F 'embedded media' "$test_repo_root/docs/linux-live-image.md" >/dev/null
grep -F 'recovery_page_flip' "$test_repo_root/tools/linux-hardware-gate" >/dev/null
grep -F 'completed significant DRM page flip.*recovery_view=true' \
  "$test_repo_root/tools/linux-hardware-gate" >/dev/null

printf 'linux_live_image_host_tests=PASS\n'
