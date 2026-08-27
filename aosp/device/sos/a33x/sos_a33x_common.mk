# Shared SOS Samsung base. Every staged product uses the reproduced a33x
# hardware definition, on-device services, and immutable revision format.
# UI ownership belongs in the profile makefiles, not here.
$(call inherit-product, device/samsung/a33x/lineage_a33x.mk)

PRODUCT_SOONG_NAMESPACES += device/sos/a33x

PRODUCT_PACKAGES += \
    sos-android-system-authority \
    sos-node \
    sos-node-cxx-shared \
    sos-agent-runner \
    sos-agent-experience-api \
    sos-agent-example-primary \
    sos-mobile-experience \
    sos-mobile-package \
    sos-mobile-theme

SYSTEM_EXT_PUBLIC_SEPOLICY_DIRS += \
    device/sos/a33x/sepolicy/system_ext/public

SYSTEM_EXT_PRIVATE_SEPOLICY_DIRS += \
    device/sos/a33x/sepolicy/system_ext/private

PRODUCT_SYSTEM_EXT_PROPERTIES += \
    ro.sos.authority=on-device \
    ro.sos.experience_api=4 \
    ro.sos.revision_format=4 \
    ro.sos.legacy_revision_read=3
