use std::path::Path;

use rand_core::OsRng;
use reticulum_rs::core::identity::{PrivateIdentity, PRIVATE_KEY_LENGTH};

use crate::error::AppResult;
use crate::identity::{read_identity_material, IdentityMaterialProvider};
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
    let raw = read_identity_material(path).map_err(|_| NativeRuntimeError::IdentityMissing)?;
    load_private_identity_bytes(&raw)
}

pub fn load_native_private_identity_file(
    path: &Path,
) -> Result<PrivateIdentity, NativeRuntimeError> {
    let raw = read_identity_material(path).map_err(|_| NativeRuntimeError::IdentityMissing)?;
    load_private_identity(&raw)
}

pub fn load_transport_private_identity_file(
    path: &Path,
) -> Result<rns_transport::identity::PrivateIdentity, NativeRuntimeError> {
    let raw = read_identity_material(path).map_err(|_| NativeRuntimeError::IdentityMissing)?;
    if raw.is_empty() {
        return Err(NativeRuntimeError::IdentityMissing);
    }
    rns_transport::identity::PrivateIdentity::from_private_key_bytes(&raw)
        .map_err(|_| NativeRuntimeError::IdentityInvalid)
}

#[cfg(all(feature = "native-rns-net", any()))]
pub fn load_rns_net_proof_signing_key_file(path: &Path) -> Result<[u8; 64], NativeRuntimeError> {
    let raw = read_identity_material(path).map_err(|_| NativeRuntimeError::IdentityMissing)?;
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

    const FIXED_PRIVATE_IDENTITY: [u8; PRIVATE_KEY_LENGTH] = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d,
        0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c,
        0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b,
        0x3c, 0x3d, 0x3e, 0x3f,
    ];
    const FIXED_ADDRESS_HASH: &str = "aca31af0441d81dbec71e82da0b4b5f5";

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
    fn fixed_private_identity_roundtrips_with_stable_public_identity() {
        let identity = load_private_identity(&FIXED_PRIVATE_IDENTITY).expect("load fixture");
        let transport_identity = rns_transport::identity::PrivateIdentity::from_private_key_bytes(
            &FIXED_PRIVATE_IDENTITY,
        )
        .expect("load transport fixture");

        assert_eq!(identity.to_private_key_bytes(), FIXED_PRIVATE_IDENTITY);
        assert_eq!(
            transport_identity.to_private_key_bytes(),
            FIXED_PRIVATE_IDENTITY
        );
        assert_eq!(identity.address_hash().to_hex_string(), FIXED_ADDRESS_HASH);
        assert_eq!(
            transport_identity.address_hash().to_hex_string(),
            FIXED_ADDRESS_HASH
        );
        assert_eq!(
            identity.as_identity().public_key_bytes(),
            transport_identity.as_identity().public_key_bytes()
        );
        assert_eq!(
            identity.as_identity().verifying_key_bytes(),
            transport_identity.as_identity().verifying_key_bytes()
        );
    }

    #[test]
    fn invalid_identity_file_is_rejected_without_mutation_or_regeneration() {
        let root = temp_dir("invalid-preserved");
        let path = root.join("identity");
        let invalid = b"invalid-existing-identity";
        std::fs::write(&path, invalid).expect("write invalid identity fixture");

        let error = match load_native_private_identity_file(&path) {
            Ok(_) => panic!("invalid identity unexpectedly loaded"),
            Err(error) => error,
        };

        assert_eq!(error, NativeRuntimeError::IdentityInvalid);
        assert_eq!(
            std::fs::read(&path).expect("read preserved fixture"),
            invalid
        );
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
