# Shared fixed native shell boundary for every pre-unlock SOS product.
# Profile makefiles select only the runtime adapters and policy differences.
PRODUCT_PACKAGES += \
    sos-core-experience-runtime \
    sos-core-host \
    sos-ui-removal-marker

PRODUCT_SYSTEM_EXT_PROPERTIES += \
    ro.sos.core.autostart=preunlock
