//! inout skills extension.
//!
//! markdown skill files with yaml frontmatter, trigger matching, and system
//! prompt injection.

#![allow(missing_docs)]
#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod budget;
pub mod commands;
pub mod loader;
pub mod scope;
pub mod skill;
pub mod trace;
pub mod trigger;

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use inout_core::extension::ExtensionApi;
use inout_core::Extension;

use crate::commands::{register_skill_commands, CommandState};
use crate::loader::load_all_skills;
use crate::scope::detect_domain_scope;

/// skills extension entry point.
#[derive(Debug, Default)]
pub struct SkillsExtension;

impl SkillsExtension {
    /// create a new extension instance.
    pub fn new() -> Self {
        Self
    }
}

impl Extension for SkillsExtension {
    fn name(&self) -> &str {
        "skills"
    }

    fn register(&self, api: &mut ExtensionApi) {
        (api.observe)(String::from("extension_loaded:skills"));

        let skills = load_all_skills(&[]);
        let domain_scope: HashSet<String> = detect_domain_scope().into_iter().collect();

        let state = CommandState {
            skills: Arc::new(RwLock::new(skills)),
            domain_scope: Arc::new(RwLock::new(domain_scope)),
            trace: Arc::new(RwLock::new(crate::trace::SkillTrace::new())),
        };

        register_skill_commands(api, state);
    }
}

#[cfg(test)]
mod tests {
    use inout_testing::{scenario, then, when};
    use super::*;

    #[test]
    fn extension_name_and_registration() {
        let mut s = scenario!("extensions", "Rust extension can register multiple surface items", "Single extension registers tool, command, and hook");
        let ext = SkillsExtension::new();
        when!(s, "the skills extension is registered with an extension api", {
            assert_eq!(ext.name(), "skills");
            let mut api = ExtensionApi::noop();
            ext.register(&mut api);
            then!(s, "the extension name is skills and slash commands are registered", {
                assert!(!api.commands.names().is_empty());
            });
        });
    }
}
