#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;

use inout_core::config::Config;
use inout_core::extension::{Extension, ExtensionApi};
use inout_core::jail::Jail;
use inout_core::scripting::{ScriptExtension, ScriptPermissions};
use inout_core::tools::ToolCall;
use serde_json::{json, Value};

fn extensions_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("extensions")
}

fn load_script(name: &str, permissions: ScriptPermissions) -> ScriptExtension {
    let path = extensions_dir().join(format!("{name}.rhai"));
    let config = Config::default();
    let jail = Jail::new(config.repo_root.clone());
    ScriptExtension::from_file(&path, jail, config, permissions).expect("script loads")
}

fn tmp_repo() -> (tempfile::TempDir, Config) {
    let dir = tempfile::tempdir().expect("tempdir");
    let config = Config { repo_root: dir.path().to_path_buf(), ..Config::default() };
    (dir, config)
}

#[tokio::test]
async fn read_tool_reads_file_slice() {
    let (_tmp, config) = tmp_repo();
    std::fs::write(config.repo_root.join("sample.txt"), "line1\nline2\nline3\n").unwrap();
    let jail = Jail::new(config.repo_root.clone());
    let ext = ScriptExtension::from_file(
        &extensions_dir().join("read.rhai"),
        jail,
        config,
        ScriptPermissions::default(),
    )
    .expect("load read.rhai");
    let mut api = ExtensionApi::noop();
    ext.register(&mut api);
    let call = ToolCall {
        id: "t1".into(),
        name: "read".into(),
        arguments: json!({ "path": "sample.txt", "offset": 2, "limit": 1 }),
    };
    let result = api.tools.dispatch_call(&call).await.expect("read ok");
    assert_eq!(result, "line2");
}

#[tokio::test]
async fn read_tool_missing_path_errors() {
    let (_tmp, _config) = tmp_repo();
    let ext = load_script("read", ScriptPermissions::default());
    let mut api = ExtensionApi::noop();
    ext.register(&mut api);
    let call = ToolCall { id: "t1".into(), name: "read".into(), arguments: json!({}) };
    let err = api.tools.dispatch_call(&call).await.expect_err("should error");
    assert!(err.to_string().contains("path required"));
}

#[tokio::test]
async fn write_tool_writes_file() {
    let (_tmp, config) = tmp_repo();
    let jail = Jail::new(config.repo_root.clone());
    let ext = ScriptExtension::from_file(
        &extensions_dir().join("write.rhai"),
        jail,
        config.clone(),
        ScriptPermissions { allow_write: true, ..ScriptPermissions::default() },
    )
    .expect("load write.rhai");
    let mut api = ExtensionApi::noop();
    ext.register(&mut api);
    let call = ToolCall {
        id: "t1".into(),
        name: "write".into(),
        arguments: json!({ "path": "out.txt", "content": "hello" }),
    };
    let result = api.tools.dispatch_call(&call).await.expect("write ok");
    assert_eq!(result, "wrote 5 bytes to out.txt");
    let written = std::fs::read_to_string(config.repo_root.join("out.txt")).unwrap();
    assert_eq!(written, "hello");
}

#[tokio::test]
async fn write_tool_blocked_without_permission() {
    let (_tmp, config) = tmp_repo();
    let jail = Jail::new(config.repo_root.clone());
    let ext = ScriptExtension::from_file(
        &extensions_dir().join("write.rhai"),
        jail,
        config,
        ScriptPermissions::default(),
    )
    .expect("load write.rhai");
    let mut api = ExtensionApi::noop();
    ext.register(&mut api);
    let call = ToolCall {
        id: "t1".into(),
        name: "write".into(),
        arguments: json!({ "path": "out.txt", "content": "hello" }),
    };
    let err = api.tools.dispatch_call(&call).await.expect_err("should be blocked");
    assert!(err.to_string().contains("write disabled"));
}

#[test]
fn all_scripts_parse() {
    for name in ["read", "write", "edit", "grep", "glob", "bash"] {
        let (_tmp, config) = tmp_repo();
        let jail = Jail::new(config.repo_root.clone());
        let path = extensions_dir().join(format!("{name}.rhai"));
        ScriptExtension::from_file(&path, jail, config, ScriptPermissions::default())
            .unwrap_or_else(|e| panic!("parse {name}.rhai: {e}"));
    }
}

// keep a reference to Value so unused-warnings stay quiet for future tests
#[test]
fn _value_type_present() {
    let _v: Value = json!({});
}

