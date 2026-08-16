# SOS Core 0A: after Android completes the trusted CE unlock ceremony, the
# native GPUI shell, exclusive input path, and watchdog become UI owner.
# Android UI remains installed behind the native surface for explicit escape.
$(call inherit-product, device/sos/a33x/sos_a33x_common.mk)

PRODUCT_NAME := lineage_sos_core0a_a33x

PRODUCT_PACKAGES += \
    SosFrameworkBridge \
    sos-core-experience-runtime \
    sos-core-host

PRODUCT_SYSTEM_EXT_PROPERTIES += \
    ro.sos.disable_user_apk_install=true \
    ro.sos.core.autostart=postunlock \
    ro.sos.core.stage=0a \
    ro.sos.profile=core \
    ro.sos.ui_owner=native-sos
