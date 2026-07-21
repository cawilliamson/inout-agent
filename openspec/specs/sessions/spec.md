# Sessions Specification

## Purpose
Session is all durable agent state, not just transcript. It is an append-only entry tree with branching, compaction, and continuity handoff between sessions.

## Requirements

### Requirement: Core builds without the sessions extension
The `inout-core` crate SHALL build and pass clippy without depending on `inout-ext-sessions`.

#### Scenario: Core compiles in isolation
- GIVEN `inout-core` compiled with no dependency on `inout-ext-sessions`
- WHEN `cargo clippy --all-targets -- -D warnings` runs for `inout-core`
- THEN the command exits with code 0

### Requirement: Session entry tree
The system SHALL represent a session as an append-only tree of `SessionEntry` values. Each entry SHALL have `id`, `parent_id`, and `timestamp` fields. The entry kinds SHALL include `Message`, `ModelChange`, `ThinkingLevelChange`, `ActiveToolsChange`, `Compaction`, `BranchSummary`, `Custom`, `CustomMessage`, `Label`, `SessionInfo`, and `Leaf`.

#### Scenario: Entry has required base fields
- GIVEN a `SessionEntry` value of any kind
- WHEN its base fields are inspected
- THEN `id` is a non-empty string
- AND `parent_id` is `Option<String>`
- AND `timestamp` is a positive integer

#### Scenario: Leaf entry marks the current branch tip
- GIVEN a `Leaf` entry with `parent_id` pointing to an existing entry
- WHEN the session tree is navigated from the leaf
- THEN the path from leaf to root follows `parent_id` links

### Requirement: SessionEntry jsonl round-trip
`SessionEntry` SHALL serialize to and deserialize from a single line of newline-delimited JSON without data loss.

#### Scenario: Serialize every entry kind
- GIVEN one example of each `SessionEntry` kind
- WHEN each is serialized to a JSON string
- THEN the output contains a `type` discriminator and all payload fields

#### Scenario: Deserialize every entry kind
- GIVEN the serialized JSON for each `SessionEntry` kind
- WHEN each line is deserialized back to `SessionEntry`
- THEN the resulting value equals the original

### Requirement: Atomic append in JsonlSessionRepo
`JsonlSessionRepo::append_entry` SHALL durably persist the entry before the call returns. The implementation SHALL flush the underlying file before returning success.

#### Scenario: Append flushes before return
- GIVEN a `JsonlSessionRepo` with an open session file
- WHEN `append_entry` is called with a valid `SessionEntry`
- THEN the entry is present on disk before the future resolves

#### Scenario: Crash before append returns
- GIVEN an `append_entry` call in progress
- WHEN the process crashes before the call returns
- THEN the entry is not present in the file after recovery

#### Scenario: Crash after append returns
- GIVEN an `append_entry` call that has already returned successfully
- WHEN the process crashes before any subsequent mutation
- THEN the entry is present in the file after recovery

### Requirement: Session repository trait
The system SHALL provide a `SessionRepo` trait with methods for `append_entry`, `fork`, `list`, `navigate_tree`, `build_context`, `set_leaf_id`, and `get_metadata`.

#### Scenario: Append via trait
- GIVEN a type implementing `SessionRepo`
- WHEN `append_entry` is called
- THEN the entry is stored durably

#### Scenario: Fork via trait
- GIVEN a `SessionRepo` and an existing entry id
- WHEN `fork` is called with that id as the fork point
- THEN a new branch is created with a leaf whose `parent_id` matches the fork point

#### Scenario: List sessions
- GIVEN multiple sessions stored in a `SessionRepo`
- WHEN `list` is called
- THEN metadata for each session is returned

#### Scenario: Navigate tree
- GIVEN a `SessionRepo` containing a branched session tree
- WHEN `navigate_tree` is called with a source and target id
- THEN the result describes the path between the two nodes

#### Scenario: Build context
- GIVEN a `SessionRepo` and a leaf id
- WHEN `build_context` is called with that leaf id
- THEN the returned context reflects the entry tree from leaf to root

#### Scenario: Set active leaf
- GIVEN a `SessionRepo` with multiple leaves
- WHEN `set_leaf_id` is called with a leaf id
- THEN subsequent `build_context` calls use that leaf

#### Scenario: Get metadata
- GIVEN a `SessionRepo`
- WHEN `get_metadata` is called
- THEN session-level metadata is returned

### Requirement: JsonlSessionRepo is append-only
`JsonlSessionRepo` SHALL store one `SessionEntry` per line in an append-only file. Entries SHALL never be modified in place; compaction and branching SHALL append new entries.

#### Scenario: File grows monotonically
- GIVEN a `JsonlSessionRepo` under normal use
- WHEN any operation succeeds
- THEN the file length never decreases

#### Scenario: One entry per line
- GIVEN a session file written by `JsonlSessionRepo`
- WHEN the file is read as raw lines
- THEN each non-empty line parses as a single `SessionEntry`

### Requirement: Branching
The system SHALL support branching via `/branch <name>`, `/branches`, and `/switch <name>`. Forking SHALL create a new leaf whose `parent_id` points to the fork target. Listing branches SHALL return the branch names. Switching SHALL change the active leaf id used for context building.

