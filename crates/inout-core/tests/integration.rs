#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use inout_core::jail::{resolve, Jail, JailError};
use inout_core::tools::{Tool, ToolError, ToolRegistry};
use inout_core::types::PermissionClass;
use inout_testing::{scenario, then, when};
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
    let mut s = scenario!("extensions", "Tool registry active set validation", "Unknown tool name rejected");
    let mut registry = ToolRegistry::new();
    registry.register(Dummy { name: "dummy" });
    registry.set_active(vec![String::from("dummy")]).unwrap();
    when!(s, "the registered tool is dispatched", {
        let result = registry.dispatch("dummy", json!({})).await.unwrap();
        then!(s, "the tool result content is returned", {
            assert_eq!(result, "ok");
        });
    });
}

#[tokio::test]
async fn dispatch_unknown_tool_fails() {
    let mut s = scenario!("extensions", "Tool registry active set validation", "Unknown tool name rejected");
    let registry = ToolRegistry::new();
    when!(s, "an unregistered tool name is dispatched", {
        let err = registry.dispatch("missing", json!({})).await.unwrap_err();
        then!(s, "the call returns an invalid-args error", {
            assert!(matches!(err, ToolError::InvalidArgs(_)));
        });
    });
}

#[tokio::test]
async fn active_schemas_only_return_active_tools() {
    let mut s = scenario!("extensions", "Tool registry active set validation", "Duplicate tool name rejected");
    let mut registry = ToolRegistry::new();
    registry.register(Dummy { name: "a" });
    registry.register(Dummy { name: "b" });
    registry.set_active(vec![String::from("a")]).unwrap();
    when!(s, "active_schemas is queried", {
        let schemas = registry.active_schemas();
        then!(s, "only the active tool's schema is returned", {
            assert_eq!(schemas.len(), 1);
            assert_eq!(schemas[0]["name"], "a");
        });
    });
}

#[tokio::test]
async fn duplicate_active_tool_is_rejected() {
    let mut s = scenario!("extensions", "Tool registry active set validation", "Duplicate tool name rejected");
    let mut registry = ToolRegistry::new();
    registry.register(Dummy { name: "a" });
    when!(s, "set_active is called with a duplicated tool name", {
        let err = registry.set_active(vec![String::from("a"), String::from("a")]).unwrap_err();
        then!(s, "the call returns an invalid-args error", {
            assert!(matches!(err, ToolError::InvalidArgs(_)));
        });
    });
}

#[test]
fn resolve_relative_path_inside_repo() {
    let mut s = scenario!("security", "Jail path confinement", "Relative path inside repo is resolved");
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();
    std::fs::write(repo.join("a.txt"), "x").unwrap();
    when!(s, "resolve is called with a relative path inside the repo", {
        let resolved = resolve(repo, "a.txt").unwrap();
        then!(s, "the resolved path matches the canonical repo path", {
            assert_eq!(resolved, repo.join("a.txt").canonicalize().unwrap());
        });
    });
}

#[test]
fn reject_absolute_path() {
    let mut s = scenario!("security", "Jail path confinement", "Absolute path is rejected");
    let tmp = tempfile::tempdir().unwrap();
    when!(s, "resolve is called with an absolute path", {
        let err = resolve(tmp.path(), "/etc/passwd").unwrap_err();
        then!(s, "the call rejects with an absolute-path error", {
            assert!(matches!(err, JailError::Absolute(_)));
        });
    });
}

#[test]
fn reject_dotdot_escape() {
    let mut s = scenario!("security", "Jail path confinement", "Parent-directory escape is rejected");
    let tmp = tempfile::tempdir().unwrap();
    when!(s, "resolve is called with a parent-directory escape", {
        let err = resolve(tmp.path(), "../escape.txt").unwrap_err();
        then!(s, "the call rejects with an escapes error", {
            assert!(matches!(err, JailError::Escapes(_)));
        });
    });
}

#[test]
fn reject_symlink_escape() {
    let mut s = scenario!("security", "Jail path confinement", "Symlink escape is rejected");
    let tmp = tempfile::tempdir().unwrap();
    let outside = tmp.path().join("../outside.txt");
    std::fs::write(&outside, "x").unwrap();
    let link = tmp.path().join("link.txt");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&outside, &link).unwrap();

    when!(s, "resolve is called with a symlink that escapes the repo", {
        let err = resolve(tmp.path(), "link.txt").unwrap_err();
        then!(s, "the call rejects with an outside error", {
            assert!(matches!(err, JailError::Outside(_)));
        });
    });
}

#[test]
fn jail_resolves_relative() {
    let mut s = scenario!("security", "Jail path confinement", "Nested relative path is resolved");
    let tmp = tempfile::tempdir().unwrap();
    let jail = Jail::new(tmp.path().to_path_buf());
    when!(s, "Jail::resolve is called with a nested relative path", {
        let target = jail.resolve("foo/bar.txt").unwrap();
        then!(s, "the resolved path is the canonical repo root joined with the input", {
            assert_eq!(target, tmp.path().canonicalize().unwrap().join("foo/bar.txt"));
        });
    });
}
