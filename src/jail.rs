use std::collections::HashMap;
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum JailError {
    #[error("path escapes repo root: {0}")]
    Escapes(PathBuf),
    #[error("path is absolute, expected relative to repo root: {0}")]
    Absolute(PathBuf),
    #[error("path not inside repo root: {0}")]
    Outside(PathBuf),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// resolve a user-supplied path against repo_root, rejecting escapes.
/// rules:
/// - relative paths joined to repo_root then canonicalised
/// - absolute paths rejected
/// - `..` segments rejected before canonicalisation
/// - final canonical path must start with repo_root
pub fn resolve(repo_root: &Path, requested: &str) -> Result<PathBuf, JailError> {
    if Path::new(requested).is_absolute() {
        return Err(JailError::Absolute(PathBuf::from(requested)));
    }
    if requested.split('/').any(|seg| seg == "..") {
        return Err(JailError::Escapes(PathBuf::from(requested)));
    }
    let joined = repo_root.join(requested);
    let canon = joined.canonicalize().unwrap_or(joined.clone());
    let root_canon = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());
    if !canon.starts_with(&root_canon) {
        return Err(JailError::Outside(canon));
    }
    Ok(canon)
}

/// metadata needed by tools to enforce the jail.
#[derive(Clone, Debug)]
pub struct Jail {
    pub root: PathBuf,
}

impl Jail {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn resolve(&self, requested: &str) -> Result<PathBuf, JailError> {
        resolve(&self.root, requested)
    }
}

/// a recorded attempt to escape the jail, for tests/auditing.
#[derive(Clone, Debug)]
pub struct EscapeAttempt {
    pub requested: String,
    pub reason: String,
}

pub type EscapeLog = HashMap<String, String>;