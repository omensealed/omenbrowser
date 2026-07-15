use omenbrowser_rs::plugins::{
    micronplus, PluginManifest, PluginPermission, PluginRegistry, PluginRegistryFile,
    BUILTIN_MICRONPLUS_PLUGIN_ID, BUILTIN_OMENCHAT_PLUGIN_ID, PLUGIN_DISCOVERY_MAX_INSTALLED,
    PLUGIN_INSTALL_MAX_DEPTH, PLUGIN_INSTALL_MAX_ENTRIES, PLUGIN_INSTALL_MAX_FILE_BYTES,
    PLUGIN_MANIFEST_MAX_BYTES, PLUGIN_REGISTRY_MAX_BYTES,
};

fn write_plugin_manifest(path: &std::path::Path, plugin_id: &str) {
    std::fs::write(
        path,
        format!(
            r#"{{
                "id": "{plugin_id}",
                "name": "Bounded plugin",
                "version": "0.1.0",
                "author": "OMENbrowser",
                "description": "Bounded discovery fixture",
                "entrypoint": "plugin.py",
                "permissions": []
            }}"#
        ),
    )
    .expect("write plugin manifest");
}

fn assert_no_install_staging(plugins_dir: &std::path::Path) {
    let staging = std::fs::read_dir(plugins_dir)
        .expect("plugins directory")
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with(".plugin-install-") || name.contains(".install-")
        })
        .count();
    assert_eq!(staging, 0, "plugin install staging must be cleaned");
}

fn assert_no_removal_quarantine(plugins_dir: &std::path::Path) {
    let quarantines = std::fs::read_dir(plugins_dir)
        .expect("plugins directory")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".plugin-remove-")
        })
        .count();
    assert_eq!(quarantines, 0, "plugin removal quarantine must be cleaned");
}

#[test]
fn plugin_manifest_round_trips_json() {
    let manifest = PluginManifest::builtin(
        BUILTIN_MICRONPLUS_PLUGIN_ID,
        "MicronPlus Text UI",
        "Built-in MicronPlus transform",
    );

    let json = serde_json::to_string(&manifest).expect("serialize plugin manifest");
    let decoded: PluginManifest = serde_json::from_str(&json).expect("deserialize plugin manifest");

    assert_eq!(decoded, manifest);
}

#[test]
fn python_manifest_aliases_load_without_executing_plugin() {
    let json = r#"{
        "id": "browser-summary-plugin",
        "name": "Browser Summary Plugin",
        "version": "0.1.0",
        "author": "OMENbrowser",
        "description": "Demonstrates visible browser hooks.",
        "entrypoint": "plugin.py",
        "permissions": ["transform_content", "augment_request_data", "render_browser_rows"]
    }"#;

    let manifest: PluginManifest = serde_json::from_str(json).expect("manifest");
    assert_eq!(manifest.plugin_id, "browser-summary-plugin");
    assert_eq!(
        manifest.permissions,
        vec![
            PluginPermission::BrowserTransformContent,
            PluginPermission::BrowserEnrichRequestData,
            PluginPermission::BrowserRenderRows,
        ]
    );
}

#[test]
fn builtin_micronplus_facade_lowers_live_partial_descriptor() {
    let lowered = micronplus::lower_micronplus_markup(
        r#"[live id="sample_badge" src=":/page/status-card.mu" refresh=1 loop=7 fields="started_at=1|seed=1"]"#,
    );

    assert!(lowered.markup.contains("`{:/page/status-card.mu`1`"));
    assert!(lowered.markup.contains("started_at=1|seed=1"));
    assert!(lowered.markup.contains("pid=sample_badge"));
    assert!(lowered.markup.contains("loop=7"));
    assert_eq!(lowered.lives.len(), 1);
    assert_eq!(lowered.lives[0].id, "sample_badge");
    assert_eq!(lowered.lives[0].src, ":/page/status-card.mu");
    assert_eq!(lowered.lives[0].refresh_secs, Some(1));
    assert_eq!(lowered.lives[0].loop_count, Some(7));
}

