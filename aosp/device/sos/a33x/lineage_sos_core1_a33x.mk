# SOS Core 1 validation target: no Zygote is started, so system_server and APK
# processes cannot exist. The native host owns display/input and exposes a
# fixed locked/recovery surface until synthetic-password unlock is native.
# The shared Samsung definition normally selects core_64_bit_only.mk. The
# audited source patch for this target substitutes core_no_zygote.mk there so
# the vendor property has one authoritative ro.zygote assignment.
$(call inherit-product, device/sos/a33x/sos_a33x_common.mk)
$(call inherit-product, device/sos/a33x/sos_native_host_common.mk)

PRODUCT_NAME := lineage_sos_core1_a33x

PRODUCT_PACKAGES += \
    sos-core-app-manifest \
    sos-core-platform-adapter \
    sos-ui-removal-marker

# Userdebug/eng acceptance uses a kernel uinput device, not Android's absent
# InputManager. Production user artifacts do not contain either executable.
PRODUCT_PACKAGES_DEBUG += \
    sos-core-input-automation \
    sos-core-inputctl

PRODUCT_SYSTEM_EXT_PROPERTIES += \
    ro.sos.block_android_activities=true \
    ro.sos.disable_user_apk_install=true \
    ro.sos.core.stage=1 \
    ro.sos.lifecycle=active \
    ro.sos.providers=core-native \
    ro.sos.profile=core \
    ro.sos.ui_owner=native-sos-no-zygote
