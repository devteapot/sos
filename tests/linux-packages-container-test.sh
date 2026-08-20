#!/usr/bin/env bash

set -euo pipefail

package_test_snapshot="${SOS_DEBIAN_SNAPSHOT:?SOS_DEBIAN_SNAPSHOT is required}"
package_test_directory="${SOS_PACKAGE_DIRECTORY:-/packages}"
[[ "$package_test_snapshot" =~ ^[0-9]{8}T[0-9]{6}Z$ ]]
[[ -d "$package_test_directory" ]]

rm -f /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources
printf '%s\n' \
  "deb [check-valid-until=no] http://snapshot.debian.org/archive/debian/$package_test_snapshot trixie main contrib non-free-firmware" \
  "deb [check-valid-until=no] http://snapshot.debian.org/archive/debian-security/$package_test_snapshot trixie-security main contrib non-free-firmware" \
  >/etc/apt/sources.list
printf 'Acquire::Check-Valid-Until "false";\n' >/etc/apt/apt.conf.d/99sos-snapshot

apt-get update >/tmp/sos-package-apt-update.log
DEBIAN_FRONTEND=noninteractive apt-get install -y \
  "$package_test_directory"/*.deb >/tmp/sos-package-apt-install.log

package_test_version=""
for package_test_name in \
  sos-runtime \
  sos-agent \
  sos-desktop-session \
  sos-appliance-session \
  sos-image-config; do
  package_test_status="$(dpkg-query -W -f='${db:Status-Abbrev}' "$package_test_name")"
  package_test_actual_version="$(dpkg-query -W -f='${Version}' "$package_test_name")"
  package_test_architecture="$(dpkg-query -W -f='${Architecture}' "$package_test_name")"
  [[ "$package_test_status" == "ii " ]]
  if [[ -z "$package_test_version" ]]; then
    package_test_version="$package_test_actual_version"
  fi
  [[ "$package_test_actual_version" == "$package_test_version" ]]
  printf 'installed package=%s version=%s architecture=%s\n' \
    "$package_test_name" "$package_test_actual_version" "$package_test_architecture"
done

for package_test_binary in \
  sos-compositor \
  sos-experience-host \
  sos-provider-state-service \
  sos-revision-supervisor \
  sos-linux-session \
  sos-agent-authoring; do
  [[ -x "/usr/lib/sos/$package_test_binary" ]]
  if ldd "/usr/lib/sos/$package_test_binary" | grep -q 'not found'; then
    printf 'unresolved shared library: %s\n' "$package_test_binary" >&2
    exit 1
  fi
done

[[ "$(/usr/lib/sos-agent/node/bin/node --version)" == v24.18.0 ]]
[[ -x /usr/lib/sos/sos-login-session ]]
[[ -x /usr/lib/sos/sos-agent-login ]]
[[ -f /usr/share/wayland-sessions/sos.desktop ]]
if grep -R -q '/usr/local/libexec/sos\|/usr/local/bin/node' \
  /usr/lib/systemd/system/sos-*.service \
  /usr/lib/sos/sos-login-session \
  /usr/lib/sos/sos-agent-login \
  /usr/share/wayland-sessions/sos.desktop; then
  printf 'installed packages retain development installation paths\n' >&2
  exit 1
fi

systemd-analyze verify \
  /usr/lib/systemd/system/sos-session.service \
  /usr/lib/systemd/system/sos-session.target \
  /usr/lib/systemd/system/sos-agent-authoring.service \
  /usr/lib/systemd/system/sos-agent.service \
  /usr/lib/systemd/system/sos-agent.target \
  /usr/lib/systemd/system/sos-image-initialize.service

/usr/lib/sos/sos-image-initialize
package_test_revision="$(runuser -u sos-supervisor -- \
  /usr/lib/sos/sos-revision-supervisor status --root /var/lib/sos/revisions)"
[[ "$package_test_revision" =~ ^[0-9a-f]{64}$ ]]
[[ "$(wc -c </etc/sos/shell-token)" -eq 64 ]]
/usr/lib/sos/sos-image-initialize >/tmp/sos-image-initialize-second.log
[[ "$(runuser -u sos-supervisor -- \
  /usr/lib/sos/sos-revision-supervisor status --root /var/lib/sos/revisions)" \
  == "$package_test_revision" ]]

dpkg -S \
  /usr/lib/sos/sos-compositor \
  /usr/lib/sos-agent/node/bin/node \
  /usr/lib/systemd/system/sos-session.service \
  /usr/lib/systemd/system/sos-image-initialize.service \
  /usr/share/wayland-sessions/sos.desktop

printf 'linux_package_install_test_passed version=%s revision_id=%s node=%s\n' \
  "$package_test_version" \
  "$package_test_revision" \
  "$(/usr/lib/sos-agent/node/bin/node --version)"