#[test]
fn registry_discovers_builtin_and_installed_manifests() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-registry-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let plugin_dir = root.join("example");
    std::fs::create_dir_all(&plugin_dir).expect("plugin dir");
    std::fs::write(
        plugin_dir.join("plugin.json"),
        r#"{
            "id": "example-plugin",
            "name": "Example Plugin",
            "version": "0.1.0",
            "author": "OMENbrowser",
            "description": "Example",
            "entrypoint": "plugin.py",
            "permissions": ["transform_content"]
        }"#,
    )
    .expect("manifest");

    let registry = PluginRegistry::new(root.clone());
    let report = registry
        .discover(&[BUILTIN_MICRONPLUS_PLUGIN_ID.into()])
        .expect("discover");

    assert!(report.warnings.is_empty());
    assert_eq!(report.plugins.len(), 3);
    assert!(report.plugins[0].builtin);
    assert_eq!(
        report.plugins[0].manifest.plugin_id,
        BUILTIN_MICRONPLUS_PLUGIN_ID
    );
    assert!(report.plugins[0].enabled);
    assert_eq!(
        report.plugins[1].manifest.plugin_id,
        BUILTIN_OMENCHAT_PLUGIN_ID
    );
    assert!(!report.plugins[1].enabled);
    assert_eq!(report.plugins[2].manifest.plugin_id, "example-plugin");
    assert!(!report.plugins[2].enabled);
    assert!(!report.plugins[2].trusted);
    let registry_file = registry
        .load_registry()
        .expect("persisted discovery metadata");
    let discovered = registry_file
        .plugins
        .get("example-plugin")
        .expect("discovered registry entry");
    assert!(!discovered.enabled);
    assert!(!discovered.trusted);
}

#[test]
fn registry_discovery_bounds_installed_candidates_and_reports_overload() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-discovery-bound-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("plugin root");
    for index in 0..=PLUGIN_DISCOVERY_MAX_INSTALLED {
        let plugin_dir = root.join(format!("plugin-{index:03}"));
        std::fs::create_dir(&plugin_dir).expect("plugin directory");
        write_plugin_manifest(
            &plugin_dir.join("plugin.json"),
            &format!("bounded-plugin-{index:03}"),
        );
    }

    let report = PluginRegistry::new(root)
        .discover(&[])
        .expect("bounded discovery");

    assert_eq!(
        report
            .plugins
            .iter()
            .filter(|plugin| !plugin.builtin)
            .count(),
        PLUGIN_DISCOVERY_MAX_INSTALLED
    );
    assert!(report
        .warnings
        .iter()
        .any(|warning| warning.contains("safety limit")));
}

#[test]
fn registry_discovery_rejects_oversized_manifest_before_reading_it() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-manifest-bound-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let plugin_dir = root.join("oversized");
    std::fs::create_dir_all(&plugin_dir).expect("plugin directory");
    let manifest_path = plugin_dir.join("plugin.json");
    let manifest = std::fs::File::create(&manifest_path).expect("manifest");
    manifest
        .set_len(PLUGIN_MANIFEST_MAX_BYTES + 1)
        .expect("oversized manifest");

    let report = PluginRegistry::new(root)
        .discover(&[])
        .expect("discovery remains available");

    assert_eq!(
        report
            .plugins
            .iter()
            .filter(|plugin| !plugin.builtin)
            .count(),
        0
    );
    assert!(report
        .warnings
        .iter()
        .any(|warning| warning.contains("exceeds the") && warning.contains("byte limit")));
}

#[cfg(unix)]
#[test]
fn registry_discovery_does_not_follow_manifest_symlinks() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-manifest-symlink-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let plugin_dir = root.join("symlinked");
    std::fs::create_dir_all(&plugin_dir).expect("plugin directory");
    let outside = root.join("outside.json");
    write_plugin_manifest(&outside, "symlinked-plugin");
    symlink(&outside, plugin_dir.join("plugin.json")).expect("manifest symlink");

    let report = PluginRegistry::new(root)
        .discover(&[])
        .expect("discovery remains available");

    assert_eq!(
        report
            .plugins
            .iter()
            .filter(|plugin| !plugin.builtin)
            .count(),
        0
    );
    assert!(report
        .warnings
        .iter()
        .any(|warning| warning.contains("must be a regular file")));
}

#[test]
fn plugin_registry_file_is_read_with_a_hard_byte_cap() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-registry-bound-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("plugin root");
    let registry_path = root.join("registry.json");
    let registry_file = std::fs::File::create(&registry_path).expect("registry file");
    registry_file
        .set_len(PLUGIN_REGISTRY_MAX_BYTES + 1)
        .expect("oversized registry");

    let error = PluginRegistry::new(root)
        .load_registry()
        .expect_err("oversized registry must be rejected");
    assert!(error.to_string().contains("exceeds the"));
    assert!(error.to_string().contains("byte limit"));
}

