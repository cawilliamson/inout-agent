//! trigger-word matching with length-aware word boundaries.

use std::collections::HashSet;

use crate::skill::{Skill, SkillCategory};

/// check whether `trigger` matches inside `text`.
///
/// rules:
/// - non-alphabetic triggers: simple substring match.
/// - alphabetic triggers of length <= 3: require both word boundaries.
/// - alphabetic triggers of length >= 4: require only a word-start boundary.
#[must_use]
pub fn trigger_word_match(text: &str, trigger: &str) -> bool {
    if trigger.is_empty() {
        return false;
    }

    let trigger_is_alpha = trigger.chars().all(|c| c.is_alphabetic());

    if !trigger_is_alpha {
        return text.contains(trigger);
    }

    let lower_text = text.to_lowercase();
    let lower_trigger = trigger.to_lowercase();

    if trigger.chars().count() <= 3 {
        // require both boundaries.
        for (start, word) in word_bounds(&lower_text) {
            let end = start + word.len();
            if word == lower_trigger
                && word_boundary_before(&lower_text, start)
                && word_boundary_after(&lower_text, end)
            {
                return true;
            }
        }
        false
    } else {
        // require only word-start boundary.
        lower_text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .any(|word| word.starts_with(&lower_trigger))
    }
}

/// split text into (byte_start, word) slices based on alphanumeric runs.
fn word_bounds(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (idx, c) in text.char_indices() {
        if c.is_alphanumeric() {
            if start.is_none() {
                start = Some(idx);
            }
        } else if let Some(s) = start.take() {
            out.push((s, &text[s..idx]));
        }
    }
    if let Some(s) = start {
        out.push((s, &text[s..]));
    }
    out
}

fn word_boundary_before(text: &str, idx: usize) -> bool {
    idx == 0 || text[..idx].ends_with(|c: char| !c.is_alphanumeric())
}

fn word_boundary_after(text: &str, idx: usize) -> bool {
    idx >= text.len() || text[idx..].starts_with(|c: char| !c.is_alphanumeric())
}

/// match skills that should fire for `query` within the current domain scope.
///
/// practice skills are always candidates. domain skills are candidates only
/// when their name appears in `domain_scope`, or when `domain_scope` is empty.
#[must_use]
pub fn match_skills_scoped<'a>(
    query: &str,
    skills: &'a [Skill],
    domain_scope: &HashSet<String>,
) -> Vec<&'a Skill> {
    let lower_query = query.to_lowercase();
    skills
        .iter()
        .filter(|s| {
            if s.category == SkillCategory::Practice {
                return true;
            }
            if s.category == SkillCategory::Domain {
                return domain_scope.is_empty() || domain_scope.contains(&s.name.to_lowercase());
            }
            false
        })
        .filter(|s| s.triggers.iter().any(|t| trigger_word_match(&lower_query, t)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_alpha_is_substring() {
        assert!(trigger_word_match("main.rs", ".rs"));
        assert!(trigger_word_match("fn main()", "fn "));
        assert!(!trigger_word_match("fnovel", "fn "));
    }

    #[test]
    fn short_alpha_both_boundaries() {
        assert!(trigger_word_match("open a pr", "pr"));
        assert!(!trigger_word_match("process", "pr"));
        assert!(!trigger_word_match("apropos", "pr"));
        assert!(trigger_word_match("go build", "go"));
    }

    #[test]
    fn long_alpha_start_boundary() {
        assert!(trigger_word_match("review this code", "review"));
        assert!(trigger_word_match("reviewing is hard", "review"));
        assert!(!trigger_word_match("preview the change", "review"));
    }

    #[test]
    fn case_insensitive() {
        assert!(trigger_word_match("Reviewing", "review"));
        assert!(trigger_word_match("Open A PR", "pr"));
    }

    #[test]
    fn scoped_match_includes_practice_and_domain() {
        let s = Skill {
            name: String::from("rust"),
            description: String::new(),
            category: SkillCategory::Domain,
            source: crate::skill::SkillSource::Bundled,
            triggers: vec![String::from("rust")],
            priority: 0,
            token_estimate: 0,
            content: String::new(),
            file_path: std::path::PathBuf::new(),
        };
        let practice = Skill {
            name: String::from("git"),
            description: String::new(),
            category: SkillCategory::Practice,
            source: crate::skill::SkillSource::Bundled,
            triggers: vec![String::from("git")],
            priority: 0,
            token_estimate: 0,
            content: String::new(),
            file_path: std::path::PathBuf::new(),
        };

        let mut scope = HashSet::new();
        scope.insert(String::from("rust"));
        let skills = [s.clone(), practice.clone()];
        let matched = match_skills_scoped("rust git", &skills, &scope);
        assert_eq!(matched.len(), 2);
        assert_eq!(matched[0].name, "rust");
        assert_eq!(matched[1].name, "git");

        let no_scope = HashSet::new();
        let skills = [s, practice];
        let matched = match_skills_scoped("rust git", &skills, &no_scope);
        assert_eq!(matched.len(), 2);
    }
}
