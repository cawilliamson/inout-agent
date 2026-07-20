#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use inout_core::jail::{resolve, Jail, JailError};
use inout_core::tools::{Tool, ToolError, ToolRegistry};
use inout_core::types::PermissionClass;
use serde_json::{json, Value};

/// a dummy tool used only for registry tests.
struct Dummy {
    name: &'static str,
}

#[async_trait::async_trait]
impl Tool for Dummy {
    fn name(&self) -> &'static str {
        self.name
    }

    fn schema(&self) -> Value {
        json!({"name": self.name})
    }

    async fn run(&self, _args: Value) -> Result<String, ToolError> {
        Ok(String::from("ok"))
    }

    fn permission_class(&self) -> PermissionClass {
        PermissionClass::Read
    }
}

#[tokio::test]
async fn register_and_dispatch_tool() {
    let mut registry = ToolRegistry::new();
    registry.register(Dummy { name: "dummy" });
    registry.set_active(vec![String::from("dummy")]).unwrap();
    let result = registry.dispatch("dummy", json!({})).await.unwrap();
    assert_eq!(result, "ok");
}

#[tokio::test]
async fn dispatch_unknown_tool_fails() {
    let registry = ToolRegistry::new();
    let err = registry.dispatch("missing", json!({})).await.unwrap_err();
    assert!(matches!(err, ToolError::InvalidArgs(_)));
}

#[tokio::test]
async fn active_schemas_only_return_active_tools() {
    let mut registry = ToolRegistry::new();
    registry.register(Dummy { name: "a" });
    registry.register(Dummy { name: "b" });
    registry.set_active(vec![String::from("a")]).unwrap();
    let schemas = registry.active_schemas();
    assert_eq!(schemas.len(), 1);
    assert_eq!(schemas[0]["name"], "a");
}

#[tokio::test]
async fn duplicate_active_tool_is_rejected() {
    let mut registry = ToolRegistry::new();
    registry.register(Dummy { name: "a" });
    let err = registry.set_active(vec![String::from("a"), String::from("a")]).unwrap_err();
    assert!(matches!(err, ToolError::InvalidArgs(_)));
}

#[test]
fn resolve_relative_path_inside_repo() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    std::fs::write(repo.join("a.txt"), "x").unwrap();
    let resolved = resolve(repo, "a.txt").unwrap();
    assert_eq!(resolved, repo.join("a.txt").canonicalize().unwrap());
}

#[test]
fn reject_absolute_path() {
    let tmp = tempfile::tempdir().unwrap();
    let err = resolve(tmp.path(), "/etc/passwd").unwrap_err();
    assert!(matches!(err, JailError::Absolute(_)));
}

#[test]
fn reject_dotdot_escape() {
    let tmp = tempfile::tempdir().unwrap();
    let err = resolve(tmp.path(), "../escape.txt").unwrap_err();
    assert!(matches!(err, JailError::Escapes(_)));
}

#[test]
fn reject_symlink_escape() {
    let tmp = tempfile::tempdir().unwrap();
    let outside = tmp.path().join("../outside.txt");
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
fn jail_resolves_relative() {
    let tmp = tempfile::tempdir().unwrap();
    let jail = Jail::new(tmp.path().to_path_buf());
    let target = jail.resolve("foo/bar.txt").unwrap();
    assert_eq!(target, tmp.path().canonicalize().unwrap().join("foo/bar.txt"));
}
