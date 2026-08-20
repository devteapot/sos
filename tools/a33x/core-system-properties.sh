#!/usr/bin/env bash

# Validate the SYSTEM partition property data staged into target-files. Keep
# this helper sourceable so the host regression can exercise the exact
# inspector path without requiring a product build.
inspect_core_system_properties() (
  [[ "$#" -eq 3 ]] || \
    fail "internal usage: inspect_core_system_properties <target-files> <stage> <ordinary|dev-credential>"
  local target_files="$1" stage="$2" product_variant="$3"
  local property_source="$target_files/SYSTEM/build.prop"
  local inspection_root="" system_properties=""

  inspection_root="$(mktemp -d "${TMPDIR:-/tmp}/sos-a33x-system-properties.XXXXXX")" || \
    fail "SOS $stage $product_variant SYSTEM property inspection setup failed"
  trap 'rm -rf -- "$inspection_root"' EXIT

  [[ -s "$property_source" ]] || \
    fail "SOS $stage $product_variant SYSTEM property data is missing or empty in target-files"
  system_properties="$inspection_root/build.prop"
  cp -- "$property_source" "$system_properties" || \
    fail "SOS $stage $product_variant SYSTEM property data could not be staged for inspection"
  [[ -s "$system_properties" ]] || \
    fail "SOS $stage $product_variant staged SYSTEM property data is empty"

  grep -Fx 'ro.debuggable=0' "$system_properties" >/dev/null || \
    fail "SOS $stage $product_variant image weakened Lineage's intentional global debugging posture"
  if [[ "$product_variant" == dev-credential ]]; then
    grep -Fx 'ro.build.type=userdebug' "$system_properties" >/dev/null || \
      fail "SOS Core 1 development image is not the registered userdebug product"
  fi
)
