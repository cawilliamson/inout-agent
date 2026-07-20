#![allow(clippy::print_stderr)]
//! rhai scripting tier.
//!
//! runtime-loaded `.rhai` extensions. each script gets an isolated
//! [`rhai::Engine`] and is invoked during extension registration. scripts
//! register tools, commands, and hooks by mutating the `api` map passed to
//! their `register(api)` function.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Context;
use rhai::{CallFnOptions, Dynamic, Engine, FnPtr, Map, AST};
use serde_json::Value;

use crate::config::Config;
use crate::extension::ExtensionApi;
use crate::jail::Jail;
use crate::tools::{Tool, ToolError};
use crate::types::PermissionClass;
use crate::Extension;

/// permission flags that gate host functions available to a script.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScriptPermissions {
    /// allow file writes.
    pub allow_write: bool,
    /// allow shell execution.
    pub allow_shell: bool,
    /// allow network requests.
    pub allow_network: bool,
}

impl ScriptPermissions {
    /// load flags from environment and config.
    pub fn from_env_and_config(config: &Config) -> Self {
        let env_or_cfg = |key: &str, cfg: bool| {
            std::env::var(key).map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(cfg)
        };
        Self {
            allow_write: env_or_cfg("INOUT_SCRIPTS_ALLOW_WRITE", config.scripts.allow_write),
            allow_shell: env_or_cfg("INOUT_SCRIPTS_ALLOW_SHELL", config.scripts.allow_shell),
            allow_network: env_or_cfg("INOUT_SCRIPTS_ALLOW_NETWORK", config.scripts.allow_network),
        }
    }
}

/// a runtime-loaded rhai extension.
pub struct ScriptExtension {
    /// script name, normally the file stem.
    name: String,
    /// isolated rhai engine, shared with spawned script tools.
    engine: Arc<Engine>,
    /// parsed ast of the script.
    ast: AST,
    /// tools registered by the script's `register(api)` call.
    pending: Arc<Mutex<Vec<PendingTool>>>,
    /// views pending registration, collected by `inout_register_view`.
    pending_views: Arc<Mutex<Vec<PendingView>>>,
    /// commands pending registration, collected by `inout_register_command`.
    pending_commands: Arc<Mutex<Vec<PendingCommand>>>,
}

impl std::fmt::Debug for ScriptExtension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptExtension").field("name", &self.name).finish_non_exhaustive()
    }
}

impl ScriptExtension {
    /// load and parse a `.rhai` file, registering host functions.
    pub fn from_file(
        path: &Path,
        jail: Jail,
        config: Config,
        permissions: ScriptPermissions,
    ) -> anyhow::Result<Self> {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(String::from)
            .with_context(|| format!("invalid script path: {}", path.display()))?;
        let source = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read script: {}", path.display()))?;
        let pending: Arc<Mutex<Vec<PendingTool>>> = Arc::new(Mutex::new(Vec::new()));
        let pending_views: Arc<Mutex<Vec<PendingView>>> = Arc::new(Mutex::new(Vec::new()));
        let pending_commands: Arc<Mutex<Vec<PendingCommand>>> = Arc::new(Mutex::new(Vec::new()));
        let mut engine = Engine::new();
        engine.set_max_expr_depths(0, 0);
        engine.set_max_array_size(0);
        engine.set_max_string_size(0);
        register_host_functions(&mut engine, jail, config, permissions);
        register_tool_registrar(&mut engine, pending.clone());
        register_view_registrar(&mut engine, pending_views.clone());
        register_command_registrar(&mut engine, pending_commands.clone());
        let ast = engine
            .compile(&source)
            .with_context(|| format!("failed to compile script: {}", path.display()))?;
        Ok(Self { name, engine: Arc::new(engine), ast, pending, pending_views, pending_commands })
    }
}

impl Extension for ScriptExtension {
    fn name(&self) -> &str {
        &self.name
    }

