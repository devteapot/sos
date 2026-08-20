#!/usr/bin/env bash

set -euo pipefail

test_repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
test_work="$(mktemp -d -t sos-packaging-test.XXXXXX)"
trap 'rm -rf -- "$test_work"' EXIT

"$test_repo_root/tools/render-linux-packaging" \
  "$test_work/rendered" \
  /usr/lib/sos \
  /usr/lib/sos-agent \
  /usr/lib/sos-agent/node/bin/node >/dev/null

grep -q 'ExecStart=/usr/lib/sos/sos-linux-session' \
  "$test_work/rendered/systemd/sos-session.service"
grep -q 'ExecStart=/usr/lib/sos-agent/node/bin/node /usr/lib/sos-agent/dist/agent-runner.cjs' \
  "$test_work/rendered/systemd/sos-agent.service"
grep -q 'Exec=/usr/lib/sos/sos-login-session' \
  "$test_work/rendered/wayland-sessions/sos.desktop"
# These are intentionally literal shell expressions in the rendered file.
# shellcheck disable=SC2016
grep -q 'sos_login_node="${SOS_NODE_BIN:-/usr/lib/sos-agent/node/bin/node}"' \
  "$test_work/rendered/libexec/sos-login-session"
# shellcheck disable=SC2016
grep -q 'sos_agent_login_node="${SOS_NODE_BIN:-/usr/lib/sos-agent/node/bin/node}"' \
  "$test_work/rendered/libexec/sos-agent-login"
if grep -R -q '/usr/local/libexec/sos\|/usr/local/bin/node' "$test_work/rendered"; then
  printf 'rendered packaging retained source-install paths\n' >&2
  exit 1
fi

# shellcheck source=/dev/null
source "$test_repo_root/images/debian-13/base-images.lock"
[[ "$SOS_IMAGE_ARCH" == arm64 ]]
[[ "$SOS_IMAGE_BASE_SIZE" =~ ^[0-9]+$ ]]
[[ "$SOS_IMAGE_BASE_SHA256" =~ ^[0-9a-f]{64}$ ]]
[[ "$SOS_IMAGE_BASE_SHA512" =~ ^[0-9a-f]{128}$ ]]
[[ "$SOS_IMAGE_DEBIAN_SNAPSHOT" =~ ^[0-9]{8}T[0-9]{6}Z$ ]]
grep -q '^ARG DEBIAN_BUILDER_IMAGE=debian:13\.6-slim@sha256:[0-9a-f]\{64\}$' \
  "$test_repo_root/packaging/debian/Containerfile"
grep -q "^ARG DEBIAN_SNAPSHOT=$SOS_IMAGE_DEBIAN_SNAPSHOT$" \
  "$test_repo_root/packaging/debian/Containerfile"
grep -q '^ENV RUSTUP_HOME=/opt/rustup$' \
  "$test_repo_root/packaging/debian/Containerfile"
grep -q 'rustup toolchain install 1\.95\.0' \
  "$test_repo_root/packaging/debian/Containerfile"

for test_package in \
  sos-runtime \
  sos-agent \
  sos-desktop-session \
  sos-appliance-session \
  sos-image-config; do
  grep -q "package_build_one $test_package " \
    "$test_repo_root/tools/build-linux-packages"
done
if grep -q 'install-linux-login-session\|cargo build\|npm run build' \
  "$test_repo_root/images/debian-13/configure-root"; then
  printf 'image root configuration invokes a source build or installer\n' >&2
  exit 1
fi
grep -q 'CARGO_TARGET_DIR=.*linux-package-builder/target' \
  "$test_repo_root/packaging/debian/container-entrypoint"
grep -q 'CARGO_BUILD_JOBS=.*SOS_CARGO_BUILD_JOBS:-2' \
  "$test_repo_root/packaging/debian/container-entrypoint"
grep -q -- '--env "SOS_SOURCE_REVISION=' \
  "$test_repo_root/tools/build-linux-packages-container"
grep -q -- '--env "SOURCE_DATE_EPOCH=' \
  "$test_repo_root/tools/build-linux-packages-container"
grep -q 'linux_package_install_test_passed' \
  "$test_repo_root/tests/linux-packages-container-test.sh"
grep -q "package_test_base_image='debian:13\\.6-slim@sha256:[0-9a-f]\\{64\\}'" \
  "$test_repo_root/tools/test-linux-packages-container"
grep -q '^ARG DEBIAN_LIVE_IMAGE=debian:13\.6-slim@sha256:[0-9a-f]\{64\}$' \
  "$test_repo_root/packaging/debian-live/Containerfile"
grep -q "^ARG DEBIAN_SNAPSHOT=$SOS_IMAGE_DEBIAN_SNAPSHOT$" \
  "$test_repo_root/packaging/debian-live/Containerfile"
grep -q -- '--binary-images iso-hybrid' \
  "$test_repo_root/packaging/debian-live/build-live-iso"
grep -q -- '--bootloaders grub-efi' \
  "$test_repo_root/packaging/debian-live/build-live-iso"
grep -q -- '--apt-options "--yes -o Acquire::Check-Valid-Until=false -o Acquire::Retries=5 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30"' \
  "$test_repo_root/packaging/debian-live/build-live-iso"
grep -q -- '--apt-pipeline 0' \
  "$test_repo_root/packaging/debian-live/build-live-iso"
grep -q -- '--firmware-binary false' \
  "$test_repo_root/packaging/debian-live/build-live-iso"
grep -q -- '--firmware-chroot false' \
  "$test_repo_root/packaging/debian-live/build-live-iso"
grep -q -- '--updates false' \
  "$test_repo_root/packaging/debian-live/build-live-iso"
if grep -q 'cargo build\|npm run build\|install-linux-login-session' \
  "$test_repo_root/packaging/debian-live/build-live-iso"; then
  printf 'live ISO recipe invokes a source build or installer\n' >&2
  exit 1
fi
grep -q 'output directory is not empty' \
  "$test_repo_root/tools/build-linux-iso-container"

printf 'linux_packaging_host_test_passed architecture=%s snapshot=%s\n' \
  "$SOS_IMAGE_ARCH" "$SOS_IMAGE_DEBIAN_SNAPSHOT"
