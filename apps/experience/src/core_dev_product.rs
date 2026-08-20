#[derive(Clone, Copy)]
pub(crate) struct DevProductMarkers<'a> {
    pub(crate) revision: &'a str,
    pub(crate) build_variant: &'a str,
    pub(crate) dev_credential: &'a str,
    pub(crate) build_type: &'a str,
    pub(crate) debuggable: &'a str,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ProductMarkerFailure<'a> {
    pub(crate) name: &'static str,
    pub(crate) expected: &'static str,
    pub(crate) actual: &'a str,
}

fn core_dev_revision(revision: &str) -> bool {
    let mut fields = revision.split('.');
    matches!(fields.next(), Some("sos"))
        && matches!(fields.next(), Some("core1dev"))
        && matches!(fields.next(), Some(hash) if lower_hex_digest(hash))
        && matches!(fields.next(), Some(hash) if lower_hex_digest(hash))
        && fields.next().is_none()
}

fn lower_hex_digest(value: &str) -> bool {
    value.len() == 12
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

pub(crate) fn validate_dev_product(
    markers: DevProductMarkers<'_>,
) -> Result<(), ProductMarkerFailure<'_>> {
    if !core_dev_revision(markers.revision) {
        return Err(ProductMarkerFailure {
            name: "ro.build.version.incremental",
            expected: "sos.core1dev.<12-lower-hex>.<12-lower-hex>",
            actual: markers.revision,
        });
    }
    for (name, expected, actual) in [
        (
            "ro.sos.build_variant",
            "core1-dev-credential",
            markers.build_variant,
        ),
        ("ro.sos.dev_credential", "1", markers.dev_credential),
    ] {
        if actual != expected {
            return Err(ProductMarkerFailure {
                name,
                expected,
                actual,
            });
        }
    }
    if markers.build_type != "userdebug" {
        return Err(ProductMarkerFailure {
            name: "ro.build.type",
            expected: "userdebug",
            actual: markers.build_type,
        });
    }
    // Lineage intentionally keeps userdebug Core globally non-debuggable. This
    // is a hardening assertion, never the switch that enables this endpoint.
    if markers.debuggable != "0" {
        return Err(ProductMarkerFailure {
            name: "ro.debuggable",
            expected: "0",
            actual: markers.debuggable,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_observed_hardened_core_dev_product_is_accepted() {
        assert!(validate_dev_product(DevProductMarkers {
            revision: "sos.core1dev.0123456789ab.cdef01234567",
            build_variant: "core1-dev-credential",
            dev_credential: "1",
            build_type: "userdebug",
            debuggable: "0",
        })
        .is_ok());
    }

    #[test]
    fn wrong_revision_build_type_marker_or_debug_posture_is_rejected() {
        let valid = DevProductMarkers {
            revision: "sos.core1dev.0123456789ab.cdef01234567",
            build_variant: "core1-dev-credential",
            dev_credential: "1",
            build_type: "userdebug",
            debuggable: "0",
        };
        for (markers, name) in [
            (
                DevProductMarkers {
                    revision: "sos.core1.0123456789ab.cdef01234567",
                    ..valid
                },
                "ro.build.version.incremental",
            ),
            (
                DevProductMarkers {
                    build_variant: "core1-ordinary",
                    ..valid
                },
                "ro.sos.build_variant",
            ),
            (
                DevProductMarkers {
                    dev_credential: "0",
                    ..valid
                },
                "ro.sos.dev_credential",
            ),
            (
                DevProductMarkers {
                    build_type: "user",
                    ..valid
                },
                "ro.build.type",
            ),
            (
                DevProductMarkers {
                    debuggable: "1",
                    ..valid
                },
                "ro.debuggable",
            ),
        ] {
            assert_eq!(validate_dev_product(markers).unwrap_err().name, name);
        }
    }
}
