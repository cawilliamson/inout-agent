//! `/skill` slash commands.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use inout_core::extension::{Command, CommandContext, CommandHandler, CommandResult, ExtensionApi};

use crate::loader::{load_all_skills, skill_dirs};
use crate::scope::detect_domain_scope;
use crate::skill::{Skill, SkillSource};
use crate::trace::SkillTrace;
use crate::trigger::match_skills_scoped;

/// shared state for command handlers.
#[derive(Clone, Debug, Default)]
pub struct CommandState {
    /// loaded skills.
    pub skills: Arc<RwLock<Vec<Skill>>>,
    /// active domain scope.
    pub domain_scope: Arc<RwLock<HashSet<String>>>,
    /// per-turn skill trace.
    pub trace: Arc<RwLock<SkillTrace>>,
}

/// register `/skill` subcommands on the api.
pub fn register_skill_commands(api: &mut ExtensionApi, state: CommandState) {
    let names = ["list", "show", "create", "log", "scope"];
    let handlers: Vec<CommandHandler> = vec![
        CommandHandler::Rust(Arc::new({
            let state = state.clone();
            move |ctx: &CommandContext| skill_list(ctx, &state)
        })),
        CommandHandler::Rust(Arc::new({
            let state = state.clone();
            move |ctx: &CommandContext| skill_show(ctx, &state)
        })),
        CommandHandler::Rust(Arc::new({
            let state = state.clone();
            move |ctx: &CommandContext| skill_create(ctx, &state)
        })),
        CommandHandler::Rust(Arc::new({
            let state = state.clone();
            move |ctx: &CommandContext| skill_log(ctx, &state)
        })),
        CommandHandler::Rust(Arc::new({
            let state = state.clone();
            move |ctx: &CommandContext| skill_scope(ctx, &state)
        })),
    ];

    for (name, handler) in names.iter().zip(handlers) {
        api.commands.register(Command {
            name: format!("skill {name}"),
            description: format!("skill command: {name}"),
            handler,
        });
    }
}

fn skill_list(_ctx: &CommandContext, state: &CommandState) -> anyhow::Result<CommandResult> {
    let skills = state.skills.read().map_err(|e| anyhow::anyhow!("skills lock poisoned: {e}"))?;
    let scope =
        state.domain_scope.read().map_err(|e| anyhow::anyhow!("scope lock poisoned: {e}"))?;
    let always_on: Vec<&Skill> = skills.iter().filter(|s| s.is_always_on()).collect();
    let triggered: Vec<&Skill> = match_skills_scoped("", &skills, &scope);

    let mut lines: Vec<String> = Vec::new();
    lines.push("always-on:".to_string());
    for s in always_on {
        lines.push(format!("  {} {}", source_glyph(s.source), s.name));
    }
    lines.push("triggered:".to_string());
    for s in triggered {
        lines.push(format!("  {} {}", source_glyph(s.source), s.name));
    }

    Ok(CommandResult { message: lines.join("\n"), action: None })
}

fn skill_show(ctx: &CommandContext, state: &CommandState) -> anyhow::Result<CommandResult> {
    let name = ctx.args.trim();
    if name.is_empty() {
        return Ok(CommandResult {
            message: "usage: /skill show <name>".to_string(),
            action: None,
        });
    }

    let skills = state.skills.read().map_err(|e| anyhow::anyhow!("skills lock poisoned: {e}"))?;
    let Some(skill) = skills.iter().find(|s| s.name.to_lowercase() == name.to_lowercase()) else {
        return Ok(CommandResult { message: format!("skill not found: {name}"), action: None });
    };

    let mut msg = format!("{}\n{}\n", skill.name, skill.description);
    msg.push_str(&format!("source: {:?}\ncategory: {:?}\n\n", skill.source, skill.category));
    msg.push_str(skill.content.trim());
    Ok(CommandResult { message: msg, action: None })
}

