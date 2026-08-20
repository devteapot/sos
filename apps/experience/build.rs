#[path = "../../build-support/core_dev_credential_protocol.rs"]
mod core_dev_credential_protocol;

fn main() {
    core_dev_credential_protocol::generate(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .as_path(),
    );
}
