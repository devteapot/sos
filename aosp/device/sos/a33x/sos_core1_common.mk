# Shared Core 1 composition: no Zygote is started, so system_server and APK
# processes cannot exist. The native host owns display/input and exposes a
# fixed locked/recovery surface until synthetic-password unlock is native.
# The shared Samsung definition normally selects core_64_bit_only.mk. The
# audited source patch substitutes core_no_zygote.mk for both Core 1 products.
# Lineage common.mk deliberately sets PRODUCT_NOT_DEBUGGABLE_IN_USERDEBUG, so
# both Core products retain ro.debuggable=0 and do not enable broad adb root.
$(call inherit-product, device/sos/a33x/sos_a33x_common.mk)
$(call inherit-product, device/sos/a33x/sos_native_host_common.mk)

PRODUCT_PACKAGES += \
    sos-core-app-manifest \
    sos-core-platform-adapter \
    sos-ui-removal-marker

PRODUCT_SYSTEM_EXT_PROPERTIES += \
    ro.sos.block_android_activities=true \
    ro.sos.disable_user_apk_install=true \
    ro.sos.core.stage=1 \
    ro.sos.lifecycle=active \
    ro.sos.providers=core-native \
    ro.sos.profile=core \
    ro.sos.ui_owner=native-sos-no-zygote