fn skill_create(ctx: &CommandContext, state: &CommandState) -> anyhow::Result<CommandResult> {
    let parts: Vec<&str> = ctx.args.split_whitespace().collect();
    if parts.is_empty() {
        return Ok(CommandResult {
            message: "usage: /skill create <name> [--global]".to_string(),
            action: None,
        });
    }

    let name = parts[0].to_lowercase();
    let global = parts.contains(&"--global");
    let dir = if global {
        home_dir()
            .map(|h| h.join(".inout").join("skills"))
            .ok_or_else(|| anyhow::anyhow!("could not resolve home directory"))?
    } else {
        PathBuf::from(".inout").join("skills")
    };

    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{name}.md"));
    if path.exists() {
        return Ok(CommandResult {
            message: format!("skill already exists: {}", path.display()),
            action: None,
        });
    }

    let content = format!(
        "---\nname: {name}\ncategory: domain\ntrigger: [\"{name}\"]\npriority: 0\n---\n\nskill body\n"
    );
    std::fs::write(&path, content)?;

    // refresh in-memory skill list so the new skill is immediately usable.
    let extra_dirs: Vec<PathBuf> = skill_dirs(&[]).into_iter().map(|(d, _)| d).collect();
    let refreshed = load_all_skills(&extra_dirs);
    state
        .skills
        .write()
        .map_err(|e| anyhow::anyhow!("skills lock poisoned: {e}"))?
        .clone_from(&refreshed);
    let scope: HashSet<String> = detect_domain_scope().into_iter().collect();
    state
        .domain_scope
        .write()
        .map_err(|e| anyhow::anyhow!("scope lock poisoned: {e}"))?
        .clone_from(&scope);

    Ok(CommandResult { message: format!("created skill at {}", path.display()), action: None })
}

fn skill_log(_ctx: &CommandContext, state: &CommandState) -> anyhow::Result<CommandResult> {
    let trace = state.trace.read().map_err(|e| anyhow::anyhow!("trace lock poisoned: {e}"))?;
    let _skills = state.skills.read().map_err(|e| anyhow::anyhow!("skills lock poisoned: {e}"))?;
    let entries = trace.all();
    if entries.is_empty() {
        return Ok(CommandResult {
            message: "no skill trace entries yet".to_string(),
            action: None,
        });
    }

    let mut lines: Vec<String> = Vec::new();
    for e in entries {
        let skills = e.matched_skills.join(", ");
        let reason = e.reason.as_deref().unwrap_or("matched");
        lines.push(format!("turn {}: {} [{}] — {reason}", e.turn, e.user_preview, skills));
    }
    Ok(CommandResult { message: lines.join("\n"), action: None })
}

fn skill_scope(_ctx: &CommandContext, state: &CommandState) -> anyhow::Result<CommandResult> {
    let mut names: Vec<String> = state
        .domain_scope
        .read()
        .map_err(|e| anyhow::anyhow!("scope lock poisoned: {e}"))?
        .iter()
        .cloned()
        .collect();
    names.sort();
    let msg = if names.is_empty() {
        "no active domain scope".to_string()
    } else {
        format!("active domain scope: {}", names.join(", "))
    };
    Ok(CommandResult { message: msg, action: None })
}

fn source_glyph(source: SkillSource) -> &'static str {
    match source {
        SkillSource::Bundled => "b",
        SkillSource::Global => "g",
        SkillSource::External => "e",
        SkillSource::Project => "p",
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").map(PathBuf::from).ok()
}

#[cfg(test)]
mod tests {
    use inout_testing::{scenario, then, when};
    use super::*;

    #[test]
    fn source_glyph_mapping() {
        let mut s = scenario!("skills", "Skill source tiers", "Bundled source for compiled-in defaults");
        when!(s, "source_glyph is called for each source tier", {});
        then!(s, "each tier maps to its single-character glyph", {
            assert_eq!(source_glyph(SkillSource::Bundled), "b");
            assert_eq!(source_glyph(SkillSource::Project), "p");
        });
    }

    #[test]
    fn skill_show_not_found() {
        let mut s = scenario!("skills", "Skill commands", "`/skill show <name>` previews a skill");
        let state = CommandState::default();
        let ctx = CommandContext {
            model: String::new(),
            system_prompt: String::new(),
            args: String::from("missing"),
            snapshot: serde_json::Value::Null,
        };
        when!(s, "skill_show runs for a name not present in state", {
            let result = skill_show(&ctx, &state).unwrap();
            then!(s, "the result message reports the skill was not found", {
                assert!(result.message.contains("not found"));
            });
        });
    }
}
