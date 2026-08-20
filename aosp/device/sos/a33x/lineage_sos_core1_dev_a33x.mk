# Non-shipping Core 1 development product. Product make sees this immutable
# selection directly; no caller-provided shell variable controls packaging.
ifneq ($(TARGET_BUILD_VARIANT),userdebug)
$(error SOS Core development credentials require the registered userdebug build)
endif

$(call inherit-product, device/sos/a33x/sos_core1_common.mk)

PRODUCT_NAME := lineage_sos_core1_dev_a33x

PRODUCT_PACKAGES += \
    sos-core-dev-credential \
    sos-node-core-dev \
    sos-agent-runner-core-dev

SYSTEM_EXT_PRIVATE_SEPOLICY_DIRS += \
    device/sos/a33x/sepolicy/core_dev_private

PRODUCT_SYSTEM_EXT_PROPERTIES += \
    ro.sos.build_variant=core1-dev-credential \
    ro.sos.dev_credential=1
