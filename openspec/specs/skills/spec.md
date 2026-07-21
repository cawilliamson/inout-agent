# Skills Specification

## Purpose
Markdown skill files with YAML frontmatter, lazy-loaded and trigger-matched per turn. Skills are injected into the system prompt only when relevant, so the agent keeps context cost and noise under control.

## Requirements

### Requirement: Core builds without skills crate
`inout-core` SHALL build and pass clippy without depending on the `inout-ext-skills` crate.

#### Scenario: Core compiles alone
- GIVEN a build of `inout-core` that excludes `inout-ext-skills`
- WHEN `cargo build -p inout-core` runs
- THEN compilation succeeds

#### Scenario: Core clippy passes alone
- GIVEN a build of `inout-core` that excludes `inout-ext-skills`
- WHEN `cargo clippy -p inout-core -- -D warnings` runs
- THEN no warnings or errors are emitted

### Requirement: Skill parses from markdown with YAML frontmatter
A skill file SHALL be a markdown file with a YAML frontmatter block delimited by `---` fences. The frontmatter SHALL provide at least `name`. Missing `category` SHALL default to `Practice` when the skill has a description, and to `Domain` otherwise. Missing `trigger` for non-Core skills SHALL derive triggers from the skill name.

#### Scenario: Skill with full frontmatter parses
- GIVEN a file containing a `---` YAML block with `name`, `category`, `trigger`, `priority`, and `tokens`
- WHEN the skill loader reads the file
- THEN a `Skill` is produced with the parsed values

#### Scenario: Skill with missing category defaults correctly
- GIVEN a skill file that has a `description` but no `category`
- WHEN the skill loader reads the file
- THEN the category is `Practice`

#### Scenario: Skill with missing category and no description defaults to domain
- GIVEN a skill file with no `category` and no `description`
- WHEN the skill loader reads the file
- THEN the category is `Domain`

#### Scenario: Foreign skill without triggers gets name-derived triggers
- GIVEN a skill file with a `name` of `rust` and no `trigger` field
- WHEN the skill loader reads the file
- THEN the skill receives at least one trigger derived from its name

### Requirement: Skill source tiers and override order
`load_all_skills` SHALL discover skills from compiled-in bundled defaults, the user-global directory, configured external directories, and the project-local directory. Project skills SHALL override global skills on name collision, and external skills SHALL sit between global and project in priority.

#### Scenario: All source tiers load in priority order
- GIVEN bundled, global, external, and project skill sources all present
- WHEN `load_all_skills` runs
- THEN the returned list is ordered from lowest priority to highest priority: Bundled, Global, External, Project

#### Scenario: Project overrides global on name collision
- GIVEN a skill named `foo` in both `~/.inout/skills/` and `.inout/skills/`
- WHEN `load_all_skills` runs
- THEN the returned `foo` skill has project source and content

### Requirement: Trigger word matching with length-aware boundaries
`trigger_word_match` SHALL match a trigger string against a query using boundary rules that depend on trigger length and character type. Non-alphabetic triggers SHALL match by substring. Alphabetic triggers of three characters or fewer SHALL require both word boundaries. Alphabetic triggers of four characters or more SHALL require only a word-start boundary.

#### Scenario: Short trigger requires both boundaries
- GIVEN the trigger `pr`
- WHEN matching against the text `process`
- THEN the match is false

#### Scenario: Short trigger matches whole word
- GIVEN the trigger `pr`
- WHEN matching against the text `open a pr`
- THEN the match is true

#### Scenario: Long trigger matches start boundary inside larger word
- GIVEN the trigger `review`
- WHEN matching against the text `reviewing the code`
- THEN the match is true

#### Scenario: Long trigger does not match inside unrelated word
- GIVEN the trigger `review`
- WHEN matching against the text `preview`
- THEN the match is false

