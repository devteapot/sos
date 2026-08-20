# Pinned Debian 13 SOS reference image

This recipe creates a package-only development appliance image. It is a
reference image, not yet the SOS production distribution or an installer ISO.
The image assembler never copies a source tree, runs Cargo/npm, or invokes the
source installer inside the image.

## Inputs

- the exact official Debian 13 ARM64 `generic` QCOW2 named and hashed in
  `base-images.lock`;
- one same-version build of each SOS package produced by
  `tools/build-linux-packages`;
- the immutable Debian archive timestamp in `base-images.lock`;
- this recipe revision.

The first lock deliberately contains only the Debian 13.6 ARM64 artifact that
has existing VM evidence. Package building also supports Debian `amd64`, but an
AMD64 image must not be claimed as pinned until its official filename, size,
SHA-256, and SHA-512 are added and independently checked.

## Build packages

The preferred host entry point uses Podman or Docker:

```sh
./tools/build-linux-packages-container 0.1.0
```

It builds the environment from the digest-pinned Debian 13.6 slim image,
switches APT to the same immutable Debian snapshot as the image recipe,
installs Rust 1.95.0 and Node 24.18.0, bind-mounts the repository, and invokes
the native package builder as the host UID. Cargo targets and package-manager
caches stay below ignored `.cache/linux-package-builder`; Linux `node_modules`
are built in a temporary staging tree rather than the host source directory.

Set `SOS_CONTAINER_ENGINE=podman` or `SOS_CONTAINER_ENGINE=docker` to select an
engine explicitly. `SOS_LINUX_PACKAGE_PLATFORM=linux/arm64` or
`linux/amd64` selects the package architecture when the engine supports that
platform; cross-architecture builds may require emulation and are not a
substitute for the architecture's boot gate.

The wrapper defaults to two Cargo jobs because release LTO exceeded the
12 GiB Podman VM when Cargo used unrestricted parallelism. Override this only
when the builder has measured headroom:

```sh
SOS_CARGO_BUILD_JOBS=1 \
  ./tools/build-linux-packages-container 0.1.0
```

The lower-level command remains available when already running inside a clean
Debian 13 builder:

```sh
./tools/build-linux-packages 0.1.0
```

The command creates five `.deb` files and `SHA256SUMS` under the ignored
`artifacts/linux-packages` directory:

- `sos-runtime`;
- `sos-agent`, including Node 24.18.0 as a private runtime;
- `sos-desktop-session`;
- `sos-appliance-session`;
- `sos-image-config`.

The native package builder rejects non-Debian-13 environments, derives native
shared-library dependencies with `dpkg-shlibdeps`, verifies the Node archive
digest, and normalizes package timestamps with `SOURCE_DATE_EPOCH`.

Verify the package dependency closure, installed paths, systemd units, private
Node runtime, package ownership, and idempotent first-boot initialization in a
fresh pinned Debian container:

```sh
SOS_CONTAINER_ENGINE=podman ./tools/test-linux-packages-container
```

## Assemble a live ISO

For a VM-visible boot gate, build an ARM64 Debian Live hybrid ISO from the same
five packages:

```sh
SOS_CONTAINER_ENGINE=podman ./tools/build-linux-iso-container
```

The live builder runs privileged inside the Podman VM because Debian
`live-build` uses chroots and mounts. Its build tree is an ephemeral Podman
volume; only the ISO, El Torito/filesystem inspection report, and deterministic
input/output manifest are copied to ignored `artifacts/linux-iso`. The ISO uses
GRUB EFI, boots the live system into `sos-session.target`, carries no source
tree or compiler, and generates its machine identity, shell token, and initial
experience revision at boot.

## Assemble the image

On a Linux host with QEMU tools and libguestfs:

```sh
./images/debian-13/build \
  /path/to/debian-13-generic-arm64.qcow2 \
  artifacts/linux-packages
```

The builder verifies both base-image digests and its byte size before writing
anything. `virt-customize` switches APT to the immutable Debian snapshot,
installs only the five local SOS packages plus dependencies from that snapshot,
selects `sos-session.target`, removes package caches and device identity, and
closes the image. The adjacent `.manifest` records the recipe revision, package
hashes, base identity, output byte size, and output SHA-256.

The first boot generates a unique shell token and machine identity, creates the
service-owned state directories, and installs the stock immutable experience
revision before the compositor session starts. Authentication credentials and
SSH host keys are never cloned into the image.

## Current confidence boundary

The recipe is functionally pinned: its base, archive timestamp, SOS package
payloads, and policy are immutable inputs. Byte-for-byte image reproduction is
not yet claimed because QCOW2/libguestfs allocation and filesystem metadata
have not been normalized and compared across two independent builders.

The ARM64 package build, clean-container install, first-boot initializer, and
same-builder byte reproducibility checks have passed. The next gate is to
assemble the image, boot the exact artifact in QEMU, and adapt the boot-session
verifier to consume the installed packages without a guest source tree.
