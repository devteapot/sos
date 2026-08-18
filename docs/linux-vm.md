# Debian 13 Wayland VM gate

Date: 2026-08-09

This is the reproducible reference environment for the Linux client-host gate.
It is development infrastructure, not the SOS distribution contract. A result
counts as the VM gate only when the verifier reports `os=debian version=13`;
running the same nested test on another Linux host remains useful regression
evidence but does not complete this milestone.

## Reference machine

The harness creates a direct QEMU/KVM guest with:

- the host-matching architecture (`arm64` on AArch64, `amd64` on x86-64);
- 8 virtual CPUs, 12 GiB RAM, and a 100 GiB copy-on-write disk;
- UEFI, KVM host CPU, VirtIO disk/network/video, and a loopback-only VNC
  console;
- Debian 13's official `generic` qcow2 image, verified by an explicitly supplied
  SHA-512 before any overlay or domain is created;
- GNOME/GDM, SSH, and cloud-init user `sos`;
- a direct, unprivileged QEMU process with user-network SSH forwarding on
  loopback port 2222. QEMU has no host bridge or privileged network device.

Debian recommends the `generic` image rather than `genericcloud` for maximum
driver compatibility outside the cloud-provider device models. Obtain both the
image and its SHA-512 from the official Debian Cloud image directory; raw image,
overlay, and generated cloud-init files remain under `.cache/` or outside the
repository.

The host needs the architecture-matching QEMU system emulator, `qemu-img`,
`cloud-localds`, UEFI firmware, and read/write KVM access. On a typical
Debian/Ubuntu host the relevant packages are QEMU, `qemu-utils`,
`cloud-image-utils`, AAVMF or OVMF, and optionally `virt-viewer`. Fedora splits
the required VirtIO GPU and its PCI wrapper from the core emulator; install the
complete host-side set with:

```sh
sudo dnf install \
  qemu-system-x86-core qemu-img cloud-utils-cloud-localds edk2-ovmf \
  qemu-device-display-virtio-gpu qemu-device-display-virtio-gpu-pci \
  virt-viewer
```

Then create the guest:

```sh
./tools/linux-vm/create \
  /path/to/debian-13-generic-arm64.qcow2 \
  <sha512-from-official-SHA512SUMS>
```

The script refuses a mismatched architecture, filename, digest, existing
output, inaccessible KVM device, or missing firmware. It never modifies the
verified base image. Open the display with:

```sh
remote-viewer vnc://127.0.0.1:5901
```

The cloud user is intentionally SSH-key-only. Set a local console password over
SSH if interactive GDM login is needed:

```sh
ssh -p 2222 sos@127.0.0.1 'sudo passwd sos'
```

Shut the guest down over SSH when possible. `tools/linux-vm/stop` sends SIGTERM
to the exact recorded QEMU PID and preserves its disk and logs; it deliberately
refuses to escalate to a forced kill. Resume the preserved overlay with
`tools/linux-vm/start`.

## Put the current worktree in the guest

The Linux branch may contain unpushed work, so copy the working tree rather than
assuming a remote branch exists:

```sh
rsync -a \
  --exclude .cache --exclude .git --exclude artifacts --exclude target \
  -e 'ssh -p 2222' ./ sos@127.0.0.1:~/sos/
ssh -p 2222 sos@127.0.0.1 '~/sos/tools/linux-vm/provision-debian'
```

Acceptance runners can capture each host command without a handwritten command
description:

```sh
./tools/evidence-run --root "$evidence_root" --name phase-e-verify-boot-session -- \
  env SOS_LINUX_VM_ROOT="$vm_root" SOS_LINUX_VM_GUEST_ROOT=/home/sos/sos \
  ./tools/linux-vm/verify-boot-session
```

