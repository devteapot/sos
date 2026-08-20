use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

const NAMES: &[&str] = &[
    "SOS_CORE_DEV_V1_MAGIC_0",
    "SOS_CORE_DEV_V1_MAGIC_1",
    "SOS_CORE_DEV_V1_MAGIC_2",
    "SOS_CORE_DEV_V1_MAGIC_3",
    "SOS_CORE_DEV_V1_VERSION",
    "SOS_CORE_DEV_V1_OP_PROBE",
    "SOS_CORE_DEV_V1_OP_SET",
    "SOS_CORE_DEV_V1_OP_CLEAR",
    "SOS_CORE_DEV_V1_OP_STATUS",
    "SOS_CORE_DEV_V1_OP_AGENT_SMOKE",
    "SOS_CORE_DEV_V1_STATUS_OK",
    "SOS_CORE_DEV_V1_STATUS_REJECTED",
    "SOS_CORE_DEV_V1_STATUS_WRONG_PEER",
    "SOS_CORE_DEV_V1_STATUS_PROTOCOL_MISMATCH",
    "SOS_CORE_DEV_V1_STATUS_CONFIGURED",
    "SOS_CORE_DEV_V1_STATUS_EMPTY",
    "SOS_CORE_DEV_V1_REQUEST_HEADER_BYTES",
    "SOS_CORE_DEV_V1_ACK_BYTES",
    "SOS_CORE_DEV_V1_MAX_PAYLOAD_BYTES",
];

pub fn generate(workspace_root: &Path) {
    println!("cargo:rustc-check-cfg=cfg(core_dev_credential_protocol_host_test)");
    let header = workspace_root.join("aosp/device/sos/a33x/core/dev_credential_protocol_v1.h");
    println!("cargo:rerun-if-changed={}", header.display());
    let source = fs::read_to_string(&header).expect("read canonical Core-dev v1 header");
    let mut values = BTreeMap::new();
    for line in source.lines() {
        let mut fields = line.split_ascii_whitespace();
        if fields.next() != Some("#define") {
            continue;
        }
        let Some(name) = fields.next() else { continue };
        if !NAMES.contains(&name) {
            continue;
        }
        let value = fields.next().expect("protocol define value");
        assert!(fields.next().is_none(), "unexpected protocol define suffix");
        let parsed = if let Some(hex) = value.strip_prefix("0x") {
            usize::from_str_radix(hex, 16)
        } else {
            value.parse()
        }
        .expect("numeric protocol define");
        assert!(
            values.insert(name, parsed).is_none(),
            "duplicate protocol define"
        );
    }
    for name in NAMES {
        assert!(values.contains_key(name), "missing protocol define {name}");
    }
    for name in &NAMES[..16] {
        assert!(
            values[name] <= u8::MAX as usize,
            "byte define out of range {name}"
        );
    }
    assert_eq!(
        [
            values["SOS_CORE_DEV_V1_MAGIC_0"],
            values["SOS_CORE_DEV_V1_MAGIC_1"],
            values["SOS_CORE_DEV_V1_MAGIC_2"],
            values["SOS_CORE_DEV_V1_MAGIC_3"],
        ],
        [b'S' as usize, b'O' as usize, b'S' as usize, b'K' as usize],
        "v1 magic changed"
    );
    assert_eq!(values["SOS_CORE_DEV_V1_VERSION"], 1, "v1 version changed");
    assert_eq!(
        values["SOS_CORE_DEV_V1_REQUEST_HEADER_BYTES"], 8,
        "v1 request header changed"
    );
    assert_eq!(
        values["SOS_CORE_DEV_V1_ACK_BYTES"], 6,
        "v1 acknowledgement changed"
    );
    assert_eq!(
        values["SOS_CORE_DEV_V1_MAX_PAYLOAD_BYTES"], 512,
        "v1 maximum payload changed"
    );
    let mut generated = String::from("// Generated from dev_credential_protocol_v1.h.\n");
    for name in NAMES {
        let rust_name = name.trim_start_matches("SOS_CORE_DEV_V1_");
        generated.push_str(&format!(
            "pub(super) const {}: usize = {};\n",
            rust_name, values[name]
        ));
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"))
        .join("core_dev_credential_protocol_v1.rs");
    fs::write(output, generated).expect("write generated Core-dev v1 Rust constants");
}