    fn register(&self, api: &mut ExtensionApi) {
        // drain any tools/views/commands left from a previous registration pass.
        self.pending.lock().expect("mutex poisoned").clear();
        self.pending_views.lock().expect("mutex poisoned").clear();
        self.pending_commands.lock().expect("mutex poisoned").clear();

        let api_map = build_api_map();
        let mut scope = rhai::Scope::new();
        let result: Result<(), _> = self.engine.call_fn_with_options(
            CallFnOptions::new().rewind_scope(false),
            &mut scope,
            &self.ast,
            "register",
            (api_map,),
        );
        if let Err(e) = result {
            eprintln!("[scripting] register failed for {}: {e}", self.name);
        }

        let tools: Vec<PendingTool> =
            std::mem::take(&mut *self.pending.lock().expect("mutex poisoned"));
        for tool in tools {
            api.tools.register(ScriptTool {
                name: tool.name,
                engine: self.engine.clone(),
                ast: self.ast.clone(),
                handler: tool.handler,
                schema: tool.schema,
                permission_class: PermissionClass::Read,
            });
        }

        let views: Vec<PendingView> =
            std::mem::take(&mut *self.pending_views.lock().expect("mutex poisoned"));
        for view in views {
            api.views.register(
                view.name,
                crate::extension::ViewBuilder {
                    title: view.title,
                    builder: view.builder,
                    engine: self.engine.clone(),
                    ast: self.ast.clone(),
                },
            );
        }

        let commands: Vec<PendingCommand> =
            std::mem::take(&mut *self.pending_commands.lock().expect("mutex poisoned"));
        for cmd in commands {
            api.commands.register(crate::extension::Command {
                name: cmd.name,
                description: cmd.description,
                handler: crate::extension::CommandHandler::Rhai {
                    engine: self.engine.clone(),
                    ast: self.ast.clone(),
                    fn_ptr: cmd.handler,
                },
            });
        }
    }
}

#[derive(Clone, Debug)]
struct PendingTool {
    name: String,
    #[allow(dead_code)]
    description: String,
    schema: Value,
    handler: FnPtr,
}

#[derive(Clone, Debug)]
struct PendingView {
    name: String,
    title: String,
    builder: FnPtr,
}

#[derive(Clone, Debug)]
struct PendingCommand {
    name: String,
    description: String,
    handler: FnPtr,
}

/// a tool whose handler is a rhai function.
pub struct ScriptTool {
    name: String,
    engine: Arc<Engine>,
    ast: AST,
    handler: FnPtr,
    schema: Value,
    permission_class: PermissionClass,
}

impl std::fmt::Debug for ScriptTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptTool")
            .field("name", &self.name)
            .field("schema", &self.schema)
            .finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl Tool for ScriptTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn schema(&self) -> Value {
        let mut schema = self.schema.clone();
        if let Some(obj) = schema.as_object_mut() {
            obj.insert("name".into(), Value::String(self.name.clone()));
        }
        schema
    }

    async fn run(&self, args: Value) -> Result<String, ToolError> {
        let args_map = json_to_map(&args);
        let result: Dynamic = self
            .handler
            .call(&self.engine, &self.ast, (args_map,))
            .map_err(|e| ToolError::Command(format!("rhai error: {e}")))?;
        let output = result
            .into_immutable_string()
            .map_err(|_| ToolError::InvalidArgs("tool handler must return a string".into()))?;
        Ok(output.to_string())
    }

    fn permission_class(&self) -> PermissionClass {
        self.permission_class
    }
}

fn register_tool_registrar(engine: &mut Engine, pending: Arc<Mutex<Vec<PendingTool>>>) {
    engine.register_fn(
        "inout_register_tool",
        move |_map: &mut Map, name: &str, desc: &str, schema: &str, handler: FnPtr| {
            let schema = serde_json::from_str(schema)
                .unwrap_or_else(|_| serde_json::json!({"type": "object"}));
            pending.lock().expect("register_tool mutex poisoned").push(PendingTool {
                name: name.to_string(),
                description: desc.to_string(),
                schema,
                handler,
            });
        },
    );
}

