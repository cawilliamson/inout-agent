//! core data types shared across the agent loop, providers, and extensions.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// a single block of content within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentBlock {
    /// plain text.
    Text {
        /// the text content.
        text: String,
    },
    /// a tool call requested by the model.
    ToolUse {
        /// unique id used to correlate the result.
        id: String,
        /// tool name.
        name: String,
        /// parsed json input.
        input: Value,
    },
    /// the result of a tool call, returned to the model.
    ToolResult {
        /// the tool call id this result answers.
        tool_use_id: String,
        /// result content.
        content: String,
        /// whether this result represents an error.
        is_error: bool,
    },
}

/// a conversation message: role + ordered content blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// who produced the message.
    pub role: Role,
    /// ordered content blocks.
    pub content: Vec<ContentBlock>,
}

impl Message {
    /// build a user message from a plain text string.
    pub fn user(text: impl Into<String>) -> Self {
        Self { role: Role::User, content: vec![ContentBlock::Text { text: text.into() }] }
    }

    /// build an assistant message from a plain text string.
    pub fn assistant(text: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: vec![ContentBlock::Text { text: text.into() }] }
    }
}

/// who produced a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    /// the user.
    User,
    /// the assistant / model.
    Assistant,
    /// a tool result message.
    Tool,
}

/// token usage for a single llm response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    /// input tokens consumed.
    pub input_tokens: u64,
    /// output tokens produced.
    pub output_tokens: u64,
    /// cache-read tokens.
    pub cache_read_tokens: u64,
    /// cache-write tokens.
    pub cache_write_tokens: u64,
}

/// a request to an llm provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    /// model identifier.
    pub model: String,
    /// conversation messages.
    pub messages: Vec<Message>,
    /// tool schemas available this turn.
    pub tools: Vec<Value>,
    /// optional system prompt.
    pub system: Option<String>,
}

/// a response from an llm provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    /// the assistant message.
    pub message: Message,
    /// token usage.
    pub usage: Usage,
    /// stop reason if provided.
    pub stop_reason: Option<String>,
}

/// the permission class a tool belongs to. used by the permissions extension
/// (v2.4) to decide whether to prompt, auto-approve, or deny.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionClass {
    /// read-only filesystem access.
    Read,
    /// mutating filesystem access.
    Write,
    /// run a shell command.
    Shell,
    /// spawn a subprocess or agent.
    Spawn,
    /// network access.
    Network,
}

/// the path a tool will affect, if any. lets hooks (e.g. jail) inspect the
/// target before execution.
pub type AffectedPath = PathBuf;
