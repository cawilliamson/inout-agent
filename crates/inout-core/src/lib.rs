//! inout core substrate.
//!
//! the substrate the agent loop, the data types, the tool trait + registry,
//! the extension trait + api, the jail, minimal config, and a minimal hook bus.
//! no tools, no llm client, no persistence, no observability. all extensions.

pub mod config;
pub mod extension;
pub mod hooks;
pub mod jail;
pub mod scripting;
pub mod tools;
pub mod types;

pub use config::{BashConfig, Config, ScriptConfig};
pub use extension::{
    Command, CommandAction, CommandContext, CommandHandler, CommandRegistry, CommandResult,
    Extension, ExtensionApi, ViewBlock, ViewBuilder, ViewRegistry, ViewSpec, ViewTurn,
};
pub use hooks::HookBus;
pub use jail::{Jail, JailError};
pub use scripting::{
    build_view, load_script_extensions, map_to_command_result, ScriptExtension, ScriptPermissions,
    ScriptTool,
};
pub use tools::{Tool, ToolCall, ToolError, ToolRegistry};
pub use types::{ContentBlock, LlmRequest, LlmResponse, Message, PermissionClass, Role, Usage};