#[test]
fn plugin_registry_save_replaces_valid_file_without_touching_legacy_temp_name() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-registry-atomic-save-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("plugin root");
    std::fs::write(root.join("registry.json"), b"{\"plugins\":{}}\n").expect("previous registry");
    let legacy_temp = root.join(format!("registry.json.tmp.{}", std::process::id()));
    std::fs::write(&legacy_temp, b"sentinel").expect("legacy temp sentinel");
    let registry = PluginRegistry::new(root.clone());
    let mut replacement = PluginRegistryFile::default();
    replacement.plugins.insert(
        "atomic-save".into(),
        omenbrowser_rs::plugins::PluginRegistryEntry {
            enabled: true,
            trusted: true,
            installed_path: "atomic-save".into(),
            source_path: None,
            installed_at_epoch_secs: 1,
        },
    );

    registry
        .save_registry(&replacement)
        .expect("atomic registry save");

    assert_eq!(registry.load_registry().expect("replacement"), replacement);
    assert_eq!(std::fs::read(&legacy_temp).expect("sentinel"), b"sentinel");
    assert_eq!(
        std::fs::read_dir(&root)
            .expect("plugin root")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".registry.json.")
            })
            .count(),
        0
    );
}

#[test]
fn plugin_registry_save_refuses_non_regular_target() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-registry-directory-target-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("registry.json")).expect("directory target");

    let error = PluginRegistry::new(root)
        .save_registry(&PluginRegistryFile::default())
        .expect_err("directory registry target must be refused");
    assert!(error.to_string().contains("must be a regular file"));
}

#[cfg(unix)]
#[test]
fn plugin_registry_save_and_load_refuse_symlink_target_without_touching_referent() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-registry-symlink-target-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("plugin root");
    let outside = root.join("outside.json");
    std::fs::write(&outside, b"outside").expect("outside fixture");
    symlink(&outside, root.join("registry.json")).expect("registry symlink");
    let registry = PluginRegistry::new(root);

    let load_error = registry
        .load_registry()
        .expect_err("registry load must refuse symlink");
    assert!(load_error.to_string().contains("must be a regular file"));
    let save_error = registry
        .save_registry(&PluginRegistryFile::default())
        .expect_err("registry save must refuse symlink");
    assert!(save_error.to_string().contains("must be a regular file"));
    assert_eq!(
        std::fs::read(outside).expect("outside preserved"),
        b"outside"
    );
}

#[cfg(unix)]
#[test]
fn newly_created_plugin_registry_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-registry-mode-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let registry = PluginRegistry::new(root.clone());
    registry
        .save_registry(&PluginRegistryFile::default())
        .expect("registry save");

    let mode = std::fs::metadata(root.join("registry.json"))
        .expect("registry metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn registry_skips_duplicate_legacy_micronplus_manifest() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-duplicate-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let plugin_dir = root.join(BUILTIN_MICRONPLUS_PLUGIN_ID);
    std::fs::create_dir_all(&plugin_dir).expect("plugin dir");
    std::fs::write(
        plugin_dir.join("plugin.json"),
        r#"{
            "id": "micronplus-textui",
            "name": "Legacy MicronPlus",
            "version": "0.1.0",
            "author": "OMENbrowser",
            "description": "Legacy",
            "entrypoint": "plugin.py",
            "permissions": ["transform_content"]
        }"#,
    )
    .expect("manifest");

    let registry = PluginRegistry::new(root);
    let report = registry
        .discover(&[BUILTIN_MICRONPLUS_PLUGIN_ID.into()])
        .expect("discover");

    assert_eq!(report.plugins.len(), 2);
    assert!(report
        .plugins
        .iter()
        .any(|plugin| plugin.manifest.plugin_id == BUILTIN_OMENCHAT_PLUGIN_ID));
    assert!(report.warnings[0].contains("duplicate plugin"));
}

#[test]
fn registry_persists_installed_manifest_metadata_without_execution() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-install-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let source = root.join("source");
    std::fs::create_dir_all(&source).expect("source dir");
    std::fs::write(
        source.join("plugin.json"),
        r#"{
            "id": "installed-plugin",
            "name": "Installed Plugin",
            "version": "0.1.0",
            "author": "OMENbrowser",
            "description": "Installed",
            "entrypoint": "plugin.py",
            "permissions": ["transform_content"]
        }"#,
    )
    .expect("manifest");
    std::fs::write(
        source.join("plugin.py"),
        "raise RuntimeError('must not run')\n",
    )
    .expect("entrypoint");

    let registry = PluginRegistry::new(root.join("plugins"));
    let blocked = registry
        .install_from_folder(&source, false)
        .expect_err("unsafe install requires confirmation");
    assert!(blocked.to_string().contains("Third-party plugins"));

    let installed = registry
        .install_from_folder(&source, true)
        .expect("install confirmed");
    assert_eq!(installed.manifest.plugin_id, "installed-plugin");
    assert!(installed.trusted);
    assert!(installed.path.as_ref().unwrap().join("plugin.py").exists());

    let registry_file = registry.load_registry().expect("load registry");
    let entry = registry_file
        .plugins
        .get("installed-plugin")
        .expect("registry entry");
    assert!(entry.enabled);
    assert!(entry.trusted);
    assert!(entry.source_path.as_deref().unwrap().contains("source"));
    assert_no_install_staging(&root.join("plugins"));
}