The evidence runner refuses to overwrite a record, rejects likely secret-bearing
arguments, preserves the command's exit status, and atomically renames each
member of one matched `.raw`/`.meta` pair. Metadata contains the literal
shell-escaped argv, each
individual argument, working directory, UTC and monotonic boundaries, elapsed
nanoseconds, and status. Put multiline remote bodies in a tracked script or a
separately identified input file; do not replace literal argv with descriptions
such as `ssh ...`, and never put credentials in argv. Finalize all such pairs
before generating the sorted, self-excluding manifest.

Generate the manifest with the existing campaign generator, then audit it with
the standalone read-only verifier. The verifier takes both paths explicitly,
validates the three-column TSV schema, C-byte order and uniqueness, safe
relative paths, the exact self-excluding finalized file set, sizes and SHA-256
values, and byte-identical deterministic regeneration. It is an independent
Python implementation and is invoked as an executable, so external acceptance
does not depend on a multiline `python -c` argument:

```sh
./tools/a33xctl evidence-manifest-generate \
  --root "$evidence_root" --output "$evidence_root/manifest.tsv"
./tools/evidence-manifest-verify \
  --root "$evidence_root" --manifest "$evidence_root/manifest.tsv"
```

### Phase F privilege and private-runtime matrix

Phase F keeps evidence privilege explicit. The campaign must first pass
`sudo -n true`; no command may prompt. Metadata inventory records paths,
types, sizes, modes, numeric owners and, where needed, device/inode identity,
but never credential or private-runtime file contents.

| Context | Operations |
| --- | --- |
| Login user, no sudo | Credential/config `find` and `stat`; both login helpers; installer top level; the GDM SOS session; same-UID process identity; `tools/linux-vm/inventory-sos-runtime --root /run/user/$(id -u)`. |
| Login user, readable system interfaces | Installed-payload `stat`/`sha256sum` and absence checks; `systemctl is-active`, `get-default`, and `show`; `loginctl show-seat`/`show-session`; `ps`, same-UID `pgrep`, `id`, and `getent`. |
| Explicit `sudo -n`, read-only | Bounded AccountsService metadata/identity; `/etc/gdm3/daemon.conf` metadata, hash, and exact `DefaultSession`; bounded system journal records; cross-UID `/proc` executable identity; unreadable GDM-greeter runtime metadata. |
| Explicit `sudo -n`, mutation | Root-owned installation cleanup or restoration and `systemctl set-default`, `enable`, `disable`, `start`, or `stop`. Logout remains selectable-session `Ctrl+Alt+Backspace`, never privileged session termination. |

The runtime helper scans only top-level names matching the product-created
`sos-session.XXXXXX` form and descends only those exact matches. It neither
walks nor reports unrelated `/run/user/<uid>` trees such as
`systemd/inaccessible`, and it rejects root execution so the ownership/access
boundary remains under test. A Phase F capture is therefore:

```sh
ssh -p 2222 sos@127.0.0.1 \
  'cd /home/sos/sos && tools/linux-vm/inventory-sos-runtime --root /run/user/$(id -u)'
```

`provision-debian` refuses non-Debian-13 guests, installs the pinned GPUI/Zed
Linux development libraries plus GStreamer test sources/PNG encoding,
Weston/Xvfb/XWayland/X11 utilities/Mesa, and the direct
GBM/libinput/libseat/udev/seatd stack. It also installs Rust 1.95.0 with
rustfmt and Clippy through Debian's `rustup` package, fetches the locked
dependency graph, and links the four session binaries plus both compositor
backends. Log out and back in if it adds render/input group membership.

## Automated acceptance gate

Run the disposable nested session inside the guest:

```sh
ssh -p 2222 sos@127.0.0.1 '~/sos/tools/linux-vm/verify-session'
```

The verifier creates an isolated Xvfb/Weston seat and revision store, then
proves:

1. the provider/state authority starts and binds the immutable boot revision;
2. the coordinated supervisor boots the real GPUI/Wayland host;
3. a second revision is installed and durably staged;
4. the coordinator commits authority state before presenting the scene;
5. the permanent host PID is unchanged;
6. the authority revision ID and supervisor `current` pointer are identical;
7. supervisor and authority shut down and the read-only disposable store is
   removed.

