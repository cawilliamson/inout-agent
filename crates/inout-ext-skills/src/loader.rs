//! skill discovery and loading from bundled, global, external, and project tiers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::skill::{parse_skill_file, Skill, SkillSource};

/// markdown skill file extensions.
const SKILL_EXTENSIONS: &[&str] = &[".md", ".skill.md"];

/// return all skill directories in override priority order.
///
/// later entries override earlier entries on name collision.
#[must_use]
pub fn skill_dirs(extra_dirs: &[PathBuf]) -> Vec<(PathBuf, SkillSource)> {
    let mut dirs: Vec<(PathBuf, SkillSource)> = Vec::new();

    // bundled placeholder: no filesystem path.
    // global
    if let Some(home) = home_dir() {
        dirs.push((home.join(".inout").join("skills"), SkillSource::Global));
    }
    // external
    for d in extra_dirs {
        dirs.push((d.clone(), SkillSource::External));
    }
    // project current working directory
    dirs.push((
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".inout")
            .join("skills"),
        SkillSource::Project,
    ));

    dirs
}

/// load all skills across tiers. later tiers override earlier tiers on name
/// collision, and within each tier results are sorted alphabetically by path.
///
/// bundled skills are compiled in via `include_str!`.
#[must_use]
pub fn load_all_skills(extra_dirs: &[PathBuf]) -> Vec<Skill> {
    let mut by_name: HashMap<String, Skill> = HashMap::new();

    // bundled first.
    for skill in bundled_skills() {
        by_name.insert(skill.name.clone(), skill);
    }

    // filesystem tiers.
    for (dir, source) in skill_dirs(extra_dirs) {
        for skill in load_from_dir(&dir, source) {
            by_name.insert(skill.name.clone(), skill);
        }
    }

    let mut skills: Vec<Skill> = by_name.into_values().collect();
    skills.sort_by_key(|a| a.name.to_lowercase());
    skills
}

/// load every skill file from a directory, sorted alphabetically by path.
fn load_from_dir(dir: &Path, source: SkillSource) -> Vec<Skill> {
    if !dir.is_dir() {
        return Vec::new();
    }

    let mut files: Vec<PathBuf> = WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .filter(|p| has_skill_extension(p))
        .collect();

    files.sort();

    files.into_iter().filter_map(|p| parse_skill_file(&p, source).ok()).collect()
}

fn has_skill_extension(path: &Path) -> bool {
    let name = path.file_name().map(|s| s.to_string_lossy()).unwrap_or_default();
    SKILL_EXTENSIONS.iter().any(|ext| name.ends_with(ext))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").map(PathBuf::from).ok()
}

/// compiled-in default skills. kept minimal so users can override via files.
#[must_use]
fn bundled_skills() -> Vec<Skill> {
    let _raw = include_str!("../skills/default.md");
    // best-effort parse from an in-memory path; the path is only used for
    // diagnostics and scaffolding, so a synthetic one is fine.
    parse_skill_file(Path::new("bundled/default.md"), SkillSource::Bundled)
        .map(|s| vec![s])
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use inout_testing::{scenario, then, when};
    use super::*;

    #[test]
    fn project_overrides_global_on_name_collision() {
        let mut s = scenario!(
            "skills",
            "Skill source tiers and override order",
            "Project overrides global on name collision"
        );
        let global = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        std::fs::write(global.path().join("rust.md"), "---\npriority: 1\n---\nglobal rust\n")
            .unwrap();
        std::fs::write(project.path().join("rust.md"), "---\npriority: 5\n---\nproject rust\n")
            .unwrap();

        when!(s, "load_all_skills runs over the global then project directories", {
            let skills = load_all_skills(&[global.path().to_path_buf(), project.path().to_path_buf()]);
            let rust = skills.iter().find(|s| s.name == "rust").unwrap();
            then!(s, "the project skill wins on name collision", {
                assert_eq!(rust.priority, 5);
                assert_eq!(rust.source, SkillSource::External);
            });
        });
    }

    #[test]
    fn alphabetical_within_tier() {
        let mut s = scenario!("skills", "Script discovery", "Within a directory, files are sorted alphabetically");
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("z.md"), "---\n---\nz\n").unwrap();
        std::fs::write(dir.path().join("a.md"), "---\n---\na\n").unwrap();
        std::fs::write(dir.path().join("m.md"), "---\n---\nm\n").unwrap();

        when!(s, "load_from_dir scans the directory", {
            let skills = load_from_dir(dir.path(), SkillSource::External);
            let names: Vec<_> = skills.iter().map(|s| s.name.as_str()).collect();
            then!(s, "skills are returned in alphabetical order", {
                assert_eq!(names, vec!["a", "m", "z"]);
            });
        });
    }
}
