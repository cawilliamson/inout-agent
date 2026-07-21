//! skill definition and frontmatter parsing.

use std::path::PathBuf;

use serde::Deserialize;

/// skill lifetime category.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SkillCategory {
    /// always injected into the system prompt.
    Core,
    /// trigger candidate in every session.
    Practice,
    /// trigger candidate only when its stack is in scope.
    #[default]
    Domain,
}

impl SkillCategory {
    /// true for core skills that are always on.
    #[must_use]
    pub fn is_always_on(self) -> bool {
        matches!(self, Self::Core)
    }
}

impl<'de> Deserialize<'de> for SkillCategory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_ascii_lowercase().as_str() {
            "core" => Ok(Self::Core),
            "practice" => Ok(Self::Practice),
            "domain" => Ok(Self::Domain),
            _ => Err(serde::de::Error::custom(format!("unknown skill category: {s}"))),
        }
    }
}

/// skill source tier. higher tiers override lower ones on name collision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillSource {
    /// compiled into the extension.
    Bundled,
    /// from `~/.inout/skills/`.
    Global,
    /// from configured external paths.
    External,
    /// from `.inout/skills/` in the project.
    Project,
}

impl SkillSource {
    /// numeric rank for sorting: larger wins on collision and sorts first.
    #[must_use]
    pub fn tier(self) -> u8 {
        match self {
            Self::Bundled => 0,
            Self::Global => 1,
            Self::External => 2,
            Self::Project => 3,
        }
    }
}

/// raw frontmatter as read from a markdown skill file.
#[derive(Debug, Default, Deserialize)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
    category: Option<SkillCategory>,
    trigger: Option<Vec<String>>,
    priority: Option<i32>,
    tokens: Option<usize>,
}

/// a loaded skill.
#[derive(Debug, Clone)]
pub struct Skill {
    /// unique name.
    pub name: String,
    /// one-line description.
    pub description: String,
    /// lifetime category.
    pub category: SkillCategory,
    /// source tier.
    pub source: SkillSource,
    /// trigger words or phrases.
    pub triggers: Vec<String>,
    /// priority used for ranking.
    pub priority: i32,
    /// explicit token estimate from frontmatter, zero if unset.
    pub token_estimate: usize,
    /// markdown body after frontmatter.
    pub content: String,
    /// absolute path of the source file.
    pub file_path: PathBuf,
}

impl Skill {
    /// true for core skills that are always on.
    #[must_use]
    pub fn is_always_on(&self) -> bool {
        self.category.is_always_on()
    }

    /// token estimate for budgeting.
    #[must_use]
    pub fn tokens(&self) -> usize {
        if self.token_estimate > 0 {
            self.token_estimate
        } else {
            self.content.len().div_ceil(4)
        }
    }
}

/// parse a skill file into a `Skill`.
///
/// # errors
///
/// returns an error if the frontmatter is invalid yaml or if the file lacks a
/// determinable name.
pub fn parse_skill_file(path: &std::path::Path, source: SkillSource) -> anyhow::Result<Skill> {
    let raw = std::fs::read_to_string(path)?;
    let (front, body) = split_frontmatter(&raw);
    let fm: Frontmatter =
        if front.is_empty() { Frontmatter::default() } else { serde_yaml::from_str(front)? };

    let file_stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();

    let name = fm.name.unwrap_or(file_stem);

    // default category: practice if there is a description, otherwise domain.
    let category = fm.category.unwrap_or_else(|| {
        if fm.description.as_ref().is_some_and(|d| !d.is_empty()) {
            SkillCategory::Practice
        } else {
            SkillCategory::Domain
        }
    });

    let triggers = if let Some(t) = fm.trigger {
        t
    } else {
        // foreign skills without explicit triggers activate on their name.
        vec![name.clone()]
    };

    Ok(Skill {
        name,
        description: fm.description.unwrap_or_default(),
        category,
        source,
        triggers,
        priority: fm.priority.unwrap_or(0),
        token_estimate: fm.tokens.unwrap_or(0),
        content: body.to_string(),
        file_path: path.to_path_buf(),
    })
}

/// split raw markdown into `(frontmatter_yaml, body)`. frontmatter must start
/// with `---` on its own line and be closed by a matching line.
fn split_frontmatter(raw: &str) -> (&str, &str) {
    if !raw.starts_with("---\n") && !raw.starts_with("---\r\n") {
        return ("", raw);
    }

    let after = raw.trim_start_matches("---").trim_start_matches(['\r', '\n']);
    let Some(idx) = after.find("\n---") else {
        return ("", raw);
    };

    // advance past the closing line and its newline
    let after_close = &after[idx + "\n---".len()..];
    let front = &after[..idx];
    let body = after_close.trim_start_matches(['\r', '\n']).trim_end_matches(['\r', '\n']);
    (front, body)
}

#[cfg(test)]
mod tests {
    use inout_testing::{scenario, then, when};
    use super::*;

    #[test]
    fn skill_with_full_frontmatter_parses() {
        let mut s = scenario!(
            "skills",
            "Skill parses from markdown with YAML frontmatter",
            "Skill with full frontmatter parses"
        );
        let raw = "---\nname: rust\ncategory: domain\ntrigger: [\"rust\", \"cargo\"]\npriority: 10\ntokens: 700\n---\nbody here\n";
        when!(s, "the skill loader splits and parses the frontmatter", {
            let (front, body) = split_frontmatter(raw);
            let fm: Frontmatter = serde_yaml::from_str(front).unwrap();
            then!(s, "all frontmatter fields and the body are recovered", {
                assert_eq!(front.trim(), "name: rust\ncategory: domain\ntrigger: [\"rust\", \"cargo\"]\npriority: 10\ntokens: 700");
                assert_eq!(body, "body here");
                assert_eq!(fm.name.as_deref(), Some("rust"));
                assert_eq!(fm.category, Some(SkillCategory::Domain));
                assert_eq!(fm.priority, Some(10));
                assert_eq!(fm.tokens, Some(700));
            });
        });
    }

    #[test]
    fn skill_with_no_frontmatter_defaults_to_name_and_domain() {
        let mut s = scenario!(
            "skills",
            "Skill parses from markdown with YAML frontmatter",
            "Skill with missing category and no description defaults to domain"
        );
        when!(s, "parse_from_memory reads a bare markdown file", {
            let skill = parse_from_memory("review.md", "review skills\n").unwrap();
            then!(s, "the name is the file stem, category is domain, and a name-derived trigger exists", {
                assert_eq!(skill.name, "review");
                assert_eq!(skill.category, SkillCategory::Domain);
                assert_eq!(skill.triggers, vec!["review"]);
                assert!(skill.content.contains("review skills"));
            });
        });
    }

    #[test]
    fn skill_with_description_defaults_to_practice() {
        let mut s = scenario!(
            "skills",
            "Skill parses from markdown with YAML frontmatter",
            "Skill with missing category defaults correctly"
        );
        when!(s, "parse_from_memory reads a file with a description but no category", {
            let skill =
                parse_from_memory("git.md", "---\ndescription: git helpers\n---\nuseful git tips\n")
                    .unwrap();
            then!(s, "the category defaults to practice and a name-derived trigger exists", {
                assert_eq!(skill.category, SkillCategory::Practice);
                assert_eq!(skill.triggers, vec!["git"]);
            });
        });
    }

    fn parse_from_memory(file: &str, raw: &str) -> anyhow::Result<Skill> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(file);
        std::fs::write(&path, raw).unwrap();
        parse_skill_file(&path, SkillSource::Project)
    }
}
