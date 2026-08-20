#![allow(dead_code)]

mod core_credential {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/experience/src/core_credential.rs"
    ));
}

// Exercise the production wire decoder and state-dispatch tests without
// linking the desktop GPUI stack required by the full experience crate.
mod core_dev_credential {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/experience/src/core_dev_credential.rs"
    ));
}
