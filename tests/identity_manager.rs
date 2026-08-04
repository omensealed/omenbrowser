use std::path::PathBuf;

use omenbrowser_rs::error::AppResult;
use omenbrowser_rs::identity::{
    hash_for_bytes, IdentityManager, IdentityMaterialProvider, IdentityProfile,
    IDENTITY_BACKUP_MAX_FILES, IDENTITY_BACKUP_MAX_SCAN_ENTRIES, IDENTITY_BACKUP_MAX_TOTAL_BYTES,
    IDENTITY_DISCOVERY_MAX_PROFILES, IDENTITY_DISCOVERY_MAX_SCAN_ENTRIES,
    IDENTITY_MATERIAL_MAX_BYTES,
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
fn identity_material_admission_accepts_exact_limit_and_rejects_next_byte() {
    let root = temp_dir("material-limit");
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));
    let exact = root.join("exact_identity");
    std::fs::write(&exact, vec![b'i'; IDENTITY_MATERIAL_MAX_BYTES as usize])
        .expect("exact identity");
    let oversized = root.join("oversized_identity");
    let oversized_file = std::fs::File::create(&oversized).expect("oversized identity");
    oversized_file
        .set_len(IDENTITY_MATERIAL_MAX_BYTES + 1)
        .expect("extend sparse identity");
    drop(oversized_file);

    manager
        .attach_existing(exact, None)
        .expect("exact identity must be admitted");
    let error = manager
        .attach_existing(oversized.clone(), None)
        .expect_err("next-byte identity must be rejected");

    assert!(error.to_string().contains("exceeds"));
    assert_eq!(
        std::fs::symlink_metadata(oversized)
            .expect("oversized metadata")
            .len(),
        IDENTITY_MATERIAL_MAX_BYTES + 1
    );
}

#[test]
fn empty_identity_material_is_rejected() {
    let root = temp_dir("empty-material");
    let source = root.join("empty_identity");
    std::fs::write(&source, b"").expect("empty identity");
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));

    let error = manager
        .attach_existing(source, None)
        .expect_err("empty identity must be rejected");

    assert!(error.to_string().contains("must not be empty"));
}

#[cfg(unix)]
#[test]
fn identity_material_admission_does_not_follow_symlinks() {
    use std::os::unix::fs::symlink;

    let root = temp_dir("material-symlink");
    let outside = root.join("outside_identity");
    let sentinel = b"outside identity material";
    std::fs::write(&outside, sentinel).expect("outside identity");
    let linked = root.join("linked_identity");
    symlink(&outside, &linked).expect("identity symlink");
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));

    let error = manager
        .attach_existing(linked.clone(), None)
        .expect_err("identity symlink must be rejected");

    assert!(error.to_string().contains("symbolic link"));
    assert_eq!(std::fs::read(outside).expect("outside identity"), sentinel);
    assert!(linked
        .symlink_metadata()
        .expect("link metadata")
        .file_type()
        .is_symlink());
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
fn rejected_import_preserves_existing_target_without_creating_backup() {
    let root = temp_dir("import-reject-preserves");
    let source = root.join("external_identity");
    let source_file = std::fs::File::create(&source).expect("source identity");
    source_file
        .set_len(IDENTITY_MATERIAL_MAX_BYTES + 1)
        .expect("oversized source");
    drop(source_file);
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));
    std::fs::create_dir_all(&manager.identities_dir).expect("identities dir");
    let target = manager.identities_dir.join("external_identity");
    let previous = b"existing managed identity";
    std::fs::write(&target, previous).expect("existing target");

    manager
        .import_identity_copy(source, None)
        .expect_err("oversized import must fail before backup or replacement");

    assert_eq!(std::fs::read(target).expect("existing target"), previous);
    assert!(!manager.backups_dir.exists());
}

