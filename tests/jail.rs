use std::path::PathBuf;
use tempfile::TempDir;
use twobobs::jail::{resolve, JailError};

#[test]
fn resolve_relative_path_inside_repo() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    std::fs::write(repo.join("a.txt"), "x").unwrap();
    let resolved = resolve(repo, "a.txt").unwrap();
    assert_eq!(resolved, repo.join("a.txt").canonicalize().unwrap());
}

#[test]
fn reject_absolute_path() {
    let tmp = TempDir::new().unwrap();
    let err = resolve(tmp.path(), "/etc/passwd").unwrap_err();
    assert!(matches!(err, JailError::Absolute(_)));
}

#[test]
fn reject_dotdot_escape() {
    let tmp = TempDir::new().unwrap();
    let err = resolve(tmp.path(), "../escape.txt").unwrap_err();
    assert!(matches!(err, JailError::Escapes(_)));
}

#[test]
fn reject_symlink_escape() {
    let tmp = TempDir::new().unwrap();
    let outside = tmp.path().join("../outside");
    std::fs::write(&outside, "x").unwrap();
    let link = tmp.path().join("link.txt");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&outside, &link).unwrap();

    let err = resolve(tmp.path(), "link.txt").unwrap_err();
    assert!(matches!(err, JailError::Outside(_)));
}

#[test]
fn resolve_nested_relative() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    let target = repo.join("src/main.rs");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "x").unwrap();
    let resolved = resolve(repo, "src/main.rs").unwrap();
    assert!(resolved.to_str().unwrap().ends_with("src/main.rs"));
}