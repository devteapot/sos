#[path = "../../build-support/core_dev_credential_protocol.rs"]
mod core_dev_credential_protocol;

fn main() {
    use std::{env, path::Path, process::Command};

    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    core_dev_credential_protocol::generate(workspace.as_path());
    println!("cargo:rustc-cfg=core_dev_credential_protocol_host_test");
    let harness = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cpp_client_harness.cpp");
    println!("cargo:rerun-if-changed={}", harness.display());
    println!(
        "cargo:rerun-if-changed={}",
        workspace
            .join("aosp/device/sos/a33x/core/dev_credential_client.cpp")
            .display()
    );
    let output =
        Path::new(&env::var_os("OUT_DIR").expect("OUT_DIR")).join("core-dev-credential-cpp-client");
    let compiler = env::var_os("CXX").unwrap_or_else(|| "c++".into());
    let status = Command::new(compiler)
        .args(["-std=c++20", "-Wall", "-Werror", "-Wextra", "-O2"])
        .arg(&harness)
        .arg("-o")
        .arg(&output)
        .status()
        .expect("run host C++ compiler");
    assert!(status.success(), "compile production C++ client harness");
    println!("cargo:rustc-env=CORE_DEV_CPP_CLIENT={}", output.display());
}