#[test]
fn context_view_builds_spec() {
    let ext = load_script("context", ScriptPermissions::default());
    let mut api = ExtensionApi::noop();
    ext.register(&mut api);
    let builder = api.views.get("context").expect("context view registered");

    let snapshot = json!({
        "messages": [
            {"role":"user","content":"hello","tool_calls":[],"tool_call_id":""},
            {"role":"assistant","content":"hi there","tool_calls":[],"tool_call_id":""},
            {"role":"user","content":"read foo.txt","tool_calls":[],"tool_call_id":""},
            {"role":"assistant","content":"","tool_calls":[{"id":"t1","name":"read","arguments_json":"{\"path\":\"foo.txt\"}"}],"tool_call_id":""},
            {"role":"tool","content":"file contents here","tool_calls":[],"tool_call_id":"t1"}
        ],
        "max_turns": 20
    });

    let spec = inout_core::build_view(builder, &snapshot).expect("view builds");

    assert_eq!(spec.turns.len(), 2, "two user turns expected");
    assert!(spec.turns[0].preview.contains("hello"), "first turn preview should contain hello");

    let second_blocks: Vec<_> = spec.turns[1].blocks.iter().collect();
    assert!(
        second_blocks.iter().any(|b| matches!(b, inout_core::extension::ViewBlock::ToolCall { name, .. } if name == "read")),
        "second turn should contain a read tool_call"
    );
    assert!(
        second_blocks.iter().any(|b| matches!(b, inout_core::extension::ViewBlock::ToolResult { tool_name, .. } if tool_name == "read")),
        "second turn should contain a read tool_result"
    );

    assert!(spec.total_tokens > 0, "total_tokens should be positive");
    assert_eq!(spec.limit_tokens, 128000, "limit_tokens should default to 128000");
    assert!(spec.context_pct <= 100, "context_pct should be clamped to 100");
}

fn load_with_perms(name: &str, perms: ScriptPermissions, config: &Config) -> ScriptExtension {
    let jail = Jail::new(config.repo_root.clone());
    let path = extensions_dir().join(format!("{name}.rhai"));
    ScriptExtension::from_file(&path, jail, config.clone(), perms).expect("script loads")
}

#[tokio::test]
async fn edit_tool_replaces_first_occurrence() {
    let (_tmp, config) = tmp_repo();
    std::fs::write(config.repo_root.join("f.txt"), "foo bar foo").unwrap();
    let ext = load_with_perms(
        "edit",
        ScriptPermissions { allow_write: true, ..ScriptPermissions::default() },
        &config,
    );
    let mut api = ExtensionApi::noop();
    ext.register(&mut api);
    let call = ToolCall {
        id: "t1".into(),
        name: "edit".into(),
        arguments: json!({ "path": "f.txt", "old_string": "foo", "new_string": "baz" }),
    };
    let result = api.tools.dispatch_call(&call).await.expect("edit ok");
    assert!(result.contains("replaced"));
    let written = std::fs::read_to_string(config.repo_root.join("f.txt")).unwrap();
    assert_eq!(written, "baz bar foo");
}

#[tokio::test]
async fn edit_tool_old_string_not_found_errors() {
    let (_tmp, config) = tmp_repo();
    std::fs::write(config.repo_root.join("f.txt"), "hello").unwrap();
    let ext = load_with_perms(
        "edit",
        ScriptPermissions { allow_write: true, ..ScriptPermissions::default() },
        &config,
    );
    let mut api = ExtensionApi::noop();
    ext.register(&mut api);
    let call = ToolCall {
        id: "t1".into(),
        name: "edit".into(),
        arguments: json!({ "path": "f.txt", "old_string": "xyz", "new_string": "abc" }),
    };
    let err = api.tools.dispatch_call(&call).await.expect_err("should error");
    assert!(err.to_string().contains("old_string not found"));
}

#[tokio::test]
async fn grep_tool_filters_matching_lines() {
    let (_tmp, config) = tmp_repo();
    std::fs::write(config.repo_root.join("f.txt"), "apple\nbanana\napricot\n").unwrap();
    let ext = load_with_perms("grep", ScriptPermissions::default(), &config);
    let mut api = ExtensionApi::noop();
    ext.register(&mut api);
    let call = ToolCall {
        id: "t1".into(),
        name: "grep".into(),
        arguments: json!({ "path": "f.txt", "pattern": "ap" }),
    };
    let result = api.tools.dispatch_call(&call).await.expect("grep ok");
    assert_eq!(result, "apple\napricot");
}

#[tokio::test]
async fn grep_tool_case_insensitive() {
    let (_tmp, config) = tmp_repo();
    std::fs::write(config.repo_root.join("f.txt"), "Hello\nworld\nHELLO\n").unwrap();
    let ext = load_with_perms("grep", ScriptPermissions::default(), &config);
    let mut api = ExtensionApi::noop();
    ext.register(&mut api);
    let call = ToolCall {
        id: "t1".into(),
        name: "grep".into(),
        arguments: json!({ "path": "f.txt", "pattern": "hello", "case_sensitive": false }),
    };
    let result = api.tools.dispatch_call(&call).await.expect("grep ok");
    assert_eq!(result, "Hello\nHELLO");
}

