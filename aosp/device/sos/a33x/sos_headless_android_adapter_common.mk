# Shared headless Android-framework adapter used by native Compat and the
# frozen Core 0B migration oracle. It has no Activity and may never take
# presentation ownership.
$(call inherit-product, device/sos/a33x/sos_native_host_common.mk)

PRODUCT_PACKAGES += \
    SosFrameworkBridge
