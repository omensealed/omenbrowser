use std::io;
use std::path::{Component, Path, PathBuf};

use crate::error::{ServerError, ServerResult};

#[derive(Clone, Debug)]
pub(crate) struct PrivatePathPolicy {
    lexical_root: PathBuf,
    canonical_root: PathBuf,
    current_dir: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PathClass {
    Managed(PathBuf),
    External(PathBuf),
}

impl PrivatePathPolicy {
    pub(crate) fn establish(selected_root: &Path) -> ServerResult<Self> {
        let current_dir = std::env::current_dir()?;
        let lexical_root = clean_absolute(selected_root, &current_dir)?;
        let canonical_root = establish_selected_root(&lexical_root)?;
        Ok(Self {
            lexical_root,
            canonical_root,
            current_dir,
        })
    }

    pub(crate) fn validate_sensitive_file(&self, path: &Path) -> ServerResult<()> {
        let clean = clean_absolute(path, &self.current_dir)?;
        if clean.file_name().is_none() {
            return Err(policy_error(
                "configured private-state file has no filename",
            ));
        }
        let _ = self.classify_clean(clean)?;
        Ok(())
    }

    pub(crate) fn ensure_sensitive_parent(&self, path: &Path) -> ServerResult<()> {
        let clean = clean_absolute(path, &self.current_dir)?;
        let parent = clean
            .parent()
            .ok_or_else(|| policy_error("configured private-state file has no parent"))?;
        match self.classify_clean(parent.to_path_buf())? {
            PathClass::Managed(relative) => self.ensure_managed_directory(&relative),
            PathClass::External(parent) => crate::private_fs::ensure_private_parent_dir(&parent)
                .map_err(|_| {
                    policy_error("configured private-state parent is not a real directory")
                }),
        }
    }

    pub(crate) fn ensure_service_directory(&self, path: &Path) -> ServerResult<()> {
        let clean = clean_absolute(path, &self.current_dir)?;
        match self.classify_clean(clean)? {
            PathClass::Managed(relative) => self.ensure_managed_directory(&relative),
            PathClass::External(path) => {
                crate::private_fs::ensure_private_parent_dir(&path).map_err(|_| {
                    policy_error("configured private-state directory is not safely reachable")
                })?;
                crate::private_fs::ensure_private_dir(&path)?;
                Ok(())
            }
        }
    }

    pub(crate) fn ensure_managed_path(&self, path: &Path) -> ServerResult<()> {
        let clean = clean_absolute(path, &self.current_dir)?;
        match self.classify_clean(clean)? {
            PathClass::Managed(relative) => self.ensure_managed_directory(&relative),
            PathClass::External(_) => Err(policy_error(
                "expected product-managed path is outside the selected server root",
            )),
        }
    }

    fn classify_clean(&self, clean: PathBuf) -> ServerResult<PathClass> {
        match clean.strip_prefix(&self.lexical_root) {
            Ok(relative) => {
                validate_relative_suffix(relative)?;
                Ok(PathClass::Managed(relative.to_path_buf()))
            }
            Err(_) => Ok(PathClass::External(clean)),
        }
    }

    fn ensure_managed_directory(&self, relative: &Path) -> ServerResult<()> {
        validate_relative_suffix(relative)?;
        let mut current = self.canonical_root.clone();
        for component in relative.components() {
            let Component::Normal(name) = component else {
                return Err(policy_error("managed path contains an unsafe component"));
            };
            current.push(name);
            match std::fs::symlink_metadata(&current) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() || !metadata.is_dir() {
                        return Err(policy_error(
                            "managed path component is not a real directory",
                        ));
                    }
                    crate::private_fs::ensure_private_dir(&current)?;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    crate::private_fs::ensure_private_parent_dir(&current)?;
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}

fn clean_absolute(path: &Path, current_dir: &Path) -> ServerResult<PathBuf> {
    reject_parent_components(path)?;
    let combined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        if path
            .components()
            .any(|component| matches!(component, Component::Prefix(_)))
        {
            return Err(policy_error(
                "ambiguous drive-relative private-state path is not supported",
            ));
        }
        current_dir.join(path)
    };
    let mut clean = PathBuf::new();
    for component in combined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(policy_error(
                    "private-state path must not contain parent traversal",
                ))
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                clean.push(component.as_os_str());
            }
        }
    }
    if !clean.is_absolute() {
        return Err(policy_error(
            "private-state path could not be resolved absolutely",
        ));
    }
    Ok(clean)
}

fn reject_parent_components(path: &Path) -> ServerResult<()> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(policy_error(
            "private-state path must not contain parent traversal",
        ));
    }
    Ok(())
}

fn validate_relative_suffix(path: &Path) -> ServerResult<()> {
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(policy_error("managed path contains an unsafe component"));
    }
    Ok(())
}

