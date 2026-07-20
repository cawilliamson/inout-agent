//! extension loading api.
//!
//! an extension is a compiled-in rust crate (first-party) or a rhai script
//! (user-facing). it registers tools, hooks, and providers against a shared
//! api object. v1.0 only defines the trait; the extension manager and rhai
//! loader land in v2.x.

use std::sync::Arc;

use crate::tools::ToolRegistry;
use crate::types::LlmRequest;
use std::collections::HashMap;

/// the api object passed to every extension during registration.
pub struct ExtensionApi {
    /// register tools for the agent loop.
    pub tools: ToolRegistry,
    /// registry of named tui views built by extensions.
    pub views: ViewRegistry,
    /// registry of slash commands registered by extensions.
    pub commands: CommandRegistry,
    /// emit an event onto the observability bus.
    pub observe: Arc<dyn Fn(String) + Send + Sync>,
    /// read-only hook to inspect an llm request before it leaves the agent.
    pub before_provider_payload: Arc<dyn Fn(&LlmRequest) + Send + Sync>,
}

impl std::fmt::Debug for ExtensionApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtensionApi")
            .field("tools", &self.tools)
            .field("views", &self.views)
            .field("commands", &self.commands)
            .field("observe", &"<dyn Fn>")
            .finish_non_exhaustive()
    }
}

impl ExtensionApi {
    /// create a no-op api for tests or when observability is disabled.
    pub fn noop() -> Self {
        Self {
            tools: ToolRegistry::new(),
            views: ViewRegistry::new(),
            commands: CommandRegistry::new(),
            observe: Arc::new(|_| {}),
            before_provider_payload: Arc::new(|_| {}),
        }
    }
}

/// every first-party extension implements this and is loaded by the binary.
pub trait Extension: Send + Sync {
    /// human-readable extension name.
    fn name(&self) -> &str;
    /// register capabilities against the api.
    fn register(&self, api: &mut ExtensionApi);
}

// ── tui view registry ────────────────────────────────────────────────────────

/// a rhai-backed tui view builder.
///
/// rhai scripts register named views via `api.register_view(name, title, fn)`.
/// the binary's tui invokes the builder with a conversation-snapshot map and
/// renders the returned view-spec map (see `spec/v2.10-scripting.md`).
/// scripts own the *computation* of what to show; rust owns rendering and
/// keyboard interaction. this keeps ratatui types out of the scripting surface.
#[derive(Clone)]
pub struct ViewBuilder {
    /// human-readable view title shown in the tui header.
    pub title: String,
    /// rhai fnptr that receives a conversation-snapshot map and returns a
    /// view-spec map. invoked through the owning engine/ast held by the
    /// script extension.
    pub builder: rhai::FnPtr,
    /// shared engine from the registering `ScriptExtension`.
    pub engine: std::sync::Arc<rhai::Engine>,
    /// parsed ast of the registering script.
    pub ast: rhai::AST,
}

impl std::fmt::Debug for ViewBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewBuilder")
            .field("title", &self.title)
            .field("builder", &self.builder.fn_name())
            .finish_non_exhaustive()
    }
}

/// a typed view-spec block, produced by converting the rhai builder's
/// returned map into a form the tui can render without holding rhai types.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum ViewBlock {
    /// user-authored text.
    UserText {
        /// text content.
        text: String,
        /// estimated token count.
        tokens: usize,
    },
    /// assistant-authored text.
    AssistantText {
        /// text content.
        text: String,
        /// estimated token count.
        tokens: usize,
    },
    /// a tool call the model requested.
    ToolCall {
        /// tool name.
        name: String,
        /// pretty-printed json input.
        input_json: String,
        /// estimated token count.
        tokens: usize,
    },
    /// a tool result returned to the model.
    ToolResult {
        /// tool name.
        tool_name: String,
        /// result content.
        content: String,
        /// estimated token count.
        tokens: usize,
    },
}

/// one turn in the viewer: a slice of the conversation plus its rendered blocks.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ViewTurn {
    /// index into the agent's message list where this turn starts.
    pub msg_index: usize,
    /// number of messages this turn spans (user + assistant + tool results).
    pub msg_count: usize,
    /// first ~60 chars of the user message, shown in the turn list.
    pub preview: String,
    /// estimated token cost for this turn.
    pub tokens_est: usize,
    /// whether this turn is within the active sliding window.
    pub in_window: bool,
    /// ordered content blocks for the detail pane.
    pub blocks: Vec<ViewBlock>,
}

/// a fully-resolved view spec ready for the tui to render.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ViewSpec {
    /// turn entries in conversation order.
    pub turns: Vec<ViewTurn>,
    /// total estimated tokens across all messages.
    pub total_tokens: usize,
    /// configured context limit in tokens.
    pub limit_tokens: usize,
    /// context fill percentage (0–100).
    pub context_pct: u8,
}

