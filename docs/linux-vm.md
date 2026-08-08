# Debian 13 Wayland VM gate

Date: 2026-08-08

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
`cloud-image-utils`, AAVMF or OVMF, and optionally `virt-viewer`. Then create
the guest:

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
Linux development libraries plus Weston/Xvfb/Mesa, installs Rust 1.95.0 through
Debian's `rustup` package, fetches the locked dependency graph, and links the
four session binaries. Log out and back in if it adds render/input group
membership.

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
development but is not an evidence artifact. The next gate is the minimal
nested Smithay compositor; this result does not change any physical-hardware or
latency claim.

References: [Debian Cloud images](https://wiki.debian.org/Cloud),
[Debian cloud image comparison](https://wiki.debian.org/Cloud/SystemsComparison),
and the [QEMU invocation manual](https://www.qemu.org/docs/master/system/invocation.html).
