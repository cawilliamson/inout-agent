//! session entry tree types.

use serde::{Deserialize, Serialize};

/// base fields carried by every session entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryBase {
    /// unique entry identifier.
    pub id: String,
    /// parent entry identifier, if any.
    pub parent_id: Option<String>,
    /// unix timestamp in milliseconds.
    pub timestamp: u64,
}

/// discriminated union of all durable session entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEntry {
    /// a conversation message.
    Message(MessageEntry),
    /// a model change event.
    ModelChange(ModelChangeEntry),
    /// a thinking level change event.
    ThinkingLevelChange(ThinkingLevelChangeEntry),
    /// an active tools change event.
    ActiveToolsChange(ActiveToolsChangeEntry),
    /// a compaction summary entry.
    Compaction(CompactionEntry),
    /// a branch summary entry.
    BranchSummary(BranchSummaryEntry),
    /// a custom entry.
    Custom(CustomEntry),
    /// a custom message entry.
    CustomMessage(CustomMessageEntry),
    /// a labelled marker entry.
    Label(LabelEntry),
    /// session metadata entry.
    SessionInfo(SessionInfoEntry),
    /// a leaf pointer entry.
    Leaf(LeafEntry),
}

/// a conversation message entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEntry {
    /// base entry fields.
    #[serde(flatten)]
    pub base: EntryBase,
    /// message role.
    pub role: String,
    /// message content.
    pub content: String,
}

/// a model change entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelChangeEntry {
    /// base entry fields.
    #[serde(flatten)]
    pub base: EntryBase,
    /// new model name.
    pub model: String,
}

/// a thinking level change entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingLevelChangeEntry {
    /// base entry fields.
    #[serde(flatten)]
    pub base: EntryBase,
    /// new thinking level.
    pub level: String,
}

/// an active tools change entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveToolsChangeEntry {
    /// base entry fields.
    #[serde(flatten)]
    pub base: EntryBase,
    /// enabled tool names.
    pub tools: Vec<String>,
}

/// a compaction summary entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionEntry {
    /// base entry fields.
    #[serde(flatten)]
    pub base: EntryBase,
    /// number of messages compacted.
    pub compacted_count: usize,
    /// summary text replacing compacted messages.
    pub summary: String,
}

/// a branch summary entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchSummaryEntry {
    /// base entry fields.
    #[serde(flatten)]
    pub base: EntryBase,
    /// branch name.
    pub branch: String,
    /// summary text.
    pub summary: String,
}

/// a custom entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomEntry {
    /// base entry fields.
    #[serde(flatten)]
    pub base: EntryBase,
    /// custom entry kind.
    pub kind: String,
    /// payload.
    pub payload: serde_json::Value,
}

/// a custom message entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomMessageEntry {
    /// base entry fields.
    #[serde(flatten)]
    pub base: EntryBase,
    /// message role.
    pub role: String,
    /// custom message kind.
    pub kind: String,
    /// payload.
    pub payload: serde_json::Value,
}

/// a labelled marker entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelEntry {
    /// base entry fields.
    #[serde(flatten)]
    pub base: EntryBase,
    /// label text.
    pub label: String,
}

/// session metadata entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfoEntry {
    /// base entry fields.
    #[serde(flatten)]
    pub base: EntryBase,
    /// session title.
    pub title: String,
    /// session goal.
    pub goal: String,
}

/// a leaf pointer entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeafEntry {
    /// base entry fields.
    #[serde(flatten)]
    pub base: EntryBase,
    /// leaf entry id.
    pub leaf_id: String,
}
