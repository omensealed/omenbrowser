use omenbrowser_rs::plugins::{
    micronplus, PluginManifest, PluginPermission, PluginRegistry, BUILTIN_MICRONPLUS_PLUGIN_ID,
    BUILTIN_OMENCHAT_PLUGIN_ID,
};

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

    let builtin = registry
        .remove_installed(BUILTIN_MICRONPLUS_PLUGIN_ID)
        .expect_err("builtin cannot be removed");
    assert!(builtin
        .to_string()
        .contains("built-in plugins cannot be removed"));
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
