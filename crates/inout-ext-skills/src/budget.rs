//! skill ranking, truncation, and always-on block assembly.

use std::collections::HashSet;

use crate::skill::Skill;

/// rank skills and truncate to fit `budget_tokens`. pinned skills are always
/// kept and do not count toward the budget.
///
/// ranking order:
/// 1. pinned first,
/// 2. priority descending,
/// 3. source tier descending (project > external > global > bundled),
/// 4. token count ascending.
#[must_use]
pub fn rank_and_truncate_skills<'a>(
    skills: Vec<&'a Skill>,
    budget_tokens: usize,
    pinned: &HashSet<String>,
) -> Vec<&'a Skill> {
    let mut sorted = skills;
    sorted.sort_by(|a, b| {
        let a_pinned = pinned.contains(&a.name);
        let b_pinned = pinned.contains(&b.name);
        b_pinned
            .cmp(&a_pinned)
            .then_with(|| b.priority.cmp(&a.priority))
            .then_with(|| b.source.tier().cmp(&a.source.tier()))
            .then_with(|| a.tokens().cmp(&b.tokens()))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let mut kept: Vec<&Skill> = Vec::new();
    let mut used: usize = 0;

    for skill in sorted {
        let is_pinned = pinned.contains(&skill.name);
        let tokens = skill.tokens();
        if is_pinned {
            kept.push(skill);
            continue;
        }
        if used.saturating_add(tokens) > budget_tokens {
            continue;
        }
        kept.push(skill);
        used += tokens;
    }

    kept
}

/// build the always-on prompt block and report dropped names.
#[must_use]
pub fn build_always_on_prompt_budgeted(
    always_on: &[&Skill],
    budget_tokens: usize,
) -> (String, usize, Vec<String>) {
    let ranked = rank_and_truncate_skills(always_on.to_vec(), budget_tokens, &HashSet::new());
    let included_names: HashSet<String> = ranked.iter().map(|s| s.name.clone()).collect();
    let dropped: Vec<String> = always_on
        .iter()
        .filter(|s| !included_names.contains(&s.name))
        .map(|s| s.name.clone())
        .collect();

    let mut block = String::new();
    let mut tokens: usize = 0;
    for skill in &ranked {
        block.push_str(&format!("# skill: {}\n\n{}\n\n", skill.name, skill.content.trim()));
        tokens += skill.tokens();
    }
    (block, tokens, dropped)
}

/// assemble the always-on block and an optional user-facing warning.
#[must_use]
pub fn assemble_always_on_block(
    always_on: &[&Skill],
    budget_tokens: usize,
) -> (String, Option<String>) {
    let (block, _tokens, dropped) = build_always_on_prompt_budgeted(always_on, budget_tokens);
    let warning = if dropped.is_empty() {
        None
    } else {
        let names = dropped.join(", ");
        Some(format!("skills dropped from system prompt due to budget: {names}"))
    };
    (block, warning)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use inout_testing::{scenario, then, when};
    use super::*;

    fn skill(name: &str, priority: i32, tokens: usize, source: crate::skill::SkillSource) -> Skill {
        Skill {
            name: String::from(name),
            description: String::new(),
            category: crate::skill::SkillCategory::Core,
            source,
            triggers: Vec::new(),
            priority,
            token_estimate: tokens,
            content: String::from("body"),
            file_path: PathBuf::new(),
        }
    }

    #[test]
    fn low_priority_skill_dropped_first() {
        let mut s = scenario!(
            "skills",
            "Skill budget ranking and truncation",
            "Low-priority skill dropped first"
        );
        let a = skill("a", 0, 100, crate::skill::SkillSource::Bundled);
        let b = skill("b", 0, 60, crate::skill::SkillSource::Bundled);
        let c = skill("c", 0, 30, crate::skill::SkillSource::Bundled);

        when!(s, "rank_and_truncate_skills runs with a 120-token budget", {
            let skills = rank_and_truncate_skills(vec![&a, &b, &c], 120, &HashSet::new());
            then!(s, "the lowest-priority skill is dropped and the remainder is ranked by token count", {
                assert_eq!(skills.len(), 2);
                assert_eq!(skills[0].name, "c");
                assert_eq!(skills[1].name, "b");
            });
        });
    }

    #[test]
    fn pinned_skill_is_always_kept() {
        let mut s = scenario!(
            "skills",
            "Skill budget ranking and truncation",
            "Pinned skill is always kept"
        );
        let a = skill("a", 0, 100, crate::skill::SkillSource::Bundled);
        let b = skill("b", 0, 100, crate::skill::SkillSource::Bundled);
        let mut pinned = HashSet::new();
        pinned.insert(String::from("b"));

        when!(s, "rank_and_truncate_skills runs with a 50-token budget and b pinned", {
            let skills = rank_and_truncate_skills(vec![&a, &b], 50, &pinned);
            then!(s, "the pinned skill is still included despite exceeding the budget", {
                assert_eq!(skills.len(), 1);
                assert_eq!(skills[0].name, "b");
            });
        });
    }

    #[test]
    fn higher_source_tier_wins_at_equal_priority() {
        let mut s = scenario!(
            "skills",
            "Skill budget ranking and truncation",
            "Higher source tier wins at equal priority"
        );
        let bundled = skill("x", 5, 10, crate::skill::SkillSource::Bundled);
        let project = skill("x", 5, 10, crate::skill::SkillSource::Project);

        when!(s, "rank_and_truncate_skills runs over a bundled and a project skill", {
            let skills = rank_and_truncate_skills(vec![&bundled, &project], 100, &HashSet::new());
            then!(s, "the project-tier skill outranks the bundled-tier skill", {
                assert_eq!(skills[0].source, crate::skill::SkillSource::Project);
            });
        });
    }

    #[test]
    fn budget_exceeded_reports_dropped_names() {
        let mut s = scenario!(
            "skills",
            "Always-on budget returns dropped skill names",
            "Budget exceeded reports dropped names"
        );
        let a = skill("a", 0, 100, crate::skill::SkillSource::Bundled);
        let b = skill("b", 0, 60, crate::skill::SkillSource::Bundled);

        when!(s, "build_always_on_prompt_budgeted runs with an 80-token budget", {
            let (_, _tokens, dropped) = build_always_on_prompt_budgeted(&[&a, &b], 80);
            then!(s, "the dropped list contains the over-budget skill name", {
                assert_eq!(dropped, vec![String::from("a")]);
            });
        });
    }
}
