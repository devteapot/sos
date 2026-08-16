# SOS Compat 0: SOS is the enforced HOME while Android retains its UI
# ceremonies, navigation, notification shade, and application runtime.
$(call inherit-product, device/sos/a33x/sos_a33x_common.mk)

PRODUCT_NAME := lineage_sos_compat0_a33x

PRODUCT_PACKAGES += \
    SosA33xFrameworkOverlay \
    SosShell \
    sos-compat-privapp-permissions

PRODUCT_SYSTEM_EXT_PROPERTIES += \
    ro.sos.home=dev.sos.experience \
    ro.sos.profile=compat \
    ro.sos.compat.stage=0 \
    ro.sos.ui_owner=android-ceremonies
