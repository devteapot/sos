PRODUCT_MAKEFILES := \
    $(LOCAL_DIR)/lineage_sos_compat0_a33x.mk \
    $(LOCAL_DIR)/lineage_sos_compat_a33x.mk \
    $(LOCAL_DIR)/lineage_sos_core0b_a33x.mk \
    $(LOCAL_DIR)/lineage_sos_core1_a33x.mk \
    $(LOCAL_DIR)/lineage_sos_core1_dev_a33x.mk \
    $(LOCAL_DIR)/lineage_sos_core_a33x.mk

# The credential-bearing product is deliberately exposed only as a
# debuggable lunch choice. Its product makefile rejects a user build too.
COMMON_LUNCH_CHOICES += \
    lineage_sos_core1_dev_a33x-userdebug
