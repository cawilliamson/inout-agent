//! per-turn record of which skills fired.

use serde::Serialize;

use crate::skill::Skill;

/// one entry in the skill trace.
#[derive(Debug, Clone, Serialize)]
pub struct SkillTraceEntry {
    /// turn number.
    pub turn: usize,
    /// first 60 characters of the user message.
    pub user_preview: String,
    /// names of skills that matched this turn.
    pub matched_skills: Vec<String>,
    /// optional reason string, e.g. `casual` or `no match`.
    pub reason: Option<String>,
}

/// trace of skill firings across a session.
#[derive(Debug, Clone, Default)]
pub struct SkillTrace {
    entries: Vec<SkillTraceEntry>,
}

impl SkillTrace {
    /// create an empty trace.
    pub fn new() -> Self {
        Self::default()
    }

    /// record a turn.
    pub fn push(&mut self, turn: usize, preview: &str, skills: &[&Skill], reason: Option<&str>) {
        self.entries.push(SkillTraceEntry {
            turn,
            user_preview: preview.chars().take(60).collect(),
            matched_skills: skills.iter().map(|s| s.name.clone()).collect(),
            reason: reason.map(String::from),
        });
    }

    /// get the entry for a specific turn, if any.
    #[must_use]
    pub fn for_turn(&self, turn: usize) -> Option<&SkillTraceEntry> {
        self.entries.iter().find(|e| e.turn == turn)
    }

    /// all entries.
    #[must_use]
    pub fn all(&self) -> &[SkillTraceEntry] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use inout_testing::{scenario, then, when};
    use super::*;

    fn dummy_skill(name: &str) -> Skill {
        Skill {
            name: String::from(name),
            description: String::new(),
            category: crate::skill::SkillCategory::Practice,
            source: crate::skill::SkillSource::Bundled,
            triggers: Vec::new(),
            priority: 0,
            token_estimate: 0,
            content: String::new(),
            file_path: std::path::PathBuf::new(),
        }
    }

    #[test]
    fn trace_entry_stores_matched_skills_and_reason() {
        let mut s = scenario!(
            "skills",
            "Skill trace records one entry per turn",
            "Trace entry stores matched skills and reason"
        );
        let mut trace = SkillTrace::new();
        let skill = dummy_skill("git");
        when!(s, "a turn with a matched skill and no reason is pushed", {
            trace.push(1, "commit changes please", &[&skill], None);
            let entry = trace.for_turn(1).unwrap();
            then!(s, "for_turn returns the matched skill names and a null reason", {
                assert_eq!(entry.matched_skills, vec![String::from("git")]);
                assert_eq!(entry.user_preview, "commit changes please");
                assert_eq!(entry.reason, None);
            });
        });
    }

    #[test]
    fn trace_entry_records_no_match_reason() {
        let mut s = scenario!(
            "skills",
            "Skill trace records one entry per turn",
            "Trace entry records no-match reason"
        );
        let mut trace = SkillTrace::new();
        when!(s, "a turn with no matched skills and reason 'no match' is pushed", {
            trace.push(2, "a".repeat(120).as_str(), &[], Some("no match"));
            let entry = trace.for_turn(2).unwrap();
            then!(s, "the preview is truncated and the reason is recorded", {
                assert_eq!(entry.user_preview.len(), 60);
                assert_eq!(entry.reason, Some(String::from("no match")));
            });
        });
    }
}
