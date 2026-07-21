# Context Management Specification

## Purpose
The context management layer assembles the system prompt for each turn, slides the message window, summarises dropped turns, maintains a persistent edit ledger across window eviction, and skips context overhead for casual turns.

## Requirements

### Requirement: Core-only build
The core crate SHALL build and pass clippy without the context extension crate present.

#### Scenario: Core crate builds in isolation
- GIVEN a checkout with only `inout-core` enabled
- WHEN `cargo clippy --all-targets -- -D warnings` runs
- THEN the command succeeds
- AND no context-extension code is required

### Requirement: System prompt assembly
The system SHALL assemble the full system prompt from modular sections: identity, current working directory, core tool rules, code navigation order, security prose, sub-agent guidance, reasoning and investigation guidance, project context, domain-map staleness nudge, and an injected skill block.

#### Scenario: Full prompt includes all modular sections
- GIVEN a configuration with all optional features enabled
- WHEN `build_system_prompt` is called
- THEN the returned prompt contains the identity section
- AND the current working directory
- AND the core tool rules
- AND the code navigation order
- AND the security prose
- AND the sub-agent guidance
- AND the reasoning and investigation guidance
- AND the project context
- AND the domain-map staleness nudge
- AND the injected skill block

#### Scenario: Casual prompt omits non-essential sections
- GIVEN a configuration with all optional features enabled
- WHEN `build_casual_system_prompt` is called
- THEN the returned prompt contains the identity section
- AND the current working directory
- AND the core tool rules
- AND does not contain code navigation order
- AND does not contain sub-agent guidance
- AND does not contain security prose
- AND does not contain git status
- AND does not contain project context

### Requirement: Project context loading
The system SHALL walk upward from the current working directory to the repository root, loading `INOUT.md` at each level, plus a global `~/.inout/INOUT.md` layer. Layers SHALL be ordered from most-general to most-specific so the most-specific layer takes priority. If no `INOUT.md` is found, the system SHALL fall back to `CLAUDE.md`.

#### Scenario: Hierarchical INOUT.md loaded
- GIVEN `~/.inout/INOUT.md`, a repo-root `INOUT.md`, and a sub-project `INOUT.md`
- WHEN project context is loaded from the sub-project directory
- THEN all three files are read
- AND content is ordered global first, then repo root, then sub-project last

#### Scenario: CLAUDE.md fallback used when INOUT.md is absent
- GIVEN a project containing `CLAUDE.md` but no `INOUT.md`
- WHEN project context is loaded
- THEN the content of `CLAUDE.md` is returned

### Requirement: Windowed history
The system SHALL cap history to the last 8 user turns. The window SHALL be counted by user turns, not raw message count. Oversized tool results in the window SHALL be pruned.

#### Scenario: Window capped at eight user turns
- GIVEN a session with more than 8 user turns
- WHEN `windowed_history` is called
- THEN only messages from the last 8 user turns remain

#### Scenario: Oversized tool results are pruned
- GIVEN a message containing a tool result that exceeds the configured size limit
- WHEN the history is prepared for sending
- THEN the oversized tool result is reduced in place before the message leaves the context layer

### Requirement: Drop summariser
The system SHALL summarise turns that slide off the window before the current turn's tool loop. The summariser SHALL call an LLM with a focused summarisation prompt. On failure, excessive input size, or an unavailable model, the system SHALL fall back to `text_drop_summary`. The produced summary SHALL be appended to the session's dropped-summary record and prepended to the windowed history on the next LLM call as a synthetic user/assistant pair.

#### Scenario: Dropped turns are LLM-summarised
- GIVEN turns that have just slid out of the window
- WHEN `maybe_summarize_dropped_turns` runs
- THEN the LLM receives a focused summarisation prompt
- AND the returned summary is appended to `dropped_summary`

#### Scenario: Summariser falls back to text summary
- GIVEN the LLM summariser call fails
- WHEN `maybe_summarize_dropped_turns` runs
- THEN `text_drop_summary` is used instead
- AND the session continues without blocking the turn

