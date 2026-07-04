use std::path::Path;

use rand_core::OsRng;
use reticulum_rs::core::identity::{PrivateIdentity, PRIVATE_KEY_LENGTH};

use crate::error::AppResult;
use crate::identity::IdentityMaterialProvider;
use crate::runtime::native::NativeRuntimeError;

#[derive(Clone, Debug, Default)]
pub struct NativeReticulumIdentityProvider;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeIdentitySummary {
    pub address_hash_hex: String,
    pub byte_len: usize,
}

impl IdentityMaterialProvider for NativeReticulumIdentityProvider {
    fn provider_name(&self) -> &'static str {
        "native-reticulum"
    }

    fn create_identity_material(&self, _label: &str) -> AppResult<Vec<u8>> {
        let identity = PrivateIdentity::new_from_rand(OsRng);
        Ok(identity.to_private_key_bytes().to_vec())
    }
}

pub fn load_private_identity_bytes(
    raw: &[u8],
) -> Result<NativeIdentitySummary, NativeRuntimeError> {
    if raw.is_empty() {
        return Err(NativeRuntimeError::IdentityMissing);
    }

    let identity = PrivateIdentity::from_private_key_bytes(raw)
        .map_err(|_| NativeRuntimeError::IdentityInvalid)?;

    Ok(NativeIdentitySummary {
        address_hash_hex: identity.address_hash().to_hex_string(),
        byte_len: raw.len(),
    })
}

pub fn load_private_identity(raw: &[u8]) -> Result<PrivateIdentity, NativeRuntimeError> {
    if raw.is_empty() {
        return Err(NativeRuntimeError::IdentityMissing);
    }
    PrivateIdentity::from_private_key_bytes(raw).map_err(|_| NativeRuntimeError::IdentityInvalid)
}

pub fn load_private_identity_file(
    path: &Path,
) -> Result<NativeIdentitySummary, NativeRuntimeError> {
    let raw = std::fs::read(path).map_err(|_| NativeRuntimeError::IdentityMissing)?;
    load_private_identity_bytes(&raw)
}

pub fn load_native_private_identity_file(
    path: &Path,
) -> Result<PrivateIdentity, NativeRuntimeError> {
    let raw = std::fs::read(path).map_err(|_| NativeRuntimeError::IdentityMissing)?;
    load_private_identity(&raw)
}

pub fn load_transport_private_identity_file(
    path: &Path,
) -> Result<rns_transport::identity::PrivateIdentity, NativeRuntimeError> {
    let raw = std::fs::read(path).map_err(|_| NativeRuntimeError::IdentityMissing)?;
    if raw.is_empty() {
        return Err(NativeRuntimeError::IdentityMissing);
    }
    rns_transport::identity::PrivateIdentity::from_private_key_bytes(&raw)
        .map_err(|_| NativeRuntimeError::IdentityInvalid)
}

#[cfg(all(feature = "native-rns-net", any()))]
pub fn load_rns_net_proof_signing_key_file(path: &Path) -> Result<[u8; 64], NativeRuntimeError> {
    let raw = std::fs::read(path).map_err(|_| NativeRuntimeError::IdentityMissing)?;
    load_private_identity_bytes(&raw)?;
    raw.try_into()
        .map_err(|_| NativeRuntimeError::IdentityInvalid)
}

#[cfg(all(feature = "native-rns-net", any()))]
pub fn rns_net_identity_from_signing_key(signing_key: &[u8; 64]) -> rns_crypto::identity::Identity {
    rns_crypto::identity::Identity::from_private_key(signing_key)
}

pub fn native_private_identity_len() -> usize {
    PRIVATE_KEY_LENGTH
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{hash_for_bytes, IdentityManager};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "omenbrowser-rs-native-identity-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn native_provider_creates_loadable_reticulum_identity_material() {
        let provider = NativeReticulumIdentityProvider;
        let raw = provider
            .create_identity_material("Native")
            .expect("create native identity material");
        let summary = load_private_identity_bytes(&raw).expect("load native identity");

        assert_eq!(raw.len(), native_private_identity_len());
        assert_eq!(summary.byte_len, native_private_identity_len());
        assert_eq!(summary.address_hash_hex.len(), 32);
    }

    #[test]
    fn identity_manager_keeps_file_safety_while_using_native_provider() {
        let root = temp_dir("manager");
        let manager = IdentityManager::new(root.join("identities"), root.join("backups"));
        let provider = NativeReticulumIdentityProvider;

        let profile = manager
            .create_managed_identity_with_provider("Native", &provider)
            .expect("create managed native identity");
        let raw = std::fs::read(&profile.path).expect("read identity");

        assert_eq!(profile.hash_hex, hash_for_bytes(&raw));
        load_private_identity_file(&profile.path).expect("load stored native identity");
    }

    #[test]
    fn invalid_identity_material_is_rejected_without_exposing_bytes() {
        let error = load_private_identity_bytes(b"not-a-reticulum-private-identity")
            .expect_err("invalid identity");

        assert_eq!(error, NativeRuntimeError::IdentityInvalid);
        assert!(!format!("{error:?}").contains("not-a-reticulum"));
    }

    #[test]
    fn native_private_identity_file_loads_exact_crate_type() {
        let root = temp_dir("native-type");
        let path = root.join("identity");
        let provider = NativeReticulumIdentityProvider;
        let raw = provider
            .create_identity_material("Native")
            .expect("create identity");
        std::fs::write(&path, raw).expect("write identity");

        let identity = load_native_private_identity_file(&path).expect("load native identity");

        assert_eq!(identity.address_hash().to_hex_string().len(), 32);
    }

    #[test]
    fn transport_private_identity_file_loads_exact_crate_type() {
        let root = temp_dir("transport-type");
        let path = root.join("identity");
        let provider = NativeReticulumIdentityProvider;
        let raw = provider
            .create_identity_material("Native")
            .expect("create identity");
        std::fs::write(&path, raw).expect("write identity");

        let identity =
            load_transport_private_identity_file(&path).expect("load transport identity");

        assert_eq!(identity.address_hash().to_hex_string().len(), 32);
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn rns_net_proof_signing_key_loads_full_private_identity_key() {
        let root = temp_dir("proof-key");
        let path = root.join("identity");
        let provider = NativeReticulumIdentityProvider;
        let raw = provider
            .create_identity_material("Native")
            .expect("create identity");
        std::fs::write(&path, raw).expect("write identity");

        let key = load_rns_net_proof_signing_key_file(&path).expect("load proof key");

        assert_eq!(key.len(), 64);
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    #[test]
    fn rns_net_identity_from_signing_key_preserves_identity_hash() {
        let root = temp_dir("rns-net-identity");
        let path = root.join("identity");
        let provider = NativeReticulumIdentityProvider;
        let raw = provider
            .create_identity_material("Native")
            .expect("create identity");
        std::fs::write(&path, raw).expect("write identity");
        let summary = load_private_identity_file(&path).expect("summary");
        let key = load_rns_net_proof_signing_key_file(&path).expect("proof key");

        let identity = rns_net_identity_from_signing_key(&key);

        assert_eq!(
            hex_bytes(identity.hash().as_slice()),
            summary.address_hash_hex
        );
    }

    #[cfg(all(feature = "native-rns-net", any()))]
    fn hex_bytes(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