#### Scenario: Non-alphabetic trigger matches substring
- GIVEN the trigger `.rs`
- WHEN matching against the text `main.rs file`
- THEN the match is true

#### Scenario: Non-alphabetic trigger matches code fragment
- GIVEN the trigger `fn `
- WHEN matching against the text `fn main()`
- THEN the match is true

### Requirement: Skill budget ranking and truncation
`rank_and_truncate_skills` SHALL keep a list of skills within a token budget by dropping the lowest-ranked skills. Ranking SHALL be by priority descending, then source tier descending, then token count ascending. Pinned skills SHALL always be kept even if the budget is exceeded.

#### Scenario: Low-priority skill dropped first
- GIVEN a budget of one skill and two skills with different priorities
- WHEN `rank_and_truncate_skills` runs
- THEN the lower-priority skill is dropped

#### Scenario: Higher source tier wins at equal priority
- GIVEN two skills with equal priority but different source tiers
- WHEN `rank_and_truncate_skills` runs
- THEN the project-tier skill outranks the bundled-tier skill

#### Scenario: Smaller skill wins at equal priority and tier
- GIVEN two skills with equal priority and source tier but different token counts
- WHEN `rank_and_truncate_skills` runs
- THEN the skill with the smaller token count outranks the larger one

#### Scenario: Pinned skill is always kept
- GIVEN a pinned skill whose token count alone exceeds the budget
- WHEN `rank_and_truncate_skills` runs
- THEN the pinned skill is still included

### Requirement: Always-on budget returns dropped skill names
`build_always_on_prompt_budgeted` SHALL return the assembled prompt block, the total token count of included skills, and the names of skills dropped because of the budget.

#### Scenario: Budget exceeded reports dropped names
- GIVEN a set of always-on skills whose combined tokens exceed the budget
- WHEN `build_always_on_prompt_budgeted` runs
- THEN the returned list of dropped names contains at least one skill name
- AND the included token total is less than or equal to the budget

#### Scenario: Budget not exceeded reports no drops
- GIVEN a set of always-on skills whose combined tokens fit inside the budget
- WHEN `build_always_on_prompt_budgeted` runs
- THEN the returned list of dropped names is empty

### Requirement: Skill trace records one entry per turn
`SkillTrace` SHALL record exactly one entry per user turn. Each entry SHALL contain the turn number, a preview of the user message, the names of skills matched for that turn, and an optional reason string.

#### Scenario: Trace entry stores matched skills and reason
- GIVEN a user turn with matched skills and a reason of `casual`
- WHEN the trace pushes the entry
- THEN `for_turn` returns the entry with the matched skill names and reason

#### Scenario: Trace entry records no-match reason
- GIVEN a user turn with no matched skills and a reason of `no match`
- WHEN the trace pushes the entry
- THEN `for_turn` returns an entry with an empty matched-skills list and reason `no match`

### Requirement: Stack auto-detection populates domain scope
The system SHALL detect project stacks by reading manifest files and populate the session domain scope with the corresponding domain names.

#### Scenario: Cargo.toml maps to rust domain
- GIVEN a project directory containing `Cargo.toml`
- WHEN stack detection runs
- THEN the domain scope includes `rust`

#### Scenario: package.json maps to typescript and react domains
- GIVEN a project directory containing `package.json`
- WHEN stack detection runs
- THEN the domain scope includes `typescript` and `react`

#### Scenario: pyproject.toml maps to python domain
- GIVEN a project directory containing `pyproject.toml`
- WHEN stack detection runs
- THEN the domain scope includes `python`

#### Scenario: go.mod maps to go domain
- GIVEN a project directory containing `go.mod`
- WHEN stack detection runs
- THEN the domain scope includes `go`

#### Scenario: pom.xml maps to java domain
- GIVEN a project directory containing `pom.xml`
- WHEN stack detection runs
- THEN the domain scope includes `java`

