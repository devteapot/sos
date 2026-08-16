# SOS Compat 1 retains Android's runtime and compatibility ceremonies while
# SOS owns HOME, navigation/exit chrome, and the attention surface.
$(call inherit-product, device/sos/a33x/sos_a33x_common.mk)

PRODUCT_NAME := lineage_sos_compat_a33x

PRODUCT_PACKAGES += \
    SosA33xFrameworkOverlay \
    SosCompat1FrameworkOverlay \
    SosCompat1SystemUiOverlay \
    SosShell \
    sos-compat-privapp-permissions \
    sos-compat-launcher-removal-marker

PRODUCT_SYSTEM_EXT_PROPERTIES += \
    ro.sos.home=dev.sos.experience \
    ro.sos.profile=compat \
    ro.sos.compat.stage=1 \
    ro.sos.ui_owner=sos-compat
