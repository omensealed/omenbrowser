//! Typed value parsing for the browser compatibility CLI.

use crate::{messaging::DeliveryMode, storage::settings::RuntimeBackendSetting};

pub fn parse_runtime_backend(value: &str) -> anyhow::Result<RuntimeBackendSetting> {
    match value {
        "auto" => Ok(RuntimeBackendSetting::Auto),
        "mock" => Ok(RuntimeBackendSetting::Mock),
        "reticulum" | "native" | "native-reticulum" => Ok(RuntimeBackendSetting::Reticulum),
        "bridge" => Ok(RuntimeBackendSetting::Bridge),
        other => Err(anyhow::anyhow!(
            "invalid backend {other}; expected auto, mock, or reticulum"
        )),
    }
}

pub fn parse_lxmf_delivery_mode(value: &str) -> anyhow::Result<DeliveryMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "direct" => Ok(DeliveryMode::Direct),
        "propagated" | "propagation" | "prop" => Ok(DeliveryMode::Propagated),
        other => Err(anyhow::anyhow!(
            "invalid LXMF smoke delivery mode {other}; expected direct or propagated"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_backend_parser_preserves_aliases_and_exact_errors() {
        assert_eq!(
            parse_runtime_backend("auto").expect("auto"),
            RuntimeBackendSetting::Auto
        );
        assert_eq!(
            parse_runtime_backend("mock").expect("mock"),
            RuntimeBackendSetting::Mock
        );
        for alias in ["reticulum", "native", "native-reticulum"] {
            assert_eq!(
                parse_runtime_backend(alias).expect("Reticulum alias"),
                RuntimeBackendSetting::Reticulum
            );
        }
        assert_eq!(
            parse_runtime_backend("bridge").expect("legacy bridge"),
            RuntimeBackendSetting::Bridge
        );
        assert_eq!(
            parse_runtime_backend("RETICULUM")
                .expect_err("backend parsing remains case-sensitive")
                .to_string(),
            "invalid backend RETICULUM; expected auto, mock, or reticulum"
        );
    }

    #[test]
    fn lxmf_delivery_parser_preserves_normalization_aliases_and_exact_errors() {
        assert_eq!(
            parse_lxmf_delivery_mode(" direct ").expect("trimmed direct"),
            DeliveryMode::Direct
        );
        for alias in ["propagated", "propagation", "prop", " PROP "] {
            assert_eq!(
                parse_lxmf_delivery_mode(alias).expect("propagated alias"),
                DeliveryMode::Propagated
            );
        }
        assert_eq!(
            parse_lxmf_delivery_mode(" Unknown ")
                .expect_err("invalid delivery mode")
                .to_string(),
            "invalid LXMF smoke delivery mode unknown; expected direct or propagated"
        );
    }
}
