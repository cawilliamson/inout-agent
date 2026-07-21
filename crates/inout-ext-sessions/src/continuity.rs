//! continuity handoff files between sessions.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Local;
use tokio::io::AsyncWriteExt;

/// paths to the four continuity files.
#[derive(Debug, Clone)]
pub struct ContinuityFiles {
    /// project overview file.
    pub inout_md: PathBuf,
    /// project understanding file.
    pub understanding: PathBuf,
    /// last-session handoff file.
    pub context: PathBuf,
    /// dated session history file.
    pub session_log: PathBuf,
}

/// context loaded from the handoff file.
#[derive(Debug, Clone, Default)]
pub struct HandoffContext {
    /// goal of the last session.
    pub goal: String,
    /// files touched in the last session.
    pub files: Vec<String>,
    /// concrete next steps.
    pub next_steps: Vec<String>,
}

impl ContinuityFiles {
    /// create file paths rooted at `project_root`.
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        let root = project_root.as_ref().to_path_buf();
        let dot_inout = root.join(".inout");
        Self {
            inout_md: root.join("INOUT.md"),
            understanding: dot_inout.join("understanding.md"),
            context: dot_inout.join("context.md"),
            session_log: dot_inout.join("session_log.md"),
        }
    }

    /// write the handoff file summarising the last session.
    ///
    /// this implementation does not call an llm; it extracts next steps from
    /// the provided session summary directly.
    pub async fn write_handoff(&self, session: &crate::repo::SessionContext) -> Result<()> {
        tokio::fs::create_dir_all(self.context.parent().expect("context has parent"))
            .await
            .context("create .inout directory")?;

        let goal = session.messages.last().map(|m| m.content.clone()).unwrap_or_default();

        let files: Vec<String> = session
            .messages
            .iter()
            .filter(|m| m.role == "system")
            .flat_map(|m| extract_files(&m.content))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let next_steps = next_steps_from_messages(&session.messages);

        let mut body = String::new();
        body.push_str("# last session handoff\n\n");
        body.push_str("## goal\n\n");
        body.push_str(&goal);
        body.push_str("\n\n## files touched\n\n");
        if files.is_empty() {
            body.push_str("_none recorded_\n");
        } else {
            for f in files {
                body.push_str(&format!("- {f}\n"));
            }
        }
        body.push_str("\n## what next\n\n");
        if next_steps.is_empty() {
            body.push_str("_no next steps recorded_\n");
        } else {
            for step in next_steps {
                body.push_str(&format!("- {step}\n"));
            }
        }

        tokio::fs::write(&self.context, body).await.context("write context.md")?;
        Ok(())
    }

    /// load handoff context if the file exists.
    pub async fn load_handoff(&self) -> Option<HandoffContext> {
        let content = tokio::fs::read_to_string(&self.context).await.ok()?;
        Some(parse_handoff(&content))
    }

    /// append a dated entry to the session log.
    pub async fn write_session_log_entry(&self, goal: &str, files: &[String]) -> Result<()> {
        tokio::fs::create_dir_all(self.session_log.parent().expect("session_log has parent"))
            .await
            .context("create .inout directory")?;

        let date = Local::now().format("%Y-%m-%d").to_string();
        let mut line = format!("- **{date}**: {goal}");
        if !files.is_empty() {
            let joined = files.join(", ");
            line.push_str(&format!(" ({joined})"));
        }
        line.push('\n');

        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.session_log)
            .await?
            .write_all(line.as_bytes())
            .await
            .context("append session_log")?;
        Ok(())
    }
}

fn parse_handoff(content: &str) -> HandoffContext {
    let mut ctx = HandoffContext::default();
    let mut section: Option<&str> = None;

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(stripped) = line.strip_prefix("## ") {
            section = Some(stripped);
            continue;
        }
        if line.starts_with("# ") {
            continue;
        }
        let item = line.strip_prefix("- ").unwrap_or(line).to_string();

        match section {
            Some("goal") => ctx.goal.push_str(&item),
            Some("files touched") => ctx.files.push(item),
            Some("what next") => ctx.next_steps.push(item),
            _ => {}
        }
    }

    ctx
}

fn extract_files(content: &str) -> Vec<String> {
    let mut files = Vec::new();
    for word in content.split_whitespace() {
        let trimmed = word.trim_matches(|c: char| {
            c == '"' || c == '`' || c == '(' || c == ')' || c == ',' || c == '.'
        });
        if trimmed.contains('/') || trimmed.contains('.') {
            files.push(trimmed.to_string());
        }
    }
    files
}

fn next_steps_from_messages(messages: &[crate::repo::MessageContext]) -> Vec<String> {
    let recent: Vec<&crate::repo::MessageContext> = messages.iter().rev().take(10).collect();
    let mut steps = Vec::new();

    for m in &recent {
        for sentence in m.content.split(['.', '!', '?']) {
            let trimmed = sentence.trim();
            if trimmed.starts_with("next ")
                || trimmed.starts_with("then ")
                || trimmed.starts_with("need to ")
            {
                steps.push(trimmed.to_string());
            }
        }
    }

    if steps.is_empty() && !recent.is_empty() {
        if let Some(last) = recent.first() {
            let summary: String =
                last.content.split_whitespace().take(12).collect::<Vec<_>>().join(" ");
            if !summary.is_empty() {
                steps.push(format!("continue: {summary}"));
            }
        }
    }

    steps.truncate(3);
    steps
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use inout_testing::{scenario, then, when};
    use super::*;

    #[tokio::test]
    async fn write_handoff_round_trip() {
        let mut s = scenario!(
            "sessions",
            "Continuity handoff writes context.md",
            "Write handoff"
        );
        let tmp = tempfile::tempdir().unwrap();
        let files = ContinuityFiles::new(tmp.path());
        let session = crate::repo::SessionContext {
            messages: vec![
                crate::repo::MessageContext {
                    role: "user".to_string(),
                    content: "fix parser".to_string(),
                },
                crate::repo::MessageContext {
                    role: "system".to_string(),
                    content: "touched src/parser.rs and src/lexer.rs".to_string(),
                },
                crate::repo::MessageContext {
                    role: "assistant".to_string(),
                    content: "next we need to update tests".to_string(),
                },
            ],
            ..crate::repo::SessionContext::default()
        };

        when!(s, "write_handoff is called with a multi-message session", {
            files.write_handoff(&session).await.unwrap();
            let loaded = files.load_handoff().await.unwrap();
            then!(s, "the handoff goal, touched files, and next steps are populated", {
                assert!(!loaded.goal.is_empty());
                assert_eq!(loaded.files.len(), 2);
                assert_eq!(loaded.next_steps.len(), 1);
            });
        });
    }

    #[tokio::test]
    async fn session_log_appends_entry() {
        let mut s = scenario!(
            "sessions",
            "Continuity handoff loads on demand",
            "Continuity files are read lazily"
        );
        let tmp = tempfile::tempdir().unwrap();
        let files = ContinuityFiles::new(tmp.path());
        when!(s, "write_session_log_entry appends a goal and touched files", {
            files.write_session_log_entry("refactor auth", &["src/auth.rs".to_string()]).await.unwrap();
            let content = tokio::fs::read_to_string(&files.session_log).await.unwrap();
            then!(s, "the session log contains the goal and the file path", {
                assert!(content.contains("refactor auth"));
                assert!(content.contains("src/auth.rs"));
            });
        });
    }
}