fn register_view_registrar(engine: &mut Engine, pending: Arc<Mutex<Vec<PendingView>>>) {
    engine.register_fn(
        "inout_register_view",
        move |_map: &mut Map, name: &str, title: &str, builder: FnPtr| {
            pending.lock().expect("register_view mutex poisoned").push(PendingView {
                name: name.to_string(),
                title: title.to_string(),
                builder,
            });
        },
    );
}

fn register_command_registrar(engine: &mut Engine, pending: Arc<Mutex<Vec<PendingCommand>>>) {
    engine.register_fn(
        "inout_register_command",
        move |_map: &mut Map, name: &str, description: &str, handler: FnPtr| {
            pending.lock().expect("register_command mutex poisoned").push(PendingCommand {
                name: name.to_string(),
                description: description.to_string(),
                handler,
            });
        },
    );
}

fn build_api_map() -> Map {
    let mut api = Map::new();

    let register_tool_ptr = FnPtr::new("inout_register_tool").expect("register_tool fnptr");
    api.insert("register_tool".into(), Dynamic::from(register_tool_ptr));

    let register_view_ptr = FnPtr::new("inout_register_view").expect("register_view fnptr");
    api.insert("register_view".into(), Dynamic::from(register_view_ptr));
    let register_command_ptr =
        FnPtr::new("inout_register_command").expect("register_command fnptr");
    api.insert("register_command".into(), Dynamic::from(register_command_ptr));

    let host_map = build_host_map();
    api.insert("host".into(), Dynamic::from(host_map));

    let config_map = Map::new();
    api.insert("config".into(), Dynamic::from(config_map));

    api
}

fn fnptr(name: &str) -> Dynamic {
    Dynamic::from(FnPtr::new(name).expect("host fnptr"))
}

fn build_host_map() -> Map {
    let mut host = Map::new();
    host.insert("now_unix_ms".into(), fnptr("inout_now_unix_ms"));
    host.insert("read_file".into(), fnptr("inout_read_file"));
    host.insert("write_file".into(), fnptr("inout_write_file"));
    host.insert("run_command".into(), fnptr("inout_run_command"));
    host.insert("walk_dir".into(), fnptr("inout_walk_dir"));
    host.insert("log".into(), fnptr("inout_log"));
    host.insert("config_get".into(), fnptr("inout_config_get"));
    host.insert("http_get".into(), fnptr("inout_http_get"));
    host.insert("http_post".into(), fnptr("inout_http_post"));
    host
}

