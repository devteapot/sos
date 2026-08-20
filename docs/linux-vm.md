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

`provision-debian` refuses non-Debian-13 guests, installs the pinned GPUI/Zed
Linux development libraries plus Weston/Xvfb/Mesa and the direct
GBM/libinput/libseat/udev/seatd stack, installs Rust 1.95.0 with rustfmt and
Clippy through Debian's `rustup` package, fetches the locked dependency graph,
and links the four session binaries plus both compositor backends. Log out and
back in if it adds render/input group membership.

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

## Package-only pinned image

The source-provisioned VM above remains the development and diagnostic gate.
The first package-only image recipe is in
[`images/debian-13/README.md`](../images/debian-13/README.md). It builds five
native Debian packages outside the image, verifies the already-evidenced Debian
13.6 ARM64 generic image by filename, byte size, SHA-256, and SHA-512, resolves
runtime dependencies only against an immutable Debian snapshot, and installs
the packages into a fresh QCOW2 without copying the repository or a compiler.

This is currently an implemented, statically checked recipe rather than a
completed image gate. It must not replace the source-provisioned VM evidence
until a Debian 13 builder has produced the packages, the exact assembled image
has booted through `sos-session.target`, and the package lifecycle and
boot-session verifiers have passed against that artifact.
