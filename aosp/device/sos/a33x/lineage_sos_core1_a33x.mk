# Shipping/ordinary Core 1 product. Development credentials use the distinct
# lineage_sos_core1_dev_a33x product and can never be selected by this graph.
$(call inherit-product, device/sos/a33x/sos_core1_common.mk)

PRODUCT_NAME := lineage_sos_core1_a33x

PRODUCT_PACKAGES += \
    sos-agent-runner

PRODUCT_SYSTEM_EXT_PROPERTIES += \
    ro.sos.build_variant=core1-ordinary
