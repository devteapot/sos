# Frozen legacy migration oracle. Core 0B is not an active product target.
# It retains Zygote/system_server and the headless bridge only for controlled
# comparisons while their remaining services move into active Core 1.
$(call inherit-product, device/sos/a33x/sos_a33x_common.mk)
$(call inherit-product, device/sos/a33x/sos_headless_android_adapter_common.mk)

PRODUCT_NAME := lineage_sos_core0b_a33x

PRODUCT_SYSTEM_EXT_PROPERTIES += \
    ro.sos.block_android_activities=true \
    ro.sos.disable_user_apk_install=true \
    ro.sos.core.stage=0b \
    ro.sos.lifecycle=legacy \
    ro.sos.profile=core \
    ro.sos.ui_owner=native-sos-headless-framework