#[test]
fn managed_identity_discovery_is_profile_bounded() {
    let root = temp_dir("profile-limit");
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));
    std::fs::create_dir_all(&manager.identities_dir).expect("identities dir");
    for index in 0..IDENTITY_DISCOVERY_MAX_PROFILES {
        std::fs::write(
            manager.identities_dir.join(format!("identity-{index:04}")),
            b"identity",
        )
        .expect("managed identity");
    }

    assert_eq!(
        manager
            .list_managed_identities()
            .expect("exact profile limit")
            .len(),
        IDENTITY_DISCOVERY_MAX_PROFILES
    );
    std::fs::write(
        manager.identities_dir.join("identity-over-limit"),
        b"identity",
    )
    .expect("extra managed identity");
    let error = manager
        .list_managed_identities()
        .expect_err("next profile must be rejected");
    assert!(error.to_string().contains("profile limit"));
}

#[test]
fn managed_identity_discovery_refuses_directory_scan_saturation() {
    let root = temp_dir("scan-limit");
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));
    std::fs::create_dir_all(&manager.identities_dir).expect("identities dir");
    for index in 0..=IDENTITY_DISCOVERY_MAX_SCAN_ENTRIES {
        std::fs::create_dir(manager.identities_dir.join(format!("ignored-{index:04}")))
            .expect("unrelated directory");
    }

    let error = manager
        .list_managed_identities()
        .expect_err("saturated discovery must fail explicitly");

    assert!(error.to_string().contains("entry scan limit"));
    std::fs::remove_dir_all(root).expect("remove saturated fixture");
}

#[cfg(unix)]
#[test]
fn managed_identity_discovery_ignores_symlink_entries_and_refuses_symlink_root() {
    use std::os::unix::fs::symlink;

    let root = temp_dir("discovery-symlink");
    let outside = root.join("outside_identity");
    let sentinel = b"outside identity material";
    std::fs::write(&outside, sentinel).expect("outside identity");
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));
    std::fs::create_dir_all(&manager.identities_dir).expect("identities dir");
    symlink(&outside, manager.identities_dir.join("linked_identity")).expect("identity link");

    assert!(manager
        .list_managed_identities()
        .expect("linked entry is ignored")
        .is_empty());
    assert_eq!(std::fs::read(&outside).expect("outside identity"), sentinel);

    std::fs::remove_dir_all(&manager.identities_dir).expect("remove identity root");
    symlink(&root, &manager.identities_dir).expect("identity root link");
    let error = manager
        .list_managed_identities()
        .expect_err("identity root symlink must be rejected");
    assert!(error.to_string().contains("identity root"));
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

#[test]
fn managed_identity_backups_are_bounded_and_legacy_files_are_preserved() {
    let root = temp_dir("backup-retention");
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));
    let profile = manager
        .create_managed_identity("Default Identity")
        .expect("create identity");
    std::fs::create_dir_all(&manager.backups_dir).expect("backup dir");
    let legacy = manager.backups_dir.join("default_identity.backup.legacy");
    std::fs::write(&legacy, b"operator-preserved legacy backup").expect("legacy backup");

    let mut newest = None;
    for _ in 0..IDENTITY_BACKUP_MAX_FILES + 4 {
        newest = Some(
            manager
                .export_backup(&profile, None)
                .expect("managed backup"),
        );
    }

    let mut managed_count = 0;
    let mut managed_bytes = 0;
    for entry in std::fs::read_dir(&manager.backups_dir).expect("backup entries") {
        let entry = entry.expect("backup entry");
        let name = entry.file_name();
        if name
            .to_str()
            .is_some_and(|name| name.starts_with("omen-identity.backup."))
        {
            managed_count += 1;
            managed_bytes += entry.metadata().expect("backup metadata").len();
        }
    }

    assert_eq!(managed_count, IDENTITY_BACKUP_MAX_FILES);
    assert!(managed_bytes <= IDENTITY_BACKUP_MAX_TOTAL_BYTES);
    assert!(newest.expect("newest backup path").exists());
    assert_eq!(
        std::fs::read(legacy).expect("preserved legacy backup"),
        b"operator-preserved legacy backup"
    );
}