#[test]
fn registry_install_rejects_oversized_file_and_removes_staging() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-install-file-bound-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let source = root.join("source");
    std::fs::create_dir_all(&source).expect("source directory");
    write_plugin_manifest(&source.join("plugin.json"), "oversized-install");
    let oversized = std::fs::File::create(source.join("oversized.bin")).expect("fixture");
    oversized
        .set_len(PLUGIN_INSTALL_MAX_FILE_BYTES + 1)
        .expect("sparse oversize fixture");
    let plugins_dir = root.join("plugins");

    let error = PluginRegistry::new(plugins_dir.clone())
        .install_from_folder(&source, true)
        .expect_err("oversized plugin file must be rejected");

    assert!(error.to_string().contains("plugin file exceeds"));
    assert!(!plugins_dir.join("oversized-install").exists());
    assert_no_install_staging(&plugins_dir);
}

#[test]
fn registry_install_rejects_entry_overload_and_removes_staging() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-install-entry-bound-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let source = root.join("source");
    std::fs::create_dir_all(&source).expect("source directory");
    write_plugin_manifest(&source.join("plugin.json"), "entry-overload-install");
    for index in 0..PLUGIN_INSTALL_MAX_ENTRIES {
        std::fs::create_dir(source.join(format!("empty-{index:04}")))
            .expect("empty plugin directory");
    }
    let plugins_dir = root.join("plugins");

    let error = PluginRegistry::new(plugins_dir.clone())
        .install_from_folder(&source, true)
        .expect_err("entry overload must be rejected");

    assert!(error.to_string().contains("entry limit"));
    assert!(!plugins_dir.join("entry-overload-install").exists());
    assert_no_install_staging(&plugins_dir);
}

#[test]
fn registry_install_rejects_excessive_depth_and_removes_staging() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-install-depth-bound-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let source = root.join("source");
    std::fs::create_dir_all(&source).expect("source directory");
    write_plugin_manifest(&source.join("plugin.json"), "deep-install");
    let mut nested = source.clone();
    for index in 0..=PLUGIN_INSTALL_MAX_DEPTH {
        nested = nested.join(format!("level-{index:02}"));
        std::fs::create_dir(&nested).expect("nested plugin directory");
    }
    let plugins_dir = root.join("plugins");

    let error = PluginRegistry::new(plugins_dir.clone())
        .install_from_folder(&source, true)
        .expect_err("excessive depth must be rejected");

    assert!(error.to_string().contains("directory depth limit"));
    assert!(!plugins_dir.join("deep-install").exists());
    assert_no_install_staging(&plugins_dir);
}

#[cfg(unix)]
#[test]
fn registry_install_rejects_symlinked_tree_entry_and_removes_staging() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-install-symlink-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let source = root.join("source");
    std::fs::create_dir_all(&source).expect("source directory");
    write_plugin_manifest(&source.join("plugin.json"), "symlink-install");
    let outside = root.join("outside.txt");
    std::fs::write(&outside, b"outside").expect("outside fixture");
    symlink(&outside, source.join("linked.txt")).expect("plugin symlink");
    let plugins_dir = root.join("plugins");

    let error = PluginRegistry::new(plugins_dir.clone())
        .install_from_folder(&source, true)
        .expect_err("plugin tree symlink must be rejected");

    assert!(error
        .to_string()
        .contains("refuses symlink or special entry"));
    assert!(!plugins_dir.join("symlink-install").exists());
    assert_no_install_staging(&plugins_dir);
}

#[cfg(unix)]
#[test]
fn registry_install_refuses_broken_destination_symlink() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-install-broken-target-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let source = root.join("source");
    let plugins_dir = root.join("plugins");
    std::fs::create_dir_all(&source).expect("source directory");
    std::fs::create_dir_all(&plugins_dir).expect("plugins directory");
    write_plugin_manifest(&source.join("plugin.json"), "broken-target");
    let target = plugins_dir.join("broken-target");
    symlink(root.join("missing-target"), &target).expect("broken destination symlink");

    let error = PluginRegistry::new(plugins_dir.clone())
        .install_from_folder(&source, true)
        .expect_err("broken destination symlink must not be replaced");

    assert!(error.to_string().contains("plugin already exists"));
    assert!(std::fs::symlink_metadata(&target)
        .expect("preserved destination symlink")
        .file_type()
        .is_symlink());
    assert_no_install_staging(&plugins_dir);
}