/// registry of named tui views built by rhai extensions.
#[derive(Clone, Default)]
pub struct ViewRegistry {
    views: std::collections::HashMap<String, ViewBuilder>,
}

impl std::fmt::Debug for ViewRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewRegistry")
            .field("views", &self.views.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ViewRegistry {
    /// create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// register a view builder under `name`. last-registered wins on conflict.
    pub fn register(&mut self, name: String, builder: ViewBuilder) {
        self.views.insert(name, builder);
    }

    /// look up a view builder by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ViewBuilder> {
        self.views.get(name)
    }

    /// registered view names, sorted alphabetically.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.views.keys().cloned().collect();
        names.sort();
        names
    }
}

// ── slash command registry ──────────────────────────────────────────────────

/// context passed to a slash command handler. rhai scripts receive this as a
/// map; rust handlers receive it directly.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CommandContext {
    /// current model name.
    pub model: String,
    /// current system prompt.
    pub system_prompt: String,
    /// raw arguments string after the command name (may be empty).
    pub args: String,
    /// conversation snapshot — same shape as view builder input.
    pub snapshot: serde_json::Value,
}

/// the outcome a command handler returns. the tui interprets the action and
/// the message is shown to the user.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CommandResult {
    /// human-readable text to display in the chat log.
    pub message: String,
    /// optional tui action for the agent to perform.
    pub action: Option<CommandAction>,
}

/// actions a slash command can request the tui/agent to perform.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum CommandAction {
    /// open a named view as an overlay.
    OpenView(String),
    /// clear conversation history.
    ClearHistory,
    /// switch the active model.
    SetModel(String),
    /// reload extensions.
    ReloadExtensions,
    /// exit the agent.
    Exit,
    /// append text as a user message and run a turn.
    RunTurn(String),
    /// drop the last conversation turn (user + assistant + tool results).
    UndoLastTurn,
}

/// a registered slash command. rhai scripts register name + handler fnptr;
/// rust extensions can implement `CommandHandler` directly.
#[derive(Clone)]
pub struct Command {
    /// command name without the leading `/` (e.g. `help`, `skill`).
    pub name: String,
    /// short description shown in `/help`.
    pub description: String,
    /// handler invoked with a `CommandContext`.
    pub handler: CommandHandler,
}

/// type-erased command handler. rhai-backed commands store an `FnPtr`; rust
/// commands store a closure. both produce a `CommandResult`.
#[derive(Clone)]
pub enum CommandHandler {
    /// a rhai function pointer + the engine/ast to call it against.
    Rhai {
        /// shared engine from the registering `ScriptExtension`.
        engine: std::sync::Arc<rhai::Engine>,
        /// parsed ast of the registering script.
        ast: rhai::AST,
        /// the handler fnptr.
        fn_ptr: rhai::FnPtr,
    },
    /// a rust closure.
    #[allow(clippy::type_complexity)]
    Rust(Arc<dyn Fn(&CommandContext) -> anyhow::Result<CommandResult> + Send + Sync>),
}

impl std::fmt::Debug for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Command")
            .field("name", &self.name)
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for CommandHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rhai { fn_ptr, .. } => f.debug_tuple("Rhai").field(fn_ptr).finish(),
            Self::Rust(_) => f.debug_tuple("Rust").field(&"<dyn Fn>").finish(),
        }
    }
}

/// registry of named slash commands built by extensions.
#[derive(Clone, Default)]
pub struct CommandRegistry {
    commands: HashMap<String, Command>,
}

impl std::fmt::Debug for CommandRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_set().entries(self.commands.keys()).finish()
    }
}

impl CommandRegistry {
    /// create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// register a command. last-registered wins on name conflict.
    pub fn register(&mut self, cmd: Command) {
        self.commands.insert(cmd.name.clone(), cmd);
    }

    /// look up a command by name (without leading `/`).
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Command> {
        self.commands.get(name)
    }

    /// registered command names, sorted alphabetically.
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.commands.keys().cloned().collect();
        names.sort();
        names
    }

    /// dispatch a command by name with the given context.
    pub fn dispatch(&self, name: &str, ctx: &CommandContext) -> anyhow::Result<CommandResult> {
        let cmd =
            self.commands.get(name).ok_or_else(|| anyhow::anyhow!("unknown command: /{name}"))?;
        match &cmd.handler {
            CommandHandler::Rhai { engine, ast, fn_ptr } => {
                let ctx_json = serde_json::to_value(ctx)?;
                let ctx_map = crate::scripting::json_to_map(&ctx_json);
                let result: rhai::Dynamic = fn_ptr
                    .call(engine, ast, (ctx_map,))
                    .map_err(|e| anyhow::anyhow!("rhai command error: {e}"))?;
                let map = result
                    .try_cast::<rhai::Map>()
                    .ok_or_else(|| anyhow::anyhow!("command handler did not return a map"))?;
                crate::scripting::map_to_command_result(&map)
            }
            CommandHandler::Rust(f) => f(ctx),
        }
    }
}
