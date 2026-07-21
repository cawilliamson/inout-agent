//! inout: minimal rust-native ai agent — io on the command line.
//!
//! v1 scope: conversation loop, tools via runtime-loaded rhai extensions,
//! single agent, jsonl history, vcr tests. not v1: subagents, hub, mcp,
//! browser, debug, lsp, ast, skills, managed skills, todos, learn, slash
//! commands, parallel tool calls.

#![allow(missing_docs)]
#![allow(missing_debug_implementations)]

pub mod history;
pub mod llm;
pub mod state;
pub mod tui;

use std::sync::{Arc, Mutex};

use anyhow::Result;
use inout_core::config::Config;
use inout_core::extension::ExtensionApi;
use inout_core::tools::ToolRegistry;

use crate::history::{History, Role};
use crate::llm::LlmClient;
use crate::state::State;

/// the agent: config, history, state machine, llm client, tool registry.
pub struct Agent {
    /// shared config.
    pub config: Arc<Config>,
    /// conversation history.
    pub history: History,
    /// agent state machine.
    pub state: State,
    /// llm provider client.
    pub llm: Box<dyn LlmClient>,
    /// registered tools.
    pub tools: ToolRegistry,
    /// registered tui views built by extensions.
    pub views: inout_core::ViewRegistry,
    /// registered slash commands built by extensions.
    pub commands: inout_core::CommandRegistry,
    /// whether extensions have been loaded.
    pub extensions_loaded: bool,
}

impl Agent {
    /// build an agent with the given config and llm client. extensions are
    /// not loaded yet — call `load_extensions` (or let the tui lazy-load).
    pub fn new(mut config: Config, llm: Box<dyn LlmClient>) -> Self {
        let repo_root = config.repo_root.clone();
        let max_turns = config.max_turns;

        // the bundled first-party tools live in the workspace `extensions/`
        // directory. add it to the extension search path so they load.
        let bundled = std::env::var("IO_EXTENSIONS_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("extensions"));
        if !config.extension_paths.contains(&bundled) {
            config.extension_paths.push(bundled);
        }

        // the agent trusts its own bundled tool scripts with write and shell
        // access. user scripts from ~/.inout or .inout inherit the same flags
        // in v1; a permission-manager extension can narrow this later.
        config.scripts.allow_write = true;
        config.scripts.allow_shell = true;

        let tools = ToolRegistry::new();
        let views = inout_core::ViewRegistry::new();
        let commands = inout_core::CommandRegistry::new();
        let prompt = std::env::var("IO_SYSTEM_PROMPT")
            .unwrap_or_else(|_| default_system_prompt(&tools, &repo_root));
        let mut history = History::new(max_turns);
        history.set_system_prompt(prompt);
        Self {
            config: Arc::new(config),
            history,
            state: State::AwaitingUser,
            llm,
            tools,
            views,
            commands,
            extensions_loaded: false,
        }
    }

    /// load all rhai script extensions, returning the names loaded.
    pub fn load_extensions(&mut self) -> Vec<String> {
        let names = Arc::new(Mutex::new(Vec::new()));
        let names_clone = names.clone();
        let observe: Arc<dyn Fn(String) + Send + Sync> = Arc::new(move |msg| {
            if let Some(rest) = msg.strip_prefix("extension_loaded:") {
                names_clone.lock().expect("observe mutex").push(rest.to_string());
            }
        });
        self.load_extensions_with(observe);
        let loaded = names.lock().expect("observe mutex").clone();
        loaded
    }

    /// load all rhai script extensions, invoking `observe` for each
    /// `extension_loaded:{name}` event emitted by the loader.
    pub fn load_extensions_with(&mut self, observe: Arc<dyn Fn(String) + Send + Sync>) {
        let before = Arc::new(|_: &inout_core::LlmRequest| {})
            as Arc<dyn Fn(&inout_core::LlmRequest) + Send + Sync>;
        let mut api = ExtensionApi {
            tools: std::mem::take(&mut self.tools),
            views: std::mem::take(&mut self.views),
            commands: std::mem::take(&mut self.commands),
            observe,
            before_provider_payload: before,
        };
        inout_core::load_script_extensions(&mut api, &self.config);

        // register compiled first-party extensions. these run after script
        // extensions so they can observe what scripts registered, and their
        // commands/views/tools override script-based ones on name conflict.
        for ext in compiled_extensions() {
            (api.observe)(format!("extension_loaded:{}", ext.name()));
            ext.register(&mut api);
        }

        self.tools = api.tools;
        self.views = api.views;
        self.commands = api.commands;

        // rebuild system prompt now that tools are registered.
        let repo_root = self.config.repo_root.clone();
        let prompt = std::env::var("IO_SYSTEM_PROMPT")
            .unwrap_or_else(|_| default_system_prompt(&self.tools, &repo_root));
        self.history.set_system_prompt(prompt);

        self.extensions_loaded = true;
    }

    /// build a tui view by name by invoking the registered builder with a
    /// conversation snapshot. returns `None` if no view with that name is
    /// registered or the builder errors.
    pub fn build_view(&self, name: &str) -> Option<inout_core::ViewSpec> {
        let builder = self.views.get(name)?;
        let snapshot = self.build_conversation_snapshot();
        inout_core::build_view(builder, &snapshot).ok()
    }

    /// build the context-viewer spec. shorthand for `build_view("context")`.
    pub fn build_context_view(&self) -> Option<inout_core::ViewSpec> {
        self.build_view("context")
    }

    /// serialize the current history as a json conversation snapshot suitable
    /// for passing to a rhai view builder.
    pub fn build_conversation_snapshot(&self) -> serde_json::Value {
        use serde_json::json;
        let messages: Vec<serde_json::Value> = self
            .history
            .messages
            .iter()
            .map(|m| {
                let tool_calls: Vec<serde_json::Value> = m
                    .tool_calls
                    .iter()
                    .map(|tc| {
                        json!({
                            "id": tc.id,
                            "name": tc.name,
                            "arguments_json": serde_json::to_string(&tc.arguments).unwrap_or_default(),
                        })
                    })
                    .collect();
                json!({
                    "role": match m.role {
                        Role::System => "system",
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        Role::Tool => "tool",
                    },
                    "content": m.content,
                    "tool_calls": tool_calls,
                    "tool_call_id": m.tool_call_id.clone().unwrap_or_default(),
                    "reasoning": m.reasoning,
                    "system_prompt": self.history.system_prompt.clone().unwrap_or_default(),
                })
            })
            .collect();
        json!({
            "messages": messages,
            "max_turns": self.config.max_turns,
        })
    }

    /// run a single user turn to completion, returning the final assistant text.
    pub async fn run_turn(&mut self, user_msg: String) -> Result<String> {
        self.state = State::Thinking;
        self.history.append_user(user_msg);

        loop {
            let req = self.history.to_request(&self.config.model, &self.tools.schemas());
            let resp = self.llm.complete(req).await?;

            if resp.tool_calls.is_empty() {
                self.state = State::Responding;
                self.history.append_assistant(resp.content.clone());
                return Ok(resp.content);
            }

            self.state = State::ToolRunning;
            self.history.append_assistant_with_tools(resp.content.clone(), resp.tool_calls.clone());

            for call in &resp.tool_calls {
                let result =
                    self.tools.dispatch_call(call).await.unwrap_or_else(|e| format!("error: {e}"));
                self.history.append_tool_result(call.id.clone(), result);
            }

            self.state = State::Thinking;
        }
    }
}

fn default_system_prompt(tools: &ToolRegistry, repo_root: &std::path::Path) -> String {
    let tool_docs: Vec<String> = tools
        .schemas()
        .iter()
        .filter_map(|s| {
            let name = s.get("name").and_then(|n| n.as_str())?;
            let desc = s.get("description").and_then(|d| d.as_str()).unwrap_or("(no description)");
            let required: Vec<String> = s
                .get("required")
                .and_then(|r| r.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let props: Vec<String> = s
                .get("properties")
                .and_then(|p| p.as_object())
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| {
                            let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("any");
                            let pdesc = v.get("description").and_then(|d| d.as_str()).unwrap_or("");
                            let req = if required.contains(k) { " (required)" } else { "" };
                            format!("    {k}: {ty}{req} — {pdesc}")
                        })
                        .collect()
                })
                .unwrap_or_default();
            let props_str = props.join("\n");
            Some(format!("  {name}: {desc}\n{props_str}"))
        })
        .collect();
    let tool_docs_str = tool_docs.join("\n\n");
    format!(
        "you are InOut Agent (io), a minimal rust-native coding agent. you operate inside the \
         repo at {repo_root}. all file access is jailed to the repo root. be terse and direct.\n\n\
         available tools:\n{tool_docs_str}",
        tool_docs_str = tool_docs_str,
        repo_root = repo_root.display(),
    )
}

