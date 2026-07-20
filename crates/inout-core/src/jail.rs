//! repo-root jail for all filesystem-touching tools.

use std::path::{Path, PathBuf};

use thiserror::Error;

/// failure modes when validating a requested path.
#[derive(Debug, Error)]
pub enum JailError {
    /// path escapes repo root.
    #[error("path escapes repo root: {0}")]
    Escapes(PathBuf),
    /// absolute path supplied when a relative repo path was required.
    #[error("path is absolute, expected relative to repo root: {0}")]
    Absolute(PathBuf),
    /// resolved canonical path falls outside repo root.
    #[error("path not inside repo root: {0}")]
    Outside(PathBuf),
    /// i/o error during resolution.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// resolve a user-supplied path against `repo_root`, rejecting escapes.
///
/// rules:
///
/// - relative paths joined to `repo_root` then canonicalised.
/// - absolute paths rejected.
/// - `..` segments rejected before canonicalisation.
/// - final canonical path must start with `repo_root`.
pub fn resolve(repo_root: &Path, requested: &str) -> Result<PathBuf, JailError> {
    let path = Path::new(requested);
    if path.is_absolute() {
        return Err(JailError::Absolute(PathBuf::from(requested)));
    }
    if requested.split('/').any(|seg| seg == "..") {
        return Err(JailError::Escapes(PathBuf::from(requested)));
    }
    let joined = repo_root.join(requested);
    let canon = joined.canonicalize().unwrap_or_else(|_| joined.clone());
    let root_canon = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());
    if !canon.starts_with(&root_canon) {
        return Err(JailError::Outside(canon));
    }
    Ok(canon)
}

/// metadata needed by tools to enforce the jail.
#[derive(Clone, Debug)]
pub struct Jail {
    /// canonical root of the working directory.
    pub root: PathBuf,
}

impl Jail {
    /// create a jail rooted at `root`.
    pub const fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// resolve `requested` against this jail's root.
    pub fn resolve(&self, requested: &str) -> Result<PathBuf, JailError> {
        resolve(&self.root, requested)
    }
}
