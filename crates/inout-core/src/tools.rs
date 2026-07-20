//! the tool trait and registry.
//!
//! tools are async, named, schema-described functions. the registry holds all
//! registered tools and the active subset the model may use in a turn.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::types::PermissionClass;

/// failure modes for a tool call.
#[derive(Debug, Error)]
pub enum ToolError {
    /// the arguments passed to the tool were malformed or missing.
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    /// the requested path broke out of the repo jail.
    #[error("jail violation: {0}")]
    Jail(String),
    /// an i/o error occurred while executing the tool.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// a shell command returned non-zero or timed out.
    #[error("command failed: {0}")]
    Command(String),
}

/// a tool call requested by the model.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    /// unique id used to correlate the result with the request.
    pub id: String,
    /// tool name, must match a registered tool.
    pub name: String,
    /// parsed json arguments.
    pub arguments: Value,
}

/// a tool that can be registered and dispatched by the agent loop.
#[async_trait]
pub trait Tool: Send + Sync {
    /// tool name as exposed to the model.
    fn name(&self) -> &str;
    /// json schema used in the provider request.
    fn schema(&self) -> Value;
    /// execute the tool with the parsed arguments.
    async fn run(&self, args: Value) -> Result<String, ToolError>;

    /// permission class for permission-manager decisions.
    fn permission_class(&self) -> PermissionClass {
        PermissionClass::Read
    }
    /// the filesystem path this call would affect, if known.
    fn affected_path(&self, _args: &Value) -> Option<std::path::PathBuf> {
        None
    }
    /// whether a failed call may safely be retried.
    fn retry_safe(&self) -> bool {
        false
    }
    /// whether the tool's output should be rendered inline in the chat transcript.
    fn shows_inline_output(&self) -> bool {
        true
    }
}

/// owns every registered tool and the active tool set.
#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    active: Vec<String>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tools", &self.tools.keys().collect::<Vec<_>>())
            .field("active", &self.active)
            .finish()
    }
}

impl ToolRegistry {
    /// create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// register a tool under its `name()` and add it to the active set.
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        let name = tool.name().to_string();
        self.tools.insert(name.clone(), Arc::new(tool));
        self.active.push(name);
    }

    /// remove a tool from the registry.
    pub fn unregister(&mut self, name: &str) {
        self.tools.remove(name);
        self.active.retain(|n| n != name);
    }

    /// set the active tool set. all names must be registered and unique.
    ///
    /// # errors
    ///
    /// returns an error if an unknown name is given or if duplicates exist.
    pub fn set_active(&mut self, names: Vec<String>) -> Result<(), ToolError> {
        let mut seen = HashSet::new();
        for name in &names {
            if !self.tools.contains_key(name) {
                return Err(ToolError::InvalidArgs(format!("unknown tool: {name}")));
            }
            if !seen.insert(name.clone()) {
                return Err(ToolError::InvalidArgs(format!("duplicate tool: {name}")));
            }
        }
        self.active = names;
        Ok(())
    }

    /// schemas for the active tools, in active order.
    #[must_use]
    pub fn active_schemas(&self) -> Vec<Value> {
        self.active.iter().filter_map(|name| self.tools.get(name).map(|t| t.schema())).collect()
    }

    /// schemas for the active tools. alias for `active_schemas`.
    #[must_use]
    pub fn schemas(&self) -> Vec<Value> {
        self.active_schemas()
    }

    /// dispatch a tool call by name.
    ///
    /// # errors
    ///
    /// returns `ToolError::InvalidArgs` if `name` is not registered, or the
    /// tool's own error if execution fails.
    pub async fn dispatch(&self, name: &str, args: Value) -> Result<String, ToolError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| ToolError::InvalidArgs(format!("unknown tool: {name}")))?;
        tool.run(args).await
    }

    /// dispatch a `ToolCall` struct.
    ///
    /// # errors
    ///
    /// returns `ToolError::InvalidArgs` if the tool name is not registered, or
    /// the tool's own error if execution fails.
    pub async fn dispatch_call(&self, call: &ToolCall) -> Result<String, ToolError> {
        self.dispatch(&call.name, call.arguments.clone()).await
    }

    /// every registered tool name.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.tools.keys().map(String::as_str).collect()
    }

    /// the active tool names.
    #[must_use]
    pub fn active_names(&self) -> &[String] {
        &self.active
    }
}
