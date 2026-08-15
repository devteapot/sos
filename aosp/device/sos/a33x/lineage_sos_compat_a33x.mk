# SOS Compat retains Android's framework, SystemUI, Recents, application
# runtime, and compatibility ceremonies. SOS owns HOME and its durable
# experience/revision services.
$(call inherit-product, device/sos/a33x/sos_a33x_common.mk)

PRODUCT_NAME := lineage_sos_compat_a33x

PRODUCT_PACKAGES += \
    SosA33xFrameworkOverlay \
    SosShell

PRODUCT_SYSTEM_EXT_PROPERTIES += \
    ro.sos.home=dev.sos.experience \
    ro.sos.profile=compat \
    ro.sos.ui_owner=android-compat
