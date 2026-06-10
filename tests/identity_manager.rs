use std::path::PathBuf;

use omenbrowser_rs::error::AppResult;
use omenbrowser_rs::identity::{
    hash_for_bytes, IdentityManager, IdentityMaterialProvider, IdentityProfile,
};
use omenbrowser_rs::storage::settings::AppSettings;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "omenbrowser-rs-identity-integration-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn identity_profile_round_trips_json() {
    let profile = IdentityProfile {
        label: "main".into(),
        path: PathBuf::from("/tmp/identity"),
        hash_hex: "abcd1234".into(),
        managed: true,
    };

    let json = serde_json::to_string(&profile).expect("serialize identity profile");
    let decoded: IdentityProfile = serde_json::from_str(&json).expect("deserialize profile");

    assert_eq!(decoded, profile);
}

#[test]
fn managed_identity_creation_activates_settings() {
    let root = temp_dir("create");
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));

    let profile = manager
        .create_managed_identity("Default Identity")
        .expect("create managed identity");
    let mut settings = AppSettings::default();
    IdentityManager::activate_profile(&mut settings, &profile);

    assert!(profile.path.exists());
    assert!(profile.managed);
    assert_eq!(profile.hash_hex.len(), 32);
    assert_eq!(settings.identity_path, Some(profile.path));
    assert_eq!(
        settings.active_identity_label.as_deref(),
        Some("Default Identity")
    );
}

#[test]
fn managed_identity_creation_backs_up_existing_identity_file() {
    let root = temp_dir("backup-existing");
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));
    std::fs::create_dir_all(&manager.identities_dir).expect("create identities dir");
    std::fs::write(manager.identities_dir.join("default_identity"), b"old").expect("seed identity");

    manager
        .create_managed_identity("Default Identity")
        .expect("create managed identity");
    assert_eq!(
        std::fs::read(manager.identities_dir.join("default_identity")).expect("read original"),
        b"old"
    );
    let managed_count = std::fs::read_dir(&manager.identities_dir)
        .expect("read identities dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .count();
    let backup_count = std::fs::read_dir(&manager.backups_dir)
        .expect("read backup dir")
        .filter_map(Result::ok)
        .count();

    assert_eq!(managed_count, 2);
    assert_eq!(backup_count, 1);
}

#[test]
fn attach_existing_identity_references_source_without_copying() {
    let root = temp_dir("attach");
    let source = root.join("external_identity");
    std::fs::write(&source, b"external").expect("write external identity");
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));

    let profile = manager
        .attach_existing(source.clone(), None)
        .expect("attach identity");

    assert_eq!(profile.path, source);
    assert!(!profile.managed);
    assert!(!manager.identities_dir.exists());
}

#[test]
fn import_identity_copy_backs_up_existing_target() {
    let root = temp_dir("import");
    let source = root.join("external_identity");
    std::fs::write(&source, b"new").expect("write source identity");
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));
    std::fs::create_dir_all(&manager.identities_dir).expect("create identities dir");
    std::fs::write(manager.identities_dir.join("external_identity"), b"old").expect("seed target");

    let profile = manager
        .import_identity_copy(source, Some("Imported"))
        .expect("import identity");

    assert!(profile.managed);
    assert_eq!(std::fs::read(profile.path).expect("read imported"), b"new");
    assert_eq!(
        std::fs::read_dir(&manager.backups_dir)
            .expect("read backups")
            .filter_map(Result::ok)
            .count(),
        1
    );
}

#[test]
fn export_backup_copies_identity_to_timestamped_file() {
    let root = temp_dir("export");
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));
    let profile = manager
        .create_managed_identity("Default Identity")
        .expect("create identity");

    let backup = manager
        .export_backup(&profile, None)
        .expect("export backup");

    assert!(backup.exists());
    assert!(backup
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .contains(".backup."));
}

struct FixedIdentityMaterialProvider;

impl IdentityMaterialProvider for FixedIdentityMaterialProvider {
    fn provider_name(&self) -> &'static str {
        "fixed"
    }

    fn create_identity_material(&self, _label: &str) -> AppResult<Vec<u8>> {
        Ok(b"fixed-native-material".to_vec())
    }
}

#[test]
fn managed_identity_creation_accepts_material_provider_boundary() {
    let root = temp_dir("provider");
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));

    let profile = manager
        .create_managed_identity_with_provider("Native", &FixedIdentityMaterialProvider)
        .expect("create identity");

    assert_eq!(
        std::fs::read(profile.path).expect("read identity"),
        b"fixed-native-material"
    );
    assert_eq!(profile.hash_hex, hash_for_bytes(b"fixed-native-material"));
}
