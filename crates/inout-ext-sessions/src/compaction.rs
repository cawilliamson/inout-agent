//! session compaction: summarise older messages while keeping recent ones.

use crate::entry::{CompactionEntry, EntryBase, SessionEntry};
use crate::repo::SessionContext;

/// settings controlling compaction behaviour.
#[derive(Debug, Clone, Copy)]
pub struct CompactionSettings {
    /// maximum number of messages before compaction triggers.
    pub max_messages: usize,
    /// number of recent messages to keep verbatim.
    pub keep_recent: usize,
    /// whether to produce a summary of older messages.
    pub summarize_older: bool,
}

impl Default for CompactionSettings {
    fn default() -> Self {
        Self { max_messages: 64, keep_recent: 16, summarize_older: true }
    }
}

/// event emitted before compaction, allowing cancellation.
#[derive(Debug, Clone)]
pub struct SessionBeforeCompact {
    /// number of messages that will be compacted.
    pub count: usize,
    /// set to true to abort compaction.
    pub cancelled: bool,
}

/// event emitted after compaction completes.
#[derive(Debug, Clone)]
pub struct SessionCompact {
    /// number of messages compacted.
    pub compacted_count: usize,
    /// generated summary text.
    pub summary: String,
}

/// compact a session context into a summary entry.
///
/// returns a pair of events and the new compacted entry. callers must append
/// the entry to the repo themselves.
pub fn compact(
    settings: &CompactionSettings,
    leaf_id: &str,
    ctx: &SessionContext,
) -> anyhow::Result<(SessionBeforeCompact, SessionCompact, SessionEntry)> {
    if ctx.messages.len() <= settings.max_messages {
        return Err(anyhow::anyhow!("message count below compaction threshold"));
    }

    let to_compact = ctx.messages.len().saturating_sub(settings.keep_recent);
    let summary = if settings.summarize_older {
        generate_summary(&ctx.messages[..to_compact])
    } else {
        format!("{} earlier messages omitted", to_compact)
    };

    let before = SessionBeforeCompact { count: to_compact, cancelled: false };
    let after = SessionCompact { compacted_count: to_compact, summary: summary.clone() };

    let entry = SessionEntry::Compaction(CompactionEntry {
        base: EntryBase {
            id: uuid::Uuid::new_v4().to_string(),
            parent_id: Some(leaf_id.to_string()),
            timestamp: now(),
        },
        compacted_count: to_compact,
        summary,
    });

    Ok((before, after, entry))
}

fn generate_summary(messages: &[crate::repo::MessageContext]) -> String {
    let roles: std::collections::HashSet<&str> = messages.iter().map(|m| m.role.as_str()).collect();
    let topics: Vec<String> = messages
        .iter()
        .filter(|m| !m.content.is_empty())
        .map(|m| m.content.split_whitespace().take(6).collect::<Vec<_>>().join(" "))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .take(5)
        .collect();

    let mut parts = Vec::new();
    parts.push(format!("{} messages summarised", messages.len()));
    if !roles.is_empty() {
        parts.push(format!("roles: {}", roles.into_iter().collect::<Vec<_>>().join(", ")));
    }
    if !topics.is_empty() {
        parts.push(format!("topics: {}", topics.join("; ")));
    }

    parts.join("; ")
}

fn now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use inout_testing::{scenario, then, when};
    use super::*;

    #[test]
    fn compact_threshold_respected() {
        let mut s = scenario!(
            "sessions",
            "Compaction emits before/after events",
            "Older messages replaced by summary"
        );
        let ctx = SessionContext {
            messages: (0..10)
                .map(|i| crate::repo::MessageContext {
                    role: "user".to_string(),
                    content: format!("msg {i}"),
                })
                .collect(),
            ..SessionContext::default()
        };

        when!(s, "compact runs against a session below the message threshold", {
            let result = compact(&CompactionSettings::default(), "leaf", &ctx);
            then!(s, "the call returns an error without compacting", {
                assert!(result.is_err());
            });
        });
    }

    #[test]
    fn compact_produces_summary() {
        let mut s = scenario!(
            "sessions",
            "Compaction emits before/after events",
            "Older messages replaced by summary"
        );
        let ctx = SessionContext {
            messages: (0..80)
                .map(|i| crate::repo::MessageContext {
                    role: if i % 2 == 0 { "user".to_string() } else { "assistant".to_string() },
                    content: format!("message number {i}"),
                })
                .collect(),
            ..SessionContext::default()
        };

        when!(s, "compact runs against a session exceeding the message threshold", {
            let (before, after, entry) = compact(&CompactionSettings::default(), "leaf", &ctx).unwrap();
            then!(s, "a compaction entry is appended covering the older messages", {
                assert_eq!(before.count, 64);
                assert_eq!(after.compacted_count, 64);
                assert!(matches!(entry, SessionEntry::Compaction(_)));
            });
        });
    }
}