/// first-party compiled extensions loaded by the binary. feature-gated so
/// v1.0 builds with all v2 features disabled still link.
fn compiled_extensions() -> Vec<Box<dyn inout_core::Extension>> {
    vec![
        Box::new(inout_ext_skills::SkillsExtension),
        Box::new(inout_ext_sessions::SessionsExtension),
    ]
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use inout_testing::{scenario, then, when};
    use super::*;
    use crate::llm::ReplayLlmClient;

    fn ensure_extensions_dir() {
        std::env::set_var(
            "IO_EXTENSIONS_DIR",
            format!("{}/../../extensions", env!("CARGO_MANIFEST_DIR")),
        );
    }

    #[test]
    fn agent_has_default_system_prompt() {
        let mut s = scenario!("core", "Minimal configuration", "Config loads required fields");
        let dir = std::env::temp_dir();
        let cfg = Config { repo_root: dir.clone(), ..Config::default() };
        ensure_extensions_dir();
        let llm: Box<dyn LlmClient> = Box::new(ReplayLlmClient::new(vec![]));
        let mut agent = Agent::new(cfg, llm);
        agent.load_extensions();
        when!(s, "an agent is constructed and extensions are loaded", {
            assert!(agent.history.system_prompt.is_some());
            let prompt = agent.history.system_prompt.as_ref().unwrap();
            then!(s, "the default system prompt mentions inout and the core tools", {
                assert!(prompt.contains("InOut"));
                assert!(prompt.contains("read"));
                assert!(prompt.contains("write"));
                assert!(prompt.contains("bash"));
            });
        });
    }

    #[test]
    fn agent_system_prompt_in_request() {
        let mut s = scenario!("core", "Minimal configuration", "Config loads required fields");
        let dir = std::env::temp_dir();
        let cfg = Config { repo_root: dir, ..Config::default() };
        ensure_extensions_dir();
        let llm: Box<dyn LlmClient> = Box::new(ReplayLlmClient::new(vec![]));
        let mut agent = Agent::new(cfg, llm);
        agent.load_extensions();
        agent.history.append_user("hi".to_string());
        when!(s, "to_request is called on an agent with a loaded system prompt", {
            let req = agent.history.to_request("m", &[]);
            then!(s, "the system prompt is the first message and the user message follows", {
                assert_eq!(req.messages.len(), 2);
                assert!(req.messages[0].content.contains("InOut"));
                assert_eq!(req.messages[1].role, history::Role::User);
            });
        });
    }
}