#### Scenario: Fork a branch
- GIVEN a session at leaf `L1`
- WHEN `/branch feature` is issued
- THEN a new leaf `L2` is created
- AND `L2.parent_id` equals `L1.id`

#### Scenario: List branches
- GIVEN a session with branches `main` and `feature`
- WHEN `/branches` is issued
- THEN both branch names are returned

#### Scenario: Switch branches
- GIVEN a session with branches `main` and `feature`
- WHEN `/switch main` is issued
- THEN `build_context` returns the context for the `main` leaf

### Requirement: build_context applies non-message entries
`SessionRepo::build_context` SHALL walk the entry tree from the active leaf to the root and apply non-message entries in reverse chronological order. `ModelChange`, `ThinkingLevelChange`, and `ActiveToolsChange` entries SHALL update the context configuration. `Message` entries SHALL be collected in leaf-to-root order and reversed for presentation. `Compaction` and `BranchSummary` entries SHALL replace or summarize the messages they cover.

#### Scenario: Active tools change is reflected
- GIVEN a leaf whose path includes an `ActiveToolsChange` entry enabling a tool
- WHEN `build_context` is called
- THEN the returned context marks that tool as active

#### Scenario: Model change is reflected
- GIVEN a leaf whose path includes a `ModelChange` entry
- WHEN `build_context` is called
- THEN the returned context uses the model named in the most recent `ModelChange`

#### Scenario: Compaction replaces older messages
- GIVEN a leaf whose path includes a `Compaction` entry covering older messages
- WHEN `build_context` is called
- THEN those older messages are not returned verbatim
- AND a summary derived from the compaction is included

### Requirement: Continuity handoff writes context.md
`ContinuityFiles::write_handoff` SHALL use a small LLM call over the last 10 messages to produce 1-3 concrete next-step bullets and write them to `.inout/context.md`.

#### Scenario: Write handoff
- GIVEN a session with at least one message
- WHEN `write_handoff` is called
- THEN the LLM is called over the recent messages
- AND the resulting 1-3 bullets are written to `.inout/context.md`
- AND the file includes the goal, files touched, and next steps

#### Scenario: Handoff bullets are concrete
- GIVEN a session that touched files `src/foo.rs` and `src/bar.rs`
- WHEN `write_handoff` runs
- THEN at least one bullet references a specific file or function

### Requirement: Continuity handoff loads on demand
`ContinuityFiles::load_handoff` SHALL read `.inout/context.md` only when requested and return its contents for injection into the system prompt. The four continuity files — `INOUT.md`, `.inout/understanding.md`, `.inout/context.md`, and `.inout/session_log.md` — SHALL never be pre-loaded into context.

#### Scenario: Load handoff at session start
- GIVEN an existing `.inout/context.md` written by a prior session
- WHEN `load_handoff` is called at session start
- THEN the file contents are returned

#### Scenario: Handoff injected into system prompt
- GIVEN a non-empty handoff loaded via `load_handoff`
- WHEN the session system prompt is assembled
- THEN the handoff appears under a `## Last Session Handoff` section

#### Scenario: Continuity files are read lazily
- GIVEN all four continuity files exist on disk
- WHEN the session starts with no explicit read
- THEN none of the files are loaded into the context

### Requirement: Compaction emits before/after events
The `compact` operation SHALL emit a `SessionBeforeCompact` event before summarising and a `SessionCompact` event after. The before event SHALL be cancellable. After compaction, older messages SHALL be replaced by a summary entry.

#### Scenario: Before-compact event fires
- GIVEN a session eligible for compaction
- WHEN `compact` is called
- THEN a `SessionBeforeCompact` event is dispatched
- AND listeners can cancel the operation

#### Scenario: After-compact event fires
- GIVEN a `compact` call that was not cancelled
- WHEN compaction completes
- THEN a `SessionCompact` event is dispatched

#### Scenario: Older messages replaced by summary
- GIVEN a session with more messages than the configured maximum
- WHEN `compact` succeeds
- THEN a `Compaction` entry is appended
- AND the older messages are no longer returned verbatim by `build_context`
- AND the most recent messages are retained unchanged

### Requirement: Durability principle
Every accepted mutation to a session SHALL be durable before the public API resolves. Pending writes SHALL queue and flush at turn end; no mutation SHALL be considered accepted until it is durably stored.

#### Scenario: Accepted mutation is durable
- GIVEN a call that returns success
- WHEN the process crashes immediately after the call returns
- THEN the mutation is present in the session file after recovery

### Requirement: Recovery model
On reload, the system SHALL restore all entries that were accepted before the crash. An entry that was still pending when the crash occurred SHALL not appear in the restored session.

#### Scenario: Restore accepted entries
- GIVEN a session file with entries written and flushed
- WHEN the repo is reopened after a crash
- THEN all flushed entries are available

#### Scenario: Discard pending entries
- GIVEN a crash during an unflushed write
- WHEN the repo is reopened
- THEN partially written entries are ignored

### Requirement: Clippy clean
All sessions code SHALL compile without warnings under `cargo clippy --all-targets -- -D warnings`.

#### Scenario: Clippy runs clean
- GIVEN the workspace including `inout-ext-sessions`
- WHEN `cargo clippy --all-targets -- -D warnings` runs
- THEN the command exits with code 0
