//! Safe preprocessing for browser CLI passphrase sources.

use anyhow::Context;
use std::io::Read;
use std::path::Path;

const MAX_PASSPHRASE_BYTES: usize = 4096;

/// Resolve safe passphrase sources into the compatibility parser's internal
/// `--passphrase <value>` form.
pub fn resolve_passphrase_args(args: Vec<String>) -> anyhow::Result<Vec<String>> {
    let mut resolved = Vec::with_capacity(args.len());
    let mut args = args.into_iter();
    let mut source_seen = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--passphrase" => {
                ensure_single_passphrase_source(&mut source_seen)?;
                let value = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--passphrase requires a value"))?;
                eprintln!(
                    "warning: --passphrase exposes secrets in process listings; use --passphrase-file, --passphrase-stdin, or --passphrase-prompt"
                );
                resolved.extend(["--passphrase".into(), validate_passphrase(value)?]);
            }
            "--passphrase-file" => {
                ensure_single_passphrase_source(&mut source_seen)?;
                let path = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--passphrase-file requires a path"))?;
                let value = read_passphrase_file(Path::new(&path))?;
                resolved.extend(["--passphrase".into(), value]);
            }
            "--passphrase-stdin" => {
                ensure_single_passphrase_source(&mut source_seen)?;
                let value = read_passphrase_from_reader(std::io::stdin().lock())?;
                resolved.extend(["--passphrase".into(), value]);
            }
            "--passphrase-prompt" => {
                ensure_single_passphrase_source(&mut source_seen)?;
                let value = rpassword::prompt_password("IFAC passphrase: ")
                    .context("failed to read hidden IFAC passphrase from terminal")?;
                resolved.extend(["--passphrase".into(), validate_passphrase(value)?]);
            }
            _ => resolved.push(arg),
        }
    }
    Ok(resolved)
}

fn ensure_single_passphrase_source(seen: &mut bool) -> anyhow::Result<()> {
    if std::mem::replace(seen, true) {
        anyhow::bail!("choose exactly one passphrase source");
    }
    Ok(())
}

fn read_passphrase_file(path: &Path) -> anyhow::Result<String> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect passphrase file {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        anyhow::bail!("passphrase file must be a regular non-symlink file");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            anyhow::bail!("passphrase file permissions must not allow group or other access");
        }
    }
    read_passphrase_from_reader(std::fs::File::open(path)?)
}

fn read_passphrase_from_reader(reader: impl Read) -> anyhow::Result<String> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_PASSPHRASE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_PASSPHRASE_BYTES {
        anyhow::bail!("passphrase input exceeds 4096 bytes");
    }
    let value = String::from_utf8(bytes).context("passphrase input is not valid UTF-8")?;
    validate_passphrase(value.trim_end_matches(['\r', '\n']).to_owned())
}

fn validate_passphrase(value: String) -> anyhow::Result<String> {
    if value.is_empty() {
        anyhow::bail!("passphrase must not be empty");
    }
    if value.contains('\0') {
        anyhow::bail!("passphrase must not contain NUL bytes");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct IsolatedRoot(PathBuf);

    impl IsolatedRoot {
        fn new(label: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "omenbrowser-cli-secret-{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("system clock")
                    .as_nanos()
            ));
            std::fs::create_dir(&root).expect("create isolated root");
            Self(root)
        }
    }

    impl Drop for IsolatedRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn reader_preserves_spaces_trims_only_line_endings_and_enforces_bounds() {
        assert_eq!(
            read_passphrase_from_reader(std::io::Cursor::new(b"  secret value  \r\n"))
                .expect("passphrase"),
            "  secret value  "
        );
        assert_eq!(
            read_passphrase_from_reader(std::io::Cursor::new(vec![b'x'; 4096]))
                .expect("boundary passphrase")
                .len(),
            4096
        );
        assert_eq!(
            read_passphrase_from_reader(std::io::Cursor::new(vec![b'x'; 4097]))
                .expect_err("oversized passphrase")
                .to_string(),
            "passphrase input exceeds 4096 bytes"
        );
        assert_eq!(
            read_passphrase_from_reader(std::io::Cursor::new(b"\n"))
                .expect_err("empty passphrase")
                .to_string(),
            "passphrase must not be empty"
        );
        assert_eq!(
            read_passphrase_from_reader(std::io::Cursor::new(b"bad\0value"))
                .expect_err("NUL passphrase")
                .to_string(),
            "passphrase must not contain NUL bytes"
        );
        assert_eq!(
            read_passphrase_from_reader(std::io::Cursor::new([0xff]))
                .expect_err("invalid UTF-8")
                .to_string(),
            "passphrase input is not valid UTF-8"
        );
    }

    #[test]
    fn owner_only_regular_file_resolves_and_sources_are_exclusive() {
        let root = IsolatedRoot::new("file");
        let path = root.0.join("passphrase");
        std::fs::write(&path, b"file-secret\n").expect("write passphrase");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .expect("permissions");
        }
        let resolved =
            resolve_passphrase_args(vec!["--passphrase-file".into(), path.display().to_string()])
                .expect("resolve file");
        assert_eq!(resolved, vec!["--passphrase", "file-secret"]);
        assert_eq!(
            resolve_passphrase_args(vec![
                "--passphrase".into(),
                "one".into(),
                "--passphrase-file".into(),
                path.display().to_string(),
            ])
            .expect_err("exclusive sources")
            .to_string(),
            "choose exactly one passphrase source"
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_source_rejects_permissive_modes_and_symlinks() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let root = IsolatedRoot::new("unsafe-file");
        let path = root.0.join("passphrase");
        std::fs::write(&path, b"file-secret\n").expect("write passphrase");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("permissions");
        assert_eq!(
            read_passphrase_file(&path)
                .expect_err("permissive mode")
                .to_string(),
            "passphrase file permissions must not allow group or other access"
        );

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .expect("permissions");
        let link = root.0.join("passphrase-link");
        symlink(&path, &link).expect("symlink");
        assert_eq!(
            read_passphrase_file(&link)
                .expect_err("symlink source")
                .to_string(),
            "passphrase file must be a regular non-symlink file"
        );
    }

    #[test]
    fn resolver_preserves_unrelated_arguments_and_missing_value_errors() {
        let args = vec!["--desktop".into(), "--app-root".into(), "/tmp/test".into()];
        assert_eq!(
            resolve_passphrase_args(args.clone()).expect("pass through"),
            args
        );
        assert_eq!(
            resolve_passphrase_args(vec!["--passphrase-file".into()])
                .expect_err("missing path")
                .to_string(),
            "--passphrase-file requires a path"
        );
        assert_eq!(
            resolve_passphrase_args(vec!["--passphrase".into()])
                .expect_err("missing value")
                .to_string(),
            "--passphrase requires a value"
        );
    }
}
