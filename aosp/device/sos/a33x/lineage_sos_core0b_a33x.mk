# SOS Core 0B: native SOS is the only rendered UI. Zygote and system_server are
# retained solely for headless framework services and the direct-boot
# credential bridge. User-facing Android packages are overridden out.
$(call inherit-product, device/sos/a33x/sos_a33x_common.mk)
$(call inherit-product, device/sos/a33x/sos_headless_android_adapter_common.mk)

PRODUCT_NAME := lineage_sos_core0b_a33x

PRODUCT_SYSTEM_EXT_PROPERTIES += \
    ro.sos.block_android_activities=true \
    ro.sos.disable_user_apk_install=true \
    ro.sos.core.stage=0b \
    ro.sos.profile=core \
    ro.sos.ui_owner=native-sos-headless-framework