#### Scenario: Summary is injected into the next LLM call
- GIVEN a non-empty `dropped_summary`
- WHEN the next LLM call is constructed
- THEN a synthetic user message carries the dropped-summary content
- AND a synthetic assistant message acknowledges it

### Requirement: Summariser input pruning
The system SHALL cap each tool result passed to the summariser to 500 characters. The total character input passed to the summariser SHALL be capped; if the cap is exceeded, the system SHALL fall back to `text_drop_summary`.

#### Scenario: Tool results are capped for the summariser
- GIVEN a dropped turn containing a tool result longer than 500 characters
- WHEN `prune_for_summarizer` processes the messages
- THEN the tool result is truncated to 500 characters before summariser input

#### Scenario: Excessive summariser input falls back
- GIVEN dropped turns whose pruned input exceeds the summariser input character cap
- WHEN the summariser is invoked
- THEN `text_drop_summary` is used instead

### Requirement: Edit ledger
The system SHALL record every file written or edited in a per-file ledger. Each ledger entry SHALL track the file path, operation count, last turn, last operation type, and a short preview of the most recent change. The ledger SHALL persist across context window eviction. When rendered, entries SHALL be sorted by `last_turn` descending then `ops_count` descending, and only the top 20 SHALL be included.

#### Scenario: Every edit and write is recorded
- GIVEN a turn that calls a write tool and an edit tool
- WHEN the tools complete
- THEN the ledger contains an entry for each affected file
- AND each entry's operation count reflects the performed operation

#### Scenario: Ledger survives window eviction
- GIVEN a ledger with entries from turns that later slide off the window
- WHEN those turns are evicted
- THEN the ledger entries remain available for injection into the system prompt

#### Scenario: Render block returns top twenty sorted correctly
- GIVEN a ledger with more than 20 edited files
- WHEN `render_block` is called
- THEN exactly 20 file summaries are returned
- AND the returned entries are ordered by `last_turn` descending, then `ops_count` descending

### Requirement: Casual turn detection
The system SHALL detect casual messages. `is_casual_message` SHALL return true for greetings and social utterances. `needs_prior_context` SHALL return true when the last assistant message asked a question and the user's reply is a short answer that requires the prior context.

#### Scenario: Greeting is casual
- GIVEN the user input "hi there"
- WHEN `is_casual_message` evaluates it
- THEN it returns true

#### Scenario: Short answer after model question needs prior context
- GIVEN an assistant message that ends with a question
- AND a user reply of "yes" or "ok" or "go ahead" or "sounds good"
- WHEN `needs_prior_context` evaluates the reply against the history
- THEN it returns true

### Requirement: Context fill percentage
The system SHALL compute `context_fill_pct` from the windowed history tokens, the dropped-summary tokens, and the projected skill tokens. Projected skill tokens SHALL be included before the compaction check. The percentage SHALL be returned as an unsigned 8-bit integer.

#### Scenario: Projected skill tokens included before compaction check
- GIVEN a windowed history and dropped summary that together fill 75% of the model limit
- AND projected skill tokens that would push the total past 90%
- WHEN `context_fill_pct` is computed
- THEN the returned value reflects the projected skill tokens
- AND compaction is triggered

### Requirement: Compaction and refusal thresholds
The system SHALL trigger context compaction when `context_fill_pct` reaches or exceeds 90%. When a context budget is set and the fill percentage reaches or exceeds 100%, the system SHALL refuse the turn with a notice to the user.

#### Scenario: Compaction triggers at ninety percent
- GIVEN a context fill percentage of 90%
- WHEN the turn is prepared
- THEN compaction is triggered automatically

#### Scenario: Turn refused at one hundred percent with budget
- GIVEN a configured context budget
- AND a context fill percentage of 100% or greater
- WHEN the turn is prepared
- THEN the turn is refused
- AND the user receives a notice explaining the refusal

### Requirement: Clippy clean
The context extension crate and the full workspace SHALL pass clippy with warnings denied.

#### Scenario: Full workspace clippy is green
- GIVEN a clean checkout
- WHEN `cargo clippy --all-targets -- -D warnings` runs
- THEN the command exits successfully
