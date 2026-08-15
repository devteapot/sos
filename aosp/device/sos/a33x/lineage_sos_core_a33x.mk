# SOS Core bring-up target. The native SurfaceComposer probe is deliberately
# disabled: Android UI remains the recovery owner until the native GPUI host,
# input, trusted lockscreen, and recovery controls pass on physical hardware.
# Reaching Core 0 will remove the superseded packages from this target only.
$(call inherit-product, device/sos/a33x/sos_a33x_common.mk)

PRODUCT_NAME := lineage_sos_core_a33x

PRODUCT_PACKAGES += \
    sos-core-surface-probe

PRODUCT_SYSTEM_EXT_PROPERTIES += \
    ro.sos.profile=core \
    ro.sos.ui_owner=android-shadow
