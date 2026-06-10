# 08 — Storage, Settings, and Identity

## Storage goal

OMENbrowser_rs must use safe, predictable app data directories and must never silently destroy identity or settings data.

## Python source

Reference files:

```text
src/omenbrowser/core/app_data.py
src/omenbrowser/core/settings.py
src/omenbrowser/core/identity.py
src/omenbrowser/core/files.py
```

## App paths

Use platform-specific app directories through a Rust crate such as `directories`.

Recommended layout:

```text
OMENbrowser_rs data dir/
  settings.json
  identities/
    default_identity
    backups/
  reticulum/
    config
    storage/
  messages/
  directory.json
  cache/
  downloads/
  plugins/
  logs/
  diagnostics/
  interfaces.json
  gateways.json
```

Keep paths centralized in `AppPaths`.

## AppPaths struct

```rust
pub struct AppPaths {
    pub root: PathBuf,
    pub settings_file: PathBuf,
    pub identities_dir: PathBuf,
    pub identity_backups_dir: PathBuf,
    pub reticulum_config_dir: PathBuf,
    pub reticulum_storage_dir: PathBuf,
    pub messages_dir: PathBuf,
    pub directory_file: PathBuf,
    pub cache_dir: PathBuf,
    pub downloads_dir: PathBuf,
    pub plugins_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub diagnostics_dir: PathBuf,
    pub interfaces_file: PathBuf,
    pub gateways_file: PathBuf,
}
```

`ensure()` must create all required directories.

## Settings

Port Python defaults.

Recommended settings fields:

```rust
pub struct AppSettings {
    pub theme: String,
    pub active_identity_path: Option<PathBuf>,
    pub active_identity_label: Option<String>,
    pub runtime_backend: RuntimeBackendSetting,
    pub reticulum_instance_mode: ReticulumInstanceMode,
    pub announce_on_start: bool,
    pub periodic_lxmf_sync: bool,
    pub lxmf_sync_interval: u64,
    pub lxmf_sync_limit: u32,
    pub preferred_propagation_node_hash: Option<String>,
    pub browser_home_url: Option<String>,
    pub plugin_remote_gate: bool,
}
```

If Python has additional settings discovered during porting, add them here and update this doc.

## Settings load behavior

Rules:

1. If settings file does not exist, return defaults.
2. If settings file exists and parses, merge with defaults for missing fields.
3. If settings file exists but is corrupted, copy it to a timestamped backup and return defaults.
4. Save should write atomically: write temp file, fsync if practical, rename.
5. Do not delete unknown future fields unless intentionally migrating.

## Identity workflows

Required workflows:

### Create managed identity

- Create identity under app-managed identities dir.
- Do not overwrite existing identity without backup.
- If the default managed identity already exists, create a new timestamped managed identity file instead of replacing the active one.
- Creating a new identity must never delete or overwrite the current active identity or any saved managed identity.
- Compute display hash from identity bytes or runtime identity hash if available.
- Save active identity path/label in settings.

### Attach existing identity

- User chooses a path.
- App references that path without copying.
- Mark profile `managed = false`.
- Save active identity path/label in settings.

### Import identity copy

- User chooses source path.
- App copies into managed identities dir.
- Existing target is backed up first.
- Mark profile `managed = true`.
- Save active identity path/label in settings.

### Export backup

- Copy active identity to user-selected backup directory or app backup dir.
- Include timestamp in filename.
- Never expose private bytes in logs.

### Desktop identity management

- The desktop UI exposes a separate Identities section for managed identity selection.
- The active identity label can be renamed without rewriting private identity bytes.
- Managed identities can be activated from the app identities directory.
- Deleting the active identity first creates a timestamped backup, then clears the active settings reference.
- Desktop deletion requires an explicit confirmation step after clicking `Delete Active`.
- `Announce Now` asks the runtime to announce the active local LXMF identity; it must fail visibly if no runtime identity is attached.
- External identity paths and custom Reticulum config directories remain editable in Settings until a file picker/import flow is added.
- Active identities receive a deterministic storage root under `identity_storage/`.
- Message history, attachments, browser cache, browser form state, directory state, and managed Reticulum storage are scoped under the active identity storage root.
- Global settings, logs, plugins, downloads, interface profile settings, and identity files remain app-level so identities can be selected and managed from one place.
- The Identities workspace preview must show both the private identity file path and identity-owned storage paths for each managed identity.
- On the first scoped-identity startup after the identity-storage migration, app-level message history, attachments, cache, directory state, browser form state, and managed Reticulum config/storage are copied into the active identity root if the target files do not already exist.
- This adoption writes an `identity_storage/.app_level_storage_adopted` marker so old app-level data is not copied again into later identities.
- Existing target files are never overwritten during adoption.

## Hash display

Python hashes identity file bytes for profile display. Rust can preserve that behavior initially. If the runtime adapter supplies an official Reticulum identity hash, prefer showing both if useful:

```text
identity file hash: abcd1234...
runtime destination hash: 1234abcd...
```

Avoid confusing them in UI labels.

## File safety

`next_available_download_path` behavior from Python should be preserved:

- sanitize or normalize filename;
- if target exists, append incrementing suffix;
- never overwrite without explicit user action.

## Migration

If importing data from Python OMENbrowser later, write a separate migration tool. Do not silently mutate Python app data. The Rust app can offer:

- locate Python OMENbrowser data;
- copy settings/messages/directory/cache selectively;
- keep source untouched;
- report what was migrated.
