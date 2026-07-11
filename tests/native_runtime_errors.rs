#![cfg(feature = "native-reticulum")]

use omenbrowser_rs::error::AppError;
use omenbrowser_rs::runtime::native::{NativePageFetchFailureStage, NativeRuntimeError};

const FIXTURE_DESTINATION_HASH: &str = "00112233445566778899aabbccddeeff";

#[test]
fn native_error_mapping_avoids_secret_paths() {
    let error = AppError::from(NativeRuntimeError::IdentityInvalid).to_string();

    assert!(error.contains("identity is invalid"));
    assert!(!error.contains("/home/"));
    assert!(!error.contains("secret"));
}

#[test]
fn unsupported_interface_is_structured() {
    let error = AppError::from(NativeRuntimeError::UnsupportedInterface {
        profile: "LoRa".into(),
        kind: "rnode".into(),
        reason: "transport API missing".into(),
    })
    .to_string();

    assert!(error.contains("LoRa"));
    assert!(error.contains("rnode"));
    assert!(error.contains("transport API missing"));
}

#[test]
fn page_fetch_error_reports_stage_without_path() {
    let error = AppError::from(NativeRuntimeError::PageFetchFailed {
        destination: FIXTURE_DESTINATION_HASH.into(),
        stage: NativePageFetchFailureStage::DestinationIdentity,
        detail: "destination signing public key is not known".into(),
    })
    .to_string();

    assert!(error.contains(FIXTURE_DESTINATION_HASH));
    assert!(error.contains("destination identity"));
    assert!(!error.contains("/page/private"));
}