fn establish_selected_root(lexical_root: &Path) -> ServerResult<PathBuf> {
    match std::fs::symlink_metadata(lexical_root) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(policy_error("selected server root is not a real directory"));
            }
            crate::private_fs::ensure_private_dir(lexical_root)?;
            return Ok(std::fs::canonicalize(lexical_root)?);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }

    let mut ancestor = lexical_root.to_path_buf();
    let mut missing = Vec::new();
    loop {
        match std::fs::symlink_metadata(&ancestor) {
            Ok(metadata) => {
                let resolved = if metadata.file_type().is_symlink() {
                    std::fs::canonicalize(&ancestor)?
                } else {
                    if !metadata.is_dir() {
                        return Err(policy_error(
                            "selected server root ancestor is not a directory",
                        ));
                    }
                    std::fs::canonicalize(&ancestor)?
                };
                if !std::fs::metadata(&resolved)?.is_dir() {
                    return Err(policy_error(
                        "selected server root ancestor is not a directory",
                    ));
                }
                ancestor = resolved;
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let name = ancestor
                    .file_name()
                    .ok_or_else(|| policy_error("selected server root has no existing ancestor"))?
                    .to_os_string();
                missing.push(name);
                if !ancestor.pop() {
                    return Err(policy_error(
                        "selected server root has no existing ancestor",
                    ));
                }
            }
            Err(error) => return Err(error.into()),
        }
    }

    for name in missing.into_iter().rev() {
        ancestor.push(name);
        crate::private_fs::ensure_private_parent_dir(&ancestor)?;
    }
    crate::private_fs::ensure_private_dir(&ancestor)?;
    Ok(std::fs::canonicalize(ancestor)?)
}

fn policy_error(message: &'static str) -> ServerError {
    ServerError::Message(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "omenchatd-path-policy-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn selected_root_and_managed_descendants_are_created_component_by_component() {
        let parent = root("managed-create");
        std::fs::create_dir_all(&parent).expect("parent");
        let selected = parent.join("home");
        let policy = PrivatePathPolicy::establish(&selected).expect("policy");
        policy
            .ensure_managed_path(&selected.join("one").join("two"))
            .expect("managed descendants");
        assert!(selected.join("one/two").is_dir());
        std::fs::remove_dir_all(parent).expect("cleanup");
    }

    #[test]
    fn traversal_is_rejected_before_classification() {
        let parent = root("traversal");
        let selected = parent.join("home");
        std::fs::create_dir_all(&selected).expect("selected");
        let policy = PrivatePathPolicy::establish(&selected).expect("policy");
        assert!(policy
            .validate_sensitive_file(&selected.join("../outside/state"))
            .is_err());
        std::fs::remove_dir_all(parent).expect("cleanup");
    }

    #[test]
    fn clean_relative_selected_root_retains_managed_classification() {
        let relative = PathBuf::from("target").join(format!(
            "omenchatd-relative-policy-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let absolute = std::env::current_dir().expect("cwd").join(&relative);
        let policy = PrivatePathPolicy::establish(&relative).expect("policy");
        policy
            .ensure_sensitive_parent(&relative.join("nested/state"))
            .expect("relative managed parent");
        assert!(absolute.join("nested").is_dir());
        std::fs::remove_dir_all(absolute).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn final_selected_root_symlink_is_rejected() {
        use std::os::unix::fs::symlink;

        let parent = root("selected-root-symlink");
        let target = parent.join("target");
        let selected = parent.join("home");
        std::fs::create_dir_all(&target).expect("target");
        symlink(&target, &selected).expect("selected root symlink");
        assert!(PrivatePathPolicy::establish(&selected).is_err());
        std::fs::remove_dir_all(parent).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn real_root_below_operator_controlled_ancestor_symlink_is_supported() {
        use std::os::unix::fs::symlink;

        let parent = root("ancestor-symlink");
        let target = parent.join("target");
        let alias = parent.join("alias");
        std::fs::create_dir_all(&target).expect("target");
        symlink(&target, &alias).expect("ancestor symlink");
        let selected = alias.join("home");
        let policy = PrivatePathPolicy::establish(&selected).expect("policy");
        policy
            .ensure_managed_path(&selected.join("nested"))
            .expect("managed descendant");
        assert!(target.join("home/nested").is_dir());
        std::fs::remove_dir_all(parent).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn managed_component_symlink_is_rejected() {
        use std::os::unix::fs::symlink;

        let parent = root("managed-symlink");
        let selected = parent.join("home");
        let outside = parent.join("outside");
        std::fs::create_dir_all(&selected).expect("selected");
        std::fs::create_dir_all(&outside).expect("outside");
        symlink(&outside, selected.join("redirect")).expect("redirect");
        let policy = PrivatePathPolicy::establish(&selected).expect("policy");
        assert!(policy
            .ensure_managed_path(&selected.join("redirect/nested"))
            .is_err());
        assert!(!outside.join("nested").exists());
        std::fs::remove_dir_all(parent).expect("cleanup");
    }
}