fn register_host_functions(
    engine: &mut Engine,
    jail: Jail,
    config: Config,
    permissions: ScriptPermissions,
) {
    let ctx = Arc::new(HostContext { jail, config, permissions });

    engine.register_fn("inout_now_unix_ms", |_map: &Map| -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    });

    let ctx_read = ctx.clone();
    engine.register_fn(
        "inout_read_file",
        move |_map: &mut Map, path: &str| -> Result<String, Box<rhai::EvalAltResult>> {
            let resolved = ctx_read
                .jail
                .resolve(path)
                .map_err(|e| -> Box<rhai::EvalAltResult> { e.to_string().into() })?;
            std::fs::read_to_string(&resolved)
                .map_err(|e| -> Box<rhai::EvalAltResult> { e.to_string().into() })
        },
    );

    let ctx_write = ctx.clone();
    engine.register_fn(
        "inout_write_file",
        move |_map: &mut Map,
              path: &str,
              contents: &str|
              -> Result<bool, Box<rhai::EvalAltResult>> {
            if !ctx_write.permissions.allow_write {
                return Err("write disabled for scripts".into());
            }
            let resolved = ctx_write
                .jail
                .resolve(path)
                .map_err(|e| -> Box<rhai::EvalAltResult> { e.to_string().into() })?;
            std::fs::write(&resolved, contents)
                .map_err(|e| -> Box<rhai::EvalAltResult> { e.to_string().into() })?;
            Ok(true)
        },
    );

    let ctx_cmd = ctx.clone();
    engine.register_fn(
        "inout_run_command",
        move |_map: &mut Map,
              cmd: &str,
              args: rhai::Array|
              -> Result<Map, Box<rhai::EvalAltResult>> {
            if !ctx_cmd.permissions.allow_shell {
                return Err("shell disabled for scripts".into());
            }
            let argv: Vec<String> = args.iter().map(|a| a.to_string()).collect();
            let output = std::process::Command::new(cmd)
                .args(&argv)
                .current_dir(&ctx_cmd.config.repo_root)
                .output()
                .map_err(|e| -> Box<rhai::EvalAltResult> { e.to_string().into() })?;
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            let status = output.status.code().unwrap_or(-1) as i64;
            let mut map = Map::new();
            map.insert("stdout".into(), Dynamic::from(stdout));
            map.insert("stderr".into(), Dynamic::from(stderr));
            map.insert("status".into(), Dynamic::from(status));
            Ok(map)
        },
    );

    let ctx_walk = ctx.clone();
    engine.register_fn(
        "inout_walk_dir",
        move |_map: &mut Map, path: &str| -> Result<rhai::Array, Box<rhai::EvalAltResult>> {
            let resolved = ctx_walk
                .jail
                .resolve(path)
                .map_err(|e| -> Box<rhai::EvalAltResult> { e.to_string().into() })?;
            let root = ctx_walk.jail.root.clone();
            let mut entries: Vec<String> = Vec::new();
            walk(&resolved, &root, &mut entries)
                .map_err(|e| -> Box<rhai::EvalAltResult> { e.to_string().into() })?;
            entries.sort();
            let arr: rhai::Array = entries.into_iter().map(Dynamic::from).collect();
            Ok(arr)
        },
    );

    engine.register_fn("inout_log", |_map: &mut Map, level: &str, message: &str| {
        eprintln!("[{level}] {message}");
    });

    let ctx_cfg = ctx.clone();
    engine.register_fn("inout_config_get", move |_map: &mut Map, key: &str| -> Dynamic {
        match key {
            "llm_provider" => Dynamic::from(ctx_cfg.config.llm_provider.clone()),
            "model" => Dynamic::from(ctx_cfg.config.model.clone()),
            "repo_root" => Dynamic::from(ctx_cfg.config.repo_root.to_string_lossy().to_string()),
            "max_turns" => Dynamic::from(ctx_cfg.config.max_turns as i64),
            "bash_full" => Dynamic::from(ctx_cfg.config.bash.full),
            "bash_timeout_secs" => Dynamic::from(ctx_cfg.config.bash.timeout_secs as i64),
            "bash_allowlist" => {
                let arr: rhai::Array =
                    ctx_cfg.config.bash.allowlist.iter().cloned().map(Dynamic::from).collect();
                Dynamic::from(arr)
            }
            _ => Dynamic::UNIT,
        }
    });

    let ctx_http = ctx.clone();
    engine.register_fn(
        "inout_http_get",
        move |_map: &mut Map, _url: &str| -> Result<String, Box<rhai::EvalAltResult>> {
            if !ctx_http.permissions.allow_network {
                return Err("network disabled for scripts".into());
            }
            Err("http_get not yet implemented".into())
        },
    );

    let ctx_http_post = ctx.clone();
    engine.register_fn(
        "inout_http_post",
        move |_map: &mut Map,
              _url: &str,
              _body: &str|
              -> Result<String, Box<rhai::EvalAltResult>> {
            if !ctx_http_post.permissions.allow_network {
                return Err("network disabled for scripts".into());
            }
            Err("http_post not yet implemented".into())
        },
    );
}

#[derive(Clone, Debug)]
struct HostContext {
    jail: Jail,
    config: Config,
    permissions: ScriptPermissions,
}

fn walk(
    root: &std::path::Path,
    jail_root: &std::path::Path,
    out: &mut Vec<String>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(jail_root).unwrap_or(&path);
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if !rel_str.is_empty() {
            out.push(rel_str);
        }
        if path.is_dir() {
            walk(&path, jail_root, out)?;
        }
    }
    Ok(())
}

