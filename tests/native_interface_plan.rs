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
