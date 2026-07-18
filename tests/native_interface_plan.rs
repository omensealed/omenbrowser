#![cfg(feature = "native-reticulum")]

use omenbrowser_rs::interfaces::ReticulumInterfaceProfile;
use omenbrowser_rs::runtime::native::error::NativeRuntimeError;
use omenbrowser_rs::runtime::native::interface::{
    plan_interface, plan_interfaces, validate_startup_plans,
};

#[test]
fn maps_tcp_client_without_secret_fields() {
    let mut profile = ReticulumInterfaceProfile::tcp_client("tcp", "Gateway");
    profile.target_host = "127.0.0.1".into();
    profile.target_port = 4242;
    profile.passphrase = "secret-passphrase".into();

    let plan = plan_interface(&profile);
    let debug = format!("{plan:?}");

    assert!(plan.supported);
    assert_eq!(plan.endpoint.as_ref().expect("endpoint").port, 4242);
    assert!(plan.ifac_configured);
    assert!(!debug.contains("secret-passphrase"));
}

#[test]
fn unsupported_enabled_interfaces_fail_startup_validation() {
    let profile = ReticulumInterfaceProfile::rnode("rnode", "LoRa");
    let plans = plan_interfaces(&[profile]);
    let error = validate_startup_plans(&plans).expect_err("unsupported");

    assert!(matches!(
        error,
        NativeRuntimeError::UnsupportedInterface { ref kind, .. } if kind == "rnode"
    ));
}

#[test]
fn disabled_unsupported_interfaces_do_not_block_startup() {
    let mut profile = ReticulumInterfaceProfile::i2p("i2p", "I2P");
    profile.enabled = false;
    let plans = plan_interfaces(&[profile]);

    validate_startup_plans(&plans).expect("disabled unsupported profile");
}

#[test]
fn enabled_tcp_client_requires_nonempty_host() {
    let profile = ReticulumInterfaceProfile::tcp_client("tcp", "Gateway");
    let plans = plan_interfaces(&[profile]);

    let error = validate_startup_plans(&plans).expect_err("empty TCP host");

    assert!(matches!(
        error,
        NativeRuntimeError::InvalidInterface { ref reason, .. }
            if reason == "TCP client host is empty"
    ));
}

#[test]
fn enabled_tcp_client_rejects_zero_port() {
    let mut profile = ReticulumInterfaceProfile::tcp_client("tcp", "Gateway");
    profile.target_host = "127.0.0.1".into();
    profile.target_port = 0;
    let plans = plan_interfaces(&[profile]);

    let error = validate_startup_plans(&plans).expect_err("zero TCP port");

    assert!(matches!(
        error,
        NativeRuntimeError::InvalidInterface { ref reason, .. }
            if reason == "TCP client port must be between 1 and 65535"
    ));
}

#[test]
fn disabled_invalid_tcp_client_does_not_block_startup() {
    let mut profile = ReticulumInterfaceProfile::tcp_client("tcp", "Gateway");
    profile.enabled = false;
    let plans = plan_interfaces(&[profile]);

    validate_startup_plans(&plans).expect("disabled invalid TCP client");
}
