// Exercise the production pre-exec implementation without linking the desktop
// GPUI stack required by the full experience crate.
mod core_child_fds {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/experience/src/core_child_fds.rs"
    ));
}

mod android_agent_contract {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/experience/src/android_agent_contract.rs"
    ));
}

#[cfg(test)]
mod product_launch_contract {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::android_agent_contract::{CoreChildLaunchContract, CORE_CHILD_LAUNCH};

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn read(relative: &str) -> String {
        fs::read_to_string(root().join(relative)).unwrap()
    }

    fn soong_block<'a>(source: &'a str, kind: &str, name: &str) -> &'a str {
        let header = format!("{kind} {{\n    name: \"{name}\",");
        let start = source.find(&header).unwrap();
        let remainder = &source[start..];
        let end = remainder.find("\n}\n").unwrap() + 3;
        &remainder[..end]
    }

    #[test]
    fn installed_products_native_launch_policy_and_node_entrypoints_are_one_contract() {
        let blueprint = read("aosp/device/sos/a33x/Android.bp");
        let ordinary_module = soong_block(&blueprint, "prebuilt_etc", "sos-agent-runner");
        let dev_module = soong_block(&blueprint, "prebuilt_etc", "sos-agent-runner-core-dev");
        assert!(ordinary_module.contains("src: \"prebuilts/sos-agent/agent-runner.cjs\""));
        assert!(ordinary_module.contains("filename: \"agent-runner.cjs\""));
        assert!(!ordinary_module.contains("core-dev"));
        assert!(dev_module.contains("src: \"prebuilts/sos-agent/agent-runner-core-dev.cjs\""));
        assert!(dev_module.contains("filename: \"agent-runner-core-dev.cjs\""));
        assert_eq!(
            blueprint.matches("filename: \"agent-runner.cjs\"").count(),
            1
        );
        assert_eq!(
            blueprint
                .matches("filename: \"agent-runner-core-dev.cjs\"")
                .count(),
            1
        );

        let ordinary_product = read("aosp/device/sos/a33x/lineage_sos_core1_a33x.mk");
        let dev_product = read("aosp/device/sos/a33x/lineage_sos_core1_dev_a33x.mk");
        assert!(ordinary_product
            .lines()
            .any(|line| line == "    sos-agent-runner"));
        for forbidden in [
            "sos-node-core-dev",
            "sos-agent-runner-core-dev",
            "sos-core-dev-credential",
            "ro.sos.dev_credential",
        ] {
            assert!(!ordinary_product.contains(forbidden));
        }
        for required in [
            "    sos-node-core-dev \\",
            "    sos-agent-runner-core-dev",
            "device/sos/a33x/sepolicy/core_dev_private",
        ] {
            assert!(dev_product.contains(required));
        }

        let ordinary_policy =
            read("aosp/device/sos/a33x/sepolicy/system_ext/private/sos_core_agent.te");
        let ordinary_contexts =
            read("aosp/device/sos/a33x/sepolicy/system_ext/private/file_contexts");
        assert!(ordinary_policy
            .contains("domain_auto_trans(sos_core_host, sos_node_exec, sos_core_agent)"));
        assert!(ordinary_policy.contains("allow netd sos_core_agent:fd use;"));
        assert!(ordinary_policy.contains(
            "allow netd sos_core_agent:tcp_socket { read write getattr setattr getopt setopt };"
        ));
        assert!(ordinary_contexts
            .contains("/system_ext/bin/sos-node                    u:object_r:sos_node_exec:s0"));
        assert!(!ordinary_policy.contains("sos_core_dev"));
        assert!(!ordinary_contexts.contains("core-dev"));

        let dev_policy =
            read("aosp/device/sos/a33x/sepolicy/core_dev_private/sos_core_dev_agent.te");
        let dev_contexts = read("aosp/device/sos/a33x/sepolicy/core_dev_private/file_contexts");
        assert!(dev_policy.contains(
            "domain_auto_trans(sos_core_host, sos_node_core_dev_exec, sos_core_dev_agent)"
        ));
        assert!(dev_policy.contains("allow netd sos_core_dev_agent:fd use;"));
        assert!(dev_policy.contains(
            "allow netd sos_core_dev_agent:tcp_socket { read write getattr setattr getopt setopt };"
        ));
        assert!(dev_contexts.contains(
            "/system_ext/bin/sos-node-core-dev           u:object_r:sos_node_core_dev_exec:s0"
        ));

        let ordinary_entrypoint = read("services/sos-agent/src/runner.ts");
        let dev_entrypoint = read("services/sos-agent/src/runner-core-dev.ts");
        let package = read("services/sos-agent/package.json");
        assert!(ordinary_entrypoint.contains("runStdio({"));
        assert!(!ordinary_entrypoint.contains("core-dev-proxy"));
        assert!(dev_entrypoint.contains("CORE_DEV_PROXY_HOOKS"));
        assert!(dev_entrypoint.contains("process.argv[2] !== \"stdio\""));
        assert_eq!(
            package.matches("--outfile=dist/agent-runner.cjs").count(),
            1
        );
        assert_eq!(
            package
                .matches("--outfile=dist/agent-runner-core-dev.cjs")
                .count(),
            1
        );

        let native_cpp = read("aosp/device/sos/a33x/core/host.cpp");
        assert!(!native_cpp.contains("agent-runner"));
        assert!(!native_cpp.contains("sos-node"));

        #[cfg(not(feature = "core-dev-credential"))]
        assert_eq!(
            CORE_CHILD_LAUNCH,
            CoreChildLaunchContract {
                node_path: "/system_ext/bin/sos-node",
                runner_path: "/system_ext/etc/sos-agent/agent-runner.cjs",
                node_identity: "ordinary_node",
                runner_identity: "ordinary_runner",
                expected_domain: "sos_core_agent",
            }
        );
        #[cfg(feature = "core-dev-credential")]
        assert_eq!(
            CORE_CHILD_LAUNCH,
            CoreChildLaunchContract {
                node_path: "/system_ext/bin/sos-node-core-dev",
                runner_path: "/system_ext/etc/sos-agent/agent-runner-core-dev.cjs",
                node_identity: "core_dev_node",
                runner_identity: "core_dev_runner",
                expected_domain: "sos_core_dev_agent",
            }
        );
    }
}
