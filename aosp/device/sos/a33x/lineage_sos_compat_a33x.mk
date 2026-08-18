# SOS Compat is a native SOS system with an Android application runtime.
# Android still supplies Zygote, system_server, PackageManager, WindowManager,
# and application processes, but its only visible system service is the IME
# requested by SOS-owned text editors.
# The fixed native host owns pre-unlock; the GPUI HOME and trusted Compat
# controls own the unlocked display around explicitly selected Android apps.
$(call inherit-product, device/sos/a33x/sos_a33x_common.mk)
$(call inherit-product, device/sos/a33x/sos_headless_android_adapter_common.mk)

PRODUCT_NAME := lineage_sos_compat_a33x

PRODUCT_PACKAGES += \
    SosA33xFrameworkOverlay \
    SosCompat1FrameworkOverlay \
    SosShell \
    sos-compat-privapp-permissions \
    sos-compat-ui-removal-marker

PRODUCT_SYSTEM_EXT_PROPERTIES += \
    ro.sos.home=dev.sos.experience \
    ro.sos.profile=compat \
    ro.sos.compat.stage=1 \
    ro.sos.core.stage=compat \
    ro.sos.block_android_system_activities=true \
    ro.sos.ui_owner=native-sos-android-runtime
