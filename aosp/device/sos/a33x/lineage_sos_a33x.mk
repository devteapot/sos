# SOS Samsung product: inherit the reproduced SM-A336B LineageOS target, then
# add the ARM64 permanent HOME and its on-device authority. Keep the inherited
# Launcher3QuickStep package because SystemUI delegates Recents to it.
$(call inherit-product, device/samsung/a33x/lineage_a33x.mk)

PRODUCT_NAME := lineage_sos_a33x

PRODUCT_SOONG_NAMESPACES += device/sos/a33x

PRODUCT_PACKAGES += \
    SosA33xFrameworkOverlay \
    SosShell \
    sos-android-system-authority \
    sos-default-experience

SYSTEM_EXT_PRIVATE_SEPOLICY_DIRS += \
    device/sos/a33x/sepolicy/system_ext/private

PRODUCT_SYSTEM_EXT_PROPERTIES += \
    ro.sos.authority=on-device \
    ro.sos.home=dev.sos.experience
