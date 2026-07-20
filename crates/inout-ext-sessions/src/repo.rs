//! session repository trait.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::entry::SessionEntry;

/// options for forking a branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForkOptions {
    /// name for the new branch.
    pub name: String,
}

/// options for listing sessions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListOptions {
    /// maximum number of sessions to return.
    pub limit: Option<usize>,
    /// only return sessions matching the given prefix.
    pub prefix: Option<String>,
}

/// lightweight session summary returned by list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// branch or session id.
    pub id: String,
    /// human-readable title.
    pub title: String,
    /// current leaf id.
    pub leaf_id: String,
}

/// resulting leaf after a navigation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigateTreeResult {
    /// new active leaf id.
    pub leaf_id: String,
    /// entries visited along the path from root to leaf.
    pub path: Vec<SessionEntry>,
}

/// runtime context built from a leaf back to root.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionContext {
    /// messages in root-to-leaf order after applying model/tool/compaction entries.
    pub messages: Vec<MessageContext>,
    /// active model name.
    pub model: Option<String>,
    /// active tool names.
    pub tools: Vec<String>,
}

/// message in the resolved session context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageContext {
    /// message role.
    pub role: String,
    /// message content.
    pub content: String,
}

/// metadata for the current session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionMetadata {
    /// active leaf id.
    pub leaf_id: String,
    /// branch name.
    pub branch: String,
}

/// persistent session storage.
#[async_trait]
pub trait SessionRepo: Send + Sync {
    /// append an entry durably.
    async fn append_entry(&self, entry: SessionEntry) -> Result<()>;

    /// fork the tree at `target_id` into a new branch.
    async fn fork(&self, target_id: &str, opts: ForkOptions) -> Result<String>;

    /// list available sessions or branches.
    async fn list(&self, opts: ListOptions) -> Result<Vec<SessionInfo>>;

    /// navigate from one entry to another.
    async fn navigate_tree(&self, from: &str, to: &str) -> Result<NavigateTreeResult>;

    /// build a runnable context from `leaf_id`.
    async fn build_context(&self, leaf_id: &str) -> Result<SessionContext>;

    /// set the active leaf id.
    async fn set_leaf_id(&self, target_id: &str) -> Result<()>;

    /// read session metadata.
    async fn get_metadata(&self) -> Result<SessionMetadata>;
}