Expected leading output:

```text
linux_nested_session_passed os=debian version=13 host_pid=... revision_id=...
```

After that automated gate, log into the GNOME Wayland session and run
`./tools/sosctl linux-run --windowed` directly. This checks the normal virtual
GPU/input path. Software rendering is acceptable for functional evidence, but
no VM run establishes hardware latency, thermals, physical touch, suspend, or
vendor DRM/KMS behavior.

Then run the compositor gate in the same guest:

```sh
ssh -p 2222 sos@127.0.0.1 '~/sos/tools/linux-compositor/verify-nested'
```

It adds a Smithay compositor between the outer Weston session and GPUI,
activates a revision without replacing the host, forces and recovers one host
crash without replacing the compositor, and maps a separate compatibility
client. Its `evidence=nested_backend_submit` is compositor-owned submission to
the outer session, not proof of a physical KMS page flip. The full contract is
in [`linux-compositor.md`](linux-compositor.md).

Finally, run the direct session gate from SSH. It deliberately stops and later
restores GDM inside the disposable VM:

```sh
ssh -p 2222 sos@127.0.0.1 '~/sos/tools/linux-vm/verify-direct-session'
```

The verifier refuses bare metal, acquires the VirtIO KMS device and `seat0`
through libseat/seatd, proves the recovery view before shell startup, and binds
boot, activation, and recovered boot to DRM VBlank evidence. Keep the VNC
console available while developing this path.

After the direct gate, run the boot-session verifier from the host worktree:

```sh
./tools/linux-vm/verify-boot-session
```

It synchronizes the current worktree into the already-provisioned `~/sos`
guest directory, builds and installs disposable system-session binaries, seeds
one immutable revision, disables seatd, selects `sos-session.target`, and
reboots the VM. Set `SOS_LINUX_VM_GUEST_ROOT` when the guest worktree uses a
different absolute path. SSH remains only the test controller; it does not
launch or own the compositor session.

Before installation, the verifier prints the absolute guest source root and
SHA-256 identities for `tools/linux-vm/provision-debian` and the actual agent
manifest at `services/sos-agent/package.json`. It runs the locked agent package
sequence `npm ci --ignore-scripts`, `npm run check`, `npm test`, and a final
`npm run build`, then reports the built
`services/sos-agent/dist/agent-runner.cjs` path, byte size, and SHA-256. The
provisioner runs and reports the same sequence, so an acceptance capture need
not infer test or bundle completion from a later consumer.

The resident-agent subgate waits on accessibility snapshot generations for at
most the authoring broker's 30-second operation timeout. It passes only when
one semantic snapshot contains the exact user request, the exact packaged faux
provider completion, `Ready`, and an empty editable composer after the new
revision is presented. It also requires the same completion in the persisted
agent history. A successful run prints the complete initial/final semantic JSON,
complete before/after daemon-status JSON, an exact request/completion object
derived from the persisted JSON plus its path/size/SHA-256, and safe PID/PPID/
UID/user/executable identities for the session owner, compositor, platform
authority, supervisor, host proxy, experience host, authoring broker, and
resident agent. It never prints credential values. Failure prints the same
semantic/status/history diagnostics plus the bounded journals. The timeout is a
bound on an explicit completion predicate, not a sleep used as evidence.

The verifier requires a clean reference guest with no existing `/var/lib/sos`,
`/etc/sos`, `/usr/local/libexec/sos`, or SOS unit files. It proves active
logind seat0/tty1 ownership, a recovery-view page flip before provider startup,
credential-file delivery without the secret in process arguments or
environment values, same-PID activation, host recovery, authority/pointer
agreement, and a full systemd restart after provider failure. It then restores
`graphical.target`, re-enables seatd, reboots into GNOME, and removes only the
installation it created. Its leading result is:

