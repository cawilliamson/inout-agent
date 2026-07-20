//! inout sessions extension.
//!
//! durable session trees, branching, compaction and continuity handoff.

#![allow(missing_docs)]

pub mod commands;
pub mod compaction;
pub mod continuity;
pub mod entry;
pub mod jsonl_repo;
pub mod repo;

use std::sync::Arc;

use inout_core::extension::ExtensionApi;
use inout_core::Extension;

use crate::commands::{register_session_commands, CommandState};
use crate::compaction::CompactionSettings;
use crate::jsonl_repo::JsonlSessionRepo;

/// sessions extension entry point.
#[derive(Debug, Default)]
pub struct SessionsExtension;

impl SessionsExtension {
    /// create a new extension instance.
    pub fn new() -> Self {
        Self
    }
}

impl Extension for SessionsExtension {
    fn name(&self) -> &str {
        "sessions"
    }

    fn register(&self, api: &mut ExtensionApi) {
        (api.observe)(String::from("extension_loaded:sessions"));

        let repo = match JsonlSessionRepo::new_blocking(".inout/sessions") {
            Ok(repo) => Arc::new(repo),
            Err(e) => {
                (api.observe)(format!("sessions_repo_error:{e}"));
                return;
            }
        };

        let state = CommandState { repo, compaction: CompactionSettings::default() };

        register_session_commands(api, state);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn extension_name() {
        let ext = SessionsExtension::new();
        assert_eq!(ext.name(), "sessions");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn extension_registers_commands() {
        let ext = SessionsExtension::new();
        let mut api = ExtensionApi::noop();
        ext.register(&mut api);
        let names = api.commands.names();
        assert!(names.contains(&"sessions".to_string()));
        assert!(names.contains(&"branch".to_string()));
        assert!(names.contains(&"switch".to_string()));
        assert!(names.contains(&"compact".to_string()));
    }
}