/// convert a `serde_json::Value` into a `rhai::Map` for passing json data
/// into rhai script functions. objects become maps, arrays become rhai
/// arrays, numbers become ints or floats, nulls become unit.
pub fn json_to_map(value: &Value) -> Map {
    let mut map = Map::new();
    if let Some(obj) = value.as_object() {
        for (k, v) in obj {
            map.insert(k.clone().into(), json_to_dynamic(v));
        }
    }
    map
}

fn json_to_dynamic(value: &Value) -> Dynamic {
    match value {
        Value::Null => Dynamic::UNIT,
        Value::Bool(b) => Dynamic::from(*b),
        Value::Number(n) => n
            .as_i64()
            .map(Dynamic::from)
            .or_else(|| n.as_f64().map(Dynamic::from))
            .unwrap_or_else(|| Dynamic::from(n.to_string())),
        Value::String(s) => Dynamic::from(s.clone()),
        Value::Array(arr) => {
            Dynamic::from(arr.iter().map(json_to_dynamic).collect::<rhai::Array>())
        }
        Value::Object(obj) => {
            let mut map = Map::new();
            for (k, v) in obj {
                map.insert(k.clone().into(), json_to_dynamic(v));
            }
            Dynamic::from(map)
        }
    }
}

/// discover `.rhai` extensions from the standard paths and register them.
///
/// discovery order: `~/.inout/extensions/`, `.inout/extensions/`, then
/// `config.extension_paths`. within each directory, files are sorted
/// alphabetically.
pub fn load_script_extensions(api: &mut ExtensionApi, config: &Config) {
    let permissions = ScriptPermissions::from_env_and_config(config);
    let jail = Jail::new(config.repo_root.clone());
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(home).join(".inout").join("extensions"));
    }
    dirs.push(config.repo_root.join(".inout").join("extensions"));
    for p in &config.extension_paths {
        dirs.push(p.clone());
    }

    for dir in &dirs {
        if !dir.is_dir() {
            continue;
        }
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
            .unwrap_or_default();
        entries.sort();
        for path in entries {
            if path.extension().and_then(|e| e.to_str()) == Some("rhai") {
                match ScriptExtension::from_file(&path, jail.clone(), config.clone(), permissions) {
                    Ok(ext) => {
                        let name = ext.name().to_string();
                        (api.observe)(format!("extension_loaded:{name}"));
                        ext.register(api);
                    }
                    Err(e) => {
                        eprintln!("[scripting] failed to load {}: {e}", path.display());
                    }
                }
            }
        }
    }
}

/// invoke a registered view's builder fnptr with a conversation snapshot and
/// convert the returned rhai map into a typed `ViewSpec` the tui can render.
///
/// `snapshot` is the conversation as json; it is converted to a rhai map before
/// being passed to the script. the script returns a map with the shape
/// documented in `spec/v2.10-scripting.md` (turns array, totals). unknown keys
/// are tolerated; missing keys default to empty/zero.
pub fn build_view(
    builder: &crate::extension::ViewBuilder,
    snapshot: &Value,
) -> anyhow::Result<crate::extension::ViewSpec> {
    let snapshot_map = json_to_map(snapshot);
    let result: Dynamic = builder
        .builder
        .call(&builder.engine, &builder.ast, (snapshot_map,))
        .map_err(|e| anyhow::anyhow!("view builder `{}` failed: {e}", builder.builder.fn_name()))?;
    let map = result
        .try_cast::<Map>()
        .ok_or_else(|| anyhow::anyhow!("view builder must return a map"))?;
    let spec = crate::extension::ViewSpec {
        total_tokens: map_get_int(&map, "total_tokens") as usize,
        limit_tokens: map_get_int(&map, "limit_tokens") as usize,
        context_pct: map_get_int(&map, "context_pct").clamp(0, 100) as u8,
        turns: {
            let mut turns = Vec::new();
            if let Some(turns_dyn) = map.get("turns") {
                if let Ok(turns_ref) = turns_dyn.as_array_ref() {
                    for turn_dyn in turns_ref.iter() {
                        if let Ok(turn_ref) = turn_dyn.as_map_ref() {
                            turns.push(parse_view_turn(&turn_ref));
                        }
                    }
                }
            }
            turns
        },
    };
    Ok(spec)
}