```text
linux_boot_session_passed ... evidence=drm_page_flip
```

The suspend/output lifecycle subgate emits a pass checkpoint for each VT
request and log match, freezer command and kernel entry/exit match, connector
request and disconnect/reconnect match, and same-PID liveness assertion. A
readable `/sys/power/mem_sleep` is captured before the freezer test; its one
selected `s2idle` or `deep` mode must match a new kernel entry followed by a
kernel suspend exit. If that sysfs selector is unavailable, the same supported
mode and ordered entry/exit pair must be unambiguous in the new kernel journal.
A successful lifecycle run also prints the terminal VT, both virtual-connector
states, `pm_test`, available and selected `mem_sleep` mode, unchanged owner PID,
and the exact direct-session pause/activation, KMS initialization/disconnect, and
significant page-flip journal records. The boot contract prints the exact
logind-session fields and process identities before destructive recovery tests.
A normal uninstall passes only after all installed SOS paths, units, service
accounts, the IPC group and login membership, and matching processes are
absent; both normal and failure cleanup print the same
`linux_boot_cleanup_audit status=passed|failed ...` category contract, and the
normal path retains `linux_boot_cleanup_passed` as its terminal marker. A
failure emits `linux_boot_lifecycle_failed phase=...` with the assertion line,
current session PID, VT/connector/`pm_test`/`mem_sleep` state, relevant
compositor journal, and kernel PM journal before the verifier restores the
disposable guest. A nonzero SSH result without that marker is classified as
lifecycle
transport/bootstrap failure rather than as one of the product assertions.

## Current status

The gate passes in a KVM-accelerated ARM64 Debian 13.6 guest on kernel
`6.12.100+deb13-arm64`. The guest built the real GPUI host and reported:

```text
linux_nested_session_passed os=debian version=13 host_pid=3874 revision_id=552f06968bbc5c69de3db581454f60d4303289f304eaaf47a6e9dc3200297cdb
```

The immutable input was `debian-13-generic-arm64.qcow2`, 428,736,512 bytes,
SHA-256 `0e68f071dec0215f5d8c7e6f51898213951a6c1a4859f1b980fb4d479255e2bc`,
and official SHA-512
`e8ed94e83edded072c66b8871beff8243e0b846ac53980847e2ae44c6d47a8a55579181390b6c85939e85e2a821014ae87e9684930c0509a045212753c8d7916`.
The mutable overlay is retained under ignored `.cache/linux-vm/` for continued
development but is not an evidence artifact. The nested Smithay gate now also
passes in this guest: revision `552f0696…` activated in unchanged PID 11310,
then the killed host recovered the same committed revision in PID 11514; boot,
activation, and recovery each produced compositor-owned nested-backend submit
evidence, and the compatibility client mapped at `(280, 140)`. This result does
not change any physical-hardware or latency claim. The direct gate now passes
as well: revision `552f0696…` activated in unchanged PID 59723 and recovered in
PID 59849, with three `drm_page_flip` fences, a compositor recovery frame, and
monotonic KMS event timestamps.

The system-owned boot gate now passes too. With seatd disabled, logind assigned
seat0/tty1 to active Wayland session 1. Revision `552f0696…` activated without
changing host PID 883, recovered after `SIGKILL` in PID 1089, then survived a
provider-triggered systemd restart in lifecycle PID 1222 and host PID 1312.
All four boot/activation/recovery boundaries used `drm_page_flip`; the service
restart counter reached one, and the verifier returned the guest to GNOME and
removed its disposable installation. Deterministic injected input is the next
VM gate.

References: [Debian Cloud images](https://wiki.debian.org/Cloud),
[Debian cloud image comparison](https://wiki.debian.org/Cloud/SystemsComparison),
and the [QEMU invocation manual](https://www.qemu.org/docs/master/system/invocation.html).
