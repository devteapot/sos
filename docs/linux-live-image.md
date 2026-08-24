# Fedora live image remix for the first Linux hardware loop

Date: 2026-08-23

This repository does not compose a Fedora live ISO from packages. A
lorax/livemedia-creator/kiwi bake needs a Fedora compose mirror, root, and a
long official image run. This path instead remixes one checksum-pinned official
Fedora Workstation live ISO by baking the same offline selectable-session
install the hardware gate already uses.

The remix is live-boot evidence media. It is not an installed product, not a
It runs without installing Fedora or SOS to the Framework Laptop's internal
disk. The removable media write happens on the build/operator machine.

## What the image contains

- Fedora Workstation live userspace with GDM left as the display manager
- the offline `install-linux-login-session` output under
  `/usr/local/libexec/sos`, `/usr/local/libexec/sos-agent`,
  `/usr/share/wayland-sessions/sos.desktop`, `/usr/share/sos`, and
  `/usr/share/doc/sos`
- runtime packages for those binaries and for prepare/collect, not `-devel`
  toolchains
- `/usr/local/libexec/sos/linux-hardware-gate`
- offline liveuser/skel agent state pointing at
  `/usr/share/sos/experiences/daily-flow.luau`
- `/usr/share/doc/sos/image-identity.env` plus mandatory ISO-level
  `/sos-image-identity.env` with matching live-boot labels and payload identity

The image does not enable `sos-session.target`, replace GDM, change the
volume label, or run device-code agent login at boot.

## Rebuild on a Fedora x86_64 host

Download a current official Fedora Workstation live x86_64 ISO from Fedora and
verify its signed CHECKSUM using Fedora's documented process. The build host
must be Fedora x86_64 at the same Fedora release as the ISO; this prevents
host-built SOS binaries from being paired with a different userspace release.
Install the Fedora build dependencies listed in
[`linux-hardware-gate.md`](linux-hardware-gate.md), plus `erofs-utils`,
`isomd5sum`, `policycoreutils`, and `xorriso`; `doctor` fails when any command
or native module required by the bake is missing.
From a clean checkout of the revision you intend to collect:

```sh
./tools/linux-live-image doctor
./tools/linux-live-image bake \
  --source-iso /path/to/Fedora-Workstation-Live-x86_64-*.iso \
  --source-sha256 "$FEDORA_ISO_SHA256" \
  --output-dir artifacts/linux-live-image
```

Bake fails before extraction when the supplied SHA-256 does not match. Fedora
44's official Workstation ISO calls its payload `LiveOS/squashfs.img`, but that
file is a flat EROFS root filesystem. The bake verifies EROFS and
`/etc/os-release`, extracts as root with owner/permission/xattr preservation,
mutates that tree, applies the image's SELinux file-context policy, and repacks
as root. It checks the rebuilt EROFS before inserting it into the ISO. It never
maps the rootfs to the desktop user.

Bake requires sudo for metadata-preserving rootfs extraction/repack, relabeling,
and `dnf --installroot`. It preserves the source ISO volume ID and El Torito/EFI boot;
changing the label breaks Fedora's `root=live:CDLABEL=...` cmdline. The output sidecar
`artifacts/linux-live-image/image-identity.env` records the source revision,
Fedora/build-host release, base ISO identity, EROFS payload hash, and
output ISO hash. The bake re-implants and verifies Fedora's embedded media
checksum after modifying the ISO, so the boot menu's media check covers the
remix rather than the unmodified source image.

Write the ISO to removable media with a tool that keeps the original hybrid
layout. Do not add persistence unless you have a reason; the first loop keeps
prepare and collect on one boot and copies evidence off before reboot.

## Boot, prepare, collect, copy off

1. Boot the remixed ISO on the Framework Laptop 12.
2. Stay in the conventional GNOME live session.
3. Prepare with the baked harness:

```sh
/usr/local/libexec/sos/linux-hardware-gate prepare \
  --expect-product 'Laptop 12' \
  --evidence-dir /home/liveuser/framework12-first-gate
```

4. Log out, select **SOS** in GDM, complete the same physical steps as the
   installed-workstation campaign, return to GNOME, and collect from the same
   directory:

```sh
/usr/local/libexec/sos/linux-hardware-gate collect \
  --evidence-dir /home/liveuser/framework12-first-gate
```

5. Copy the evidence directory off the live overlay (`scp` or a mounted USB)
   before reboot.

If the checkout of the baked revision is also present, prepare still requires
that clean worktree to match the image identity. Persistence is optional only
because prepare, the SOS login, and collect stay on one boot.

## Labels and limits

Prepare and collect record `boot_kind=live-boot`, the exact kernel `boot_id`,
and `not_installed_product=true`. Collect rejects a different boot ID. Prepare
also requires the ISO-level identity, requires it to agree with the rootfs
identity, and verifies the mounted `LiveOS/squashfs.img` byte size and SHA-256.
Hardware (DRM, input, DMI) is the same silicon as an installed Fedora
Workstation. Persistence, disk, and bootloader are not. Do not call this
campaign an installed product.

Stock Fedora live media without SOS image-identity is refused. A later disk
install of this remix is also refused: the identity remains live-boot but the
overlay is gone, so it is neither live-boot nor the installed-workstation
path.

This document does not change the PASS contract in
[`linux-hardware-gate.md`](linux-hardware-gate.md).
