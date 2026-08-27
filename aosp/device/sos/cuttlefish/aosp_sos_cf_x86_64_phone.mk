# SOS Cuttlefish product: the stock x86-64-only phone plus an on-device
# authority and SOS as the default HOME. Keep Quickstep installed because
# Android's SystemUI still delegates the Recents implementation to it.
$(call inherit-product, device/google/cuttlefish/vsoc_x86_64_only/phone/aosp_cf.mk)

PRODUCT_NAME := aosp_sos_cf_x86_64_phone
PRODUCT_DEVICE := vsoc_x86_64_only
PRODUCT_BRAND := Android
PRODUCT_MANUFACTURER := SOS
PRODUCT_MODEL := SOS Cuttlefish x86_64 phone

PRODUCT_SOONG_NAMESPACES += device/sos/cuttlefish

PRODUCT_PACKAGES += \
    SosShell \
    SosFrameworkOverlay \
    sos-android-system-authority \
    sos-mobile-experience \
    sos-mobile-package \
    sos-mobile-theme

SYSTEM_EXT_PRIVATE_SEPOLICY_DIRS += \
    device/sos/cuttlefish/sepolicy/system_ext/private

PRODUCT_SYSTEM_EXT_PROPERTIES += \
    ro.sos.authority=on-device \
    ro.sos.home=dev.sos.experience \
    ro.sos.experience_api=4 \
    ro.sos.revision_format=4 \
    ro.sos.legacy_revision_read=3