#[tokio::test]
async fn glob_tool_lists_matching_files() {
    let (_tmp, config) = tmp_repo();
    std::fs::write(config.repo_root.join("a.txt"), "").unwrap();
    std::fs::write(config.repo_root.join("b.rs"), "").unwrap();
    std::fs::create_dir_all(config.repo_root.join("sub")).unwrap();
    std::fs::write(config.repo_root.join("sub").join("c.txt"), "").unwrap();
    let ext = load_with_perms("glob", ScriptPermissions::default(), &config);
    let mut api = ExtensionApi::noop();
    ext.register(&mut api);
    let call = ToolCall {
        id: "t1".into(),
        name: "glob".into(),
        arguments: json!({ "path": "", "pattern": "*.txt" }),
    };
    let result = api.tools.dispatch_call(&call).await.expect("glob ok");
    let entries: Vec<&str> = result.split('\n').filter(|s| !s.is_empty()).collect();
    assert!(entries.contains(&"a.txt"));
    assert!(entries.contains(&"sub/c.txt"));
    assert!(!entries.contains(&"b.rs"));
}

#[tokio::test]
async fn bash_tool_blocked_binary_rejected() {
    let (_tmp, config) = tmp_repo();
    let ext = load_with_perms(
        "bash",
        ScriptPermissions { allow_shell: true, ..ScriptPermissions::default() },
        &config,
    );
    let mut api = ExtensionApi::noop();
    ext.register(&mut api);
    let call = ToolCall {
        id: "t1".into(),
        name: "bash".into(),
        arguments: json!({ "command": "rm -f x" }),
    };
    let err = api.tools.dispatch_call(&call).await.expect_err("should block rm");
    assert!(err.to_string().contains("rm is blocked"));
}

#[tokio::test]
async fn bash_tool_runs_allowed_command() {
    let (_tmp, config) = tmp_repo();
    let ext = load_with_perms(
        "bash",
        ScriptPermissions { allow_shell: true, ..ScriptPermissions::default() },
        &config,
    );
    let mut api = ExtensionApi::noop();
    ext.register(&mut api);
    let call = ToolCall {
        id: "t1".into(),
        name: "bash".into(),
        arguments: json!({ "command": "echo hello" }),
    };
    let result = api.tools.dispatch_call(&call).await.expect("bash ok");
    assert_eq!(result.trim(), "hello");
}

#[test]
fn fullview_registers_view_and_command() {
    let ext = load_script("fullview", ScriptPermissions::default());
    let mut api = ExtensionApi::noop();
    ext.register(&mut api);
    assert!(api.views.get("full").is_some(), "full view should be registered");
    assert!(api.commands.get("full").is_some(), "/full command should be registered");
}

#[test]
fn fullview_builds_spec_with_system_prompt() {
    let ext = load_script("fullview", ScriptPermissions::default());
    let mut api = ExtensionApi::noop();
    ext.register(&mut api);
    let builder = api.views.get("full").expect("full view registered");

    let snapshot = json!({
        "messages": [
            {"role":"system","content":"you are a helpful agent","tool_calls":[],"tool_call_id":"","reasoning":"","system_prompt":"you are a helpful agent"},
            {"role":"user","content":"what is 2+2","tool_calls":[],"tool_call_id":"","reasoning":"","system_prompt":"you are a helpful agent"},
            {"role":"assistant","content":"4","tool_calls":[],"tool_call_id":"","reasoning":"adding two and two","system_prompt":"you are a helpful agent"}
        ],
        "max_turns": 20
    });

    let spec = inout_core::build_view(builder, &snapshot).expect("full view builds");

    // system prompt turn + one user turn = 2 turns
    assert!(spec.turns.len() >= 2, "should have system prompt + user turn");
    // first turn is the system prompt
    assert!(
        spec.turns[0].preview.contains("system prompt"),
        "first turn should be system prompt, got: {}",
        spec.turns[0].preview
    );
    // find the user turn and check reasoning is present in blocks
    let user_turn =
        spec.turns.iter().find(|t| t.preview.contains("2+2")).expect("should find user turn");
    let has_reasoning = user_turn.blocks.iter().any(|b| {
        matches!(b, inout_core::extension::ViewBlock::AssistantText { text, .. } if text.contains("[reasoning]"))
    });
    assert!(has_reasoning, "user turn should contain reasoning block");
}

#[test]
fn commands_register_core_slash_commands() {
    let ext = load_script("commands", ScriptPermissions::default());
    let mut api = ExtensionApi::noop();
    ext.register(&mut api);

    for cmd in ["help", "clear", "new", "model", "undo", "exit", "reload", "context"] {
        assert!(api.commands.get(cmd).is_some(), "/{cmd} should be registered");
    }
}

#[test]
fn commands_clear_returns_clear_action() {
    let ext = load_script("commands", ScriptPermissions::default());
    let mut api = ExtensionApi::noop();
    ext.register(&mut api);

    let ctx = inout_core::CommandContext {
        model: "test".to_string(),
        system_prompt: String::new(),
        args: String::new(),
        snapshot: json!({"messages":[],"max_turns":20}),
    };
    let result = api.commands.dispatch("clear", &ctx).expect("dispatch clear");
    assert_eq!(result.message, "history cleared");
    assert!(matches!(result.action, Some(inout_core::CommandAction::ClearHistory)));
}