fn parse_view_turn(map: &Map) -> crate::extension::ViewTurn {
    let mut blocks: Vec<crate::extension::ViewBlock> = Vec::new();
    if let Some(blocks_dyn) = map.get("blocks") {
        if let Ok(blocks_ref) = blocks_dyn.as_array_ref() {
            for block_dyn in blocks_ref.iter() {
                if let Ok(block_ref) = block_dyn.as_map_ref() {
                    if let Some(block) = parse_view_block(&block_ref) {
                        blocks.push(block);
                    }
                }
            }
        }
    }
    crate::extension::ViewTurn {
        msg_index: map_get_int(map, "msg_index") as usize,
        msg_count: map_get_int(map, "msg_count") as usize,
        preview: map_get_str(map, "preview"),
        tokens_est: map_get_int(map, "tokens_est") as usize,
        in_window: map_get_bool(map, "in_window"),
        blocks,
    }
}

fn parse_view_block(map: &Map) -> Option<crate::extension::ViewBlock> {
    let kind = map_get_str(map, "kind");
    let tokens = map_get_int(map, "tokens") as usize;
    match kind.as_str() {
        "user_text" => {
            Some(crate::extension::ViewBlock::UserText { text: map_get_str(map, "text"), tokens })
        }
        "assistant_text" => Some(crate::extension::ViewBlock::AssistantText {
            text: map_get_str(map, "text"),
            tokens,
        }),
        "tool_call" => Some(crate::extension::ViewBlock::ToolCall {
            name: map_get_str(map, "name"),
            input_json: map_get_str(map, "input_json"),
            tokens,
        }),
        "tool_result" => Some(crate::extension::ViewBlock::ToolResult {
            tool_name: map_get_str(map, "tool_name"),
            content: map_get_str(map, "content"),
            tokens,
        }),
        _ => None,
    }
}

fn map_get_int(map: &Map, key: &str) -> i64 {
    map.get(key).and_then(|d| d.as_int().ok()).unwrap_or(0)
}

fn map_get_str(map: &Map, key: &str) -> String {
    map.get(key)
        .and_then(|d| d.as_immutable_string_ref().ok())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn map_get_bool(map: &Map, key: &str) -> bool {
    map.get(key).and_then(|d| d.as_bool().ok()).unwrap_or(false)
}

/// convert a rhai map returned by a command handler into a typed
/// `CommandResult`. expected shape: `{ message: string, action: string }`
/// where `action` is one of `open_view:<name>`, `clear_history`,
/// `set_model:<model>`, `reload`, `exit`, `run_turn:<text>`, or empty/missing
/// for no action.
pub fn map_to_command_result(map: &Map) -> anyhow::Result<crate::extension::CommandResult> {
    let message = map_get_str(map, "message");
    let action_str = map_get_str(map, "action");
    let action = if action_str.is_empty() { None } else { parse_action(&action_str) };
    Ok(crate::extension::CommandResult { message, action })
}

fn parse_action(s: &str) -> Option<crate::extension::CommandAction> {
    if let Some(name) = s.strip_prefix("open_view:") {
        Some(crate::extension::CommandAction::OpenView(name.to_string()))
    } else if s == "clear_history" {
        Some(crate::extension::CommandAction::ClearHistory)
    } else if let Some(model) = s.strip_prefix("set_model:") {
        Some(crate::extension::CommandAction::SetModel(model.to_string()))
    } else if s == "reload" {
        Some(crate::extension::CommandAction::ReloadExtensions)
    } else if s == "exit" {
        Some(crate::extension::CommandAction::Exit)
    } else if let Some(text) = s.strip_prefix("run_turn:") {
        Some(crate::extension::CommandAction::RunTurn(text.to_string()))
    } else if s == "undo_last_turn" {
        Some(crate::extension::CommandAction::UndoLastTurn)
    } else {
        None
    }
}