#[test]
fn registry_remove_refuses_builtin_and_deletes_installed_plugin() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-remove-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let source = root.join("source");
    std::fs::create_dir_all(&source).expect("source dir");
    std::fs::write(
        source.join("plugin.json"),
        r#"{
            "id": "remove-me",
            "name": "Remove Me",
            "version": "0.1.0",
            "author": "OMENbrowser",
            "description": "Remove",
            "entrypoint": "plugin.py",
            "permissions": []
        }"#,
    )
    .expect("manifest");

    let registry = PluginRegistry::new(root.join("plugins"));
    registry
        .install_from_folder(&source, true)
        .expect("install confirmed");
    assert!(registry.remove_installed("remove-me").expect("remove"));
    assert!(!root.join("plugins").join("remove-me").exists());
    assert!(!registry
        .load_registry()
        .expect("registry")
        .plugins
        .contains_key("remove-me"));
    assert_no_removal_quarantine(&root.join("plugins"));

    let builtin = registry
        .remove_installed(BUILTIN_MICRONPLUS_PLUGIN_ID)
        .expect_err("builtin cannot be removed");
    assert!(builtin
        .to_string()
        .contains("built-in plugins cannot be removed"));
    let omenchat_builtin = registry
        .remove_installed(BUILTIN_OMENCHAT_PLUGIN_ID)
        .expect_err("OMENchat builtin cannot be removed");
    assert!(omenchat_builtin
        .to_string()
        .contains("built-in plugins cannot be removed"));
}

#[cfg(unix)]
#[test]
fn registry_remove_refuses_symlink_target_without_touching_referent() {
    use std::os::unix::fs::symlink;

    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-remove-symlink-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let plugins = root.join("plugins");
    let outside = root.join("outside");
    std::fs::create_dir_all(&plugins).expect("plugins directory");
    std::fs::create_dir_all(&outside).expect("outside directory");
    std::fs::write(outside.join("keep.txt"), b"keep").expect("outside fixture");
    symlink(&outside, plugins.join("linked-plugin")).expect("plugin symlink");

    let error = PluginRegistry::new(plugins)
        .remove_installed("linked-plugin")
        .expect_err("symlink removal target must be refused");

    assert!(error.to_string().contains("must be a regular directory"));
    assert_eq!(
        std::fs::read(outside.join("keep.txt")).expect("outside preserved"),
        b"keep"
    );
}

#[test]
fn registry_syncs_enabled_metadata_from_settings_ids() {
    let root =
        std::env::temp_dir().join(format!("omenbrowser-rs-plugin-sync-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let source = root.join("source");
    std::fs::create_dir_all(&source).expect("source dir");
    std::fs::write(
        source.join("plugin.json"),
        r#"{
            "id": "sync-me",
            "name": "Sync Me",
            "version": "0.1.0",
            "author": "OMENbrowser",
            "description": "Sync",
            "entrypoint": "plugin.py",
            "permissions": []
        }"#,
    )
    .expect("manifest");

    let registry = PluginRegistry::new(root.join("plugins"));
    registry
        .install_from_folder(&source, true)
        .expect("install");
    registry.sync_enabled_metadata(&[]).expect("sync disabled");
    assert!(
        !registry
            .load_registry()
            .expect("registry")
            .plugins
            .get("sync-me")
            .expect("entry")
            .enabled
    );

    registry
        .sync_enabled_metadata(&["sync-me".into()])
        .expect("sync enabled");
    assert!(
        registry
            .load_registry()
            .expect("registry")
            .plugins
            .get("sync-me")
            .expect("entry")
            .enabled
    );
}

#[test]
fn registry_rejects_unsafe_plugin_ids() {
    let root = std::env::temp_dir().join(format!(
        "omenbrowser-rs-plugin-unsafe-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let source = root.join("source");
    std::fs::create_dir_all(&source).expect("source dir");
    std::fs::write(
        source.join("plugin.json"),
        r#"{
            "id": "../bad",
            "name": "Bad",
            "version": "0.1.0",
            "author": "OMENbrowser",
            "description": "Bad",
            "entrypoint": "plugin.py",
            "permissions": []
        }"#,
    )
    .expect("manifest");

    let registry = PluginRegistry::new(root.join("plugins"));
    let error = registry
        .install_from_folder(&source, true)
        .expect_err("unsafe id rejected");
    assert!(error.to_string().contains("unsafe plugin id"));
}
