#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ctl="$repo_root/tools/a33xctl"
source_root="${SOS_LINEAGE_ROOT:-$HOME/dev/lineage-a33x}"
test_root="$(mktemp -d /tmp/sos-a33xctl-patch-test.XXXXXX)"
lineage_root="$test_root/lineage-a33x"
trap 'rm -rf -- "$test_root"' EXIT

clone_sparse_project() {
  [[ "$#" -ge 2 ]] || return 2
  local relative_project="$1"
  shift
  local source_project="$source_root/$relative_project"
  local destination_project="$lineage_root/$relative_project"

  [[ -d "$source_project/.git" || -f "$source_project/.git" ]] || {
    printf 'expected Lineage source project missing: %s\n' "$relative_project" >&2
    return 1
  }
  mkdir -p "$(dirname "$destination_project")"
  git clone --quiet --shared --no-checkout "$source_project" "$destination_project"
  git -C "$destination_project" sparse-checkout set --no-cone "$@"
  git -C "$destination_project" checkout --quiet --detach HEAD
}

clone_sparse_project device/samsung/s5e8825-common \
  BoardConfigCommon.mk \
  common.mk \
  configs/init/init.s5e8825.rc \
  shims/libepicoperator/epicoperator.c
clone_sparse_project vendor/samsung/s5e8825-common Android.bp
clone_sparse_project frameworks/base \
  services/core/java/com/android/server/am/ActivityManagerService.java \
  services/core/java/com/android/server/am/AppErrors.java \
  services/core/java/com/android/server/pm/PackageInstallerService.java \
  services/core/java/com/android/server/wm/ActivityStarter.java \
  services/core/java/com/android/server/wm/WindowManagerService.java

SOS_LINEAGE_ROOT="$lineage_root" "$ctl" check-patch-series >/dev/null
SOS_LINEAGE_ROOT="$lineage_root" "$ctl" apply-patches >/dev/null
first_identity="$(
  for relative_project in \
    device/samsung/s5e8825-common \
    vendor/samsung/s5e8825-common \
    frameworks/base; do
    git -C "$lineage_root/$relative_project" diff --no-ext-diff --binary
  done | sha256sum
)"
SOS_LINEAGE_ROOT="$lineage_root" "$ctl" apply-patches >/dev/null
second_identity="$(
  for relative_project in \
    device/samsung/s5e8825-common \
    vendor/samsung/s5e8825-common \
    frameworks/base; do
    git -C "$lineage_root/$relative_project" diff --no-ext-diff --binary
  done | sha256sum
)"
[[ "$second_identity" == "$first_identity" ]] || {
  printf 'second patch bootstrap changed the complete ordered result\n' >&2
  exit 1
}
SOS_LINEAGE_ROOT="$lineage_root" "$ctl" check-patch-series >/dev/null

printf 'a33xctl_patch_series_test=PASS\n'