#[test]
fn backup_scan_saturation_aborts_replacement_after_preserving_new_backup() {
    let root = temp_dir("backup-scan-limit");
    let source = root.join("external_identity");
    std::fs::write(&source, b"replacement").expect("source identity");
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));
    std::fs::create_dir_all(&manager.identities_dir).expect("identities dir");
    let target = manager.identities_dir.join("external_identity");
    std::fs::write(&target, b"previous identity").expect("previous identity");
    std::fs::create_dir_all(&manager.backups_dir).expect("backup dir");
    for index in 0..IDENTITY_BACKUP_MAX_SCAN_ENTRIES {
        std::fs::create_dir(manager.backups_dir.join(format!("unrelated-{index:04}")))
            .expect("unrelated backup entry");
    }

    let error = manager
        .import_identity_copy(source, None)
        .expect_err("saturated backup discovery must abort replacement");

    assert!(error.to_string().contains("entry scan limit"));
    assert_eq!(
        std::fs::read(target).expect("preserved identity"),
        b"previous identity"
    );
    assert_eq!(
        std::fs::read_dir(&manager.backups_dir)
            .expect("backup entries")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_file())
            .count(),
        1
    );
    std::fs::remove_dir_all(root).expect("remove saturated fixture");
}

#[cfg(unix)]
#[test]
fn identity_publication_is_private_and_refuses_linked_storage_roots() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let root = temp_dir("publication-safety");
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));
    let profile = manager
        .create_managed_identity("Private Identity")
        .expect("create private identity");
    assert_eq!(
        std::fs::metadata(&profile.path)
            .expect("identity metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );

    let linked_manager = IdentityManager::new(root.join("linked-identities"), root.join("unused"));
    symlink(&manager.identities_dir, &linked_manager.identities_dir).expect("linked root");
    linked_manager
        .create_managed_identity("Rejected")
        .expect_err("linked identity storage root must be rejected");

    assert!(std::fs::symlink_metadata(&linked_manager.identities_dir)
        .expect("linked root metadata")
        .file_type()
        .is_symlink());
}

#[cfg(unix)]
#[test]
fn managed_identity_discovery_repairs_mode_without_changing_identity_or_hash() {
    use std::os::unix::fs::PermissionsExt;

    let root = temp_dir("existing-permission-repair");
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));
    let profile = manager
        .create_managed_identity("Existing Identity")
        .expect("identity");
    let bytes = std::fs::read(&profile.path).expect("identity bytes");
    std::fs::set_permissions(&profile.path, std::fs::Permissions::from_mode(0o644))
        .expect("permissive identity mode");

    let discovered = manager
        .list_managed_identities()
        .expect("discover identity");

    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].hash_hex, profile.hash_hex);
    assert_eq!(std::fs::read(&profile.path).expect("identity bytes"), bytes);
    assert_eq!(
        std::fs::metadata(&profile.path)
            .expect("identity metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    std::fs::remove_dir_all(root).expect("cleanup");
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

struct OversizedIdentityMaterialProvider;

impl IdentityMaterialProvider for OversizedIdentityMaterialProvider {
    fn provider_name(&self) -> &'static str {
        "oversized"
    }

    fn create_identity_material(&self, _label: &str) -> AppResult<Vec<u8>> {
        Ok(vec![0; IDENTITY_MATERIAL_MAX_BYTES as usize + 1])
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

#[test]
fn managed_identity_creation_rejects_provider_overflow_before_filesystem_mutation() {
    let root = temp_dir("provider-overflow");
    let manager = IdentityManager::new(root.join("identities"), root.join("backups"));

    manager
        .create_managed_identity_with_provider("Oversized", &OversizedIdentityMaterialProvider)
        .expect_err("oversized provider output must be rejected");

    assert!(!manager.identities_dir.exists());
    assert!(!manager.backups_dir.exists());
}