### Requirement: Core builds with skills feature disabled
`inout-core` SHALL provide a compile-time feature flag that disables all skill loading and injection, leaving the v1.0 core loop intact.

#### Scenario: Core compiles without skills feature
- GIVEN `inout-core` built with the skills feature disabled
- WHEN `cargo build -p inout-core --no-default-features` runs
- THEN compilation succeeds
- AND no skill injection code is active at runtime

### Requirement: Skill categories
Skills SHALL belong to one of three categories. `Core` skills are always injected into the system prompt. `Practice` skills are trigger candidates across all sessions. `Domain` skills are trigger candidates only when their domain is in the session scope.

#### Scenario: Core skill is always-on
- GIVEN a skill with category `Core`
- WHEN the always-on block is built
- THEN the skill is included in the always-on prompt block

#### Scenario: Practice skill triggers regardless of domain scope
- GIVEN a skill with category `Practice`
- WHEN a user query matches the skill trigger
- THEN the skill is injected even if the domain scope is empty

#### Scenario: Domain skill triggers only within scope
- GIVEN a skill with category `Domain` and a domain of `rust`
- WHEN the session domain scope contains `rust`
- THEN the skill can be matched by a user query

#### Scenario: Domain skill stays dormant outside scope
- GIVEN the same `rust` Domain skill
- WHEN the session domain scope does not contain `rust`
- THEN the skill is not matched by any user query

### Requirement: Skill source tiers
Skills SHALL carry one of four source tiers: `Bundled`, `Global`, `External`, or `Project`. The tier determines override priority and ranking.

#### Scenario: Bundled source for compiled-in defaults
- GIVEN a skill loaded from compiled-in defaults
- WHEN the skill is parsed
- THEN its source tier is `Bundled`

#### Scenario: Global source for user directory
- GIVEN a skill loaded from `~/.inout/skills/`
- WHEN the skill is parsed
- THEN its source tier is `Global`

#### Scenario: External source for configured paths
- GIVEN a skill loaded from a path listed in `config.skill_paths`
- WHEN the skill is parsed
- THEN its source tier is `External`

#### Scenario: Project source for local directory
- GIVEN a skill loaded from `.inout/skills/`
- WHEN the skill is parsed
- THEN its source tier is `Project`

### Requirement: Skill commands
The system SHALL expose `/skill` commands to inspect and manage skills: `list`, `show`, `create`, `log`, and `scope`.

#### Scenario: `/skill list` shows grouped skills
- GIVEN skills exist across always-on and trigger categories
- WHEN `/skill list` runs
- THEN output groups skills as always-on and triggered
- AND each skill shows its source tier

#### Scenario: `/skill show <name>` previews a skill
- GIVEN a skill named `rust`
- WHEN `/skill show rust` runs
- THEN output includes the skill description and content preview

#### Scenario: `/skill create <name>` scaffolds a project skill
- GIVEN no skill named `foo` in `.inout/skills/`
- WHEN `/skill create foo` runs
- THEN a new skill file is created under `.inout/skills/`

#### Scenario: `/skill create <name> --global` scaffolds a global skill
- GIVEN no skill named `foo` in `~/.inout/skills/`
- WHEN `/skill create foo --global` runs
- THEN a new skill file is created under `~/.inout/skills/`

#### Scenario: `/skill log` shows the skill trace
- GIVEN a session with at least one recorded turn
- WHEN `/skill log` runs
- THEN output contains the turn number, user preview, matched skill names, and reason

#### Scenario: `/skill scope` shows active domain scope
- GIVEN a session with a domain scope containing `rust`
- WHEN `/skill scope` runs
- THEN output includes `rust`

### Requirement: Clippy green
The workspace SHALL build and pass `cargo clippy --all-targets -- -D warnings` without warnings or errors.

#### Scenario: Full clippy run passes
- GIVEN the full workspace source
- WHEN `cargo clippy --all-targets -- -D warnings` runs
- THEN the command exits successfully
