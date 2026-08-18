# Shared fixed native shell boundary for every pre-unlock SOS product.
# Profile makefiles explicitly select runtime adapters and package policy.
PRODUCT_PACKAGES += \
    sos-core-experience-runtime \
    sos-core-host

PRODUCT_SYSTEM_EXT_PROPERTIES += \
    ro.sos.core.autostart=preunlock
