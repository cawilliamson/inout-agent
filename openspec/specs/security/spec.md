# Security Specification

## Purpose
The agent handles source code, credentials, and shell. Security is a core feature. Four layers protect the user and the project: permission modes, secret scanner, shell sandbox, and audit trail. An undo ledger lets users reverse edits safely. This specification describes the observable behavior of those layers.

## Requirements

### Requirement: Core crate independence
The core crate SHALL build and pass linting without any security extension crates present.

#### Scenario: Build core without security extensions
- GIVEN a workspace where `inout-core` is built in isolation from `inout-ext-permissions`, `inout-ext-secret-scanner`, `inout-ext-jail`, and `inout-ext-undo`
- WHEN `cargo build` runs for the core crate
- THEN the build succeeds
- AND `cargo clippy` on the core crate completes without warnings treated as errors

### Requirement: Permission modes
The permission system SHALL support three modes: `Ask`, `Auto`, and `Deny`. In `Ask` mode the system prompts before any write-affecting tool runs. In `Auto` mode the system approves all write-affecting tools without prompting. In `Deny` mode the system blocks all write-affecting tools.

#### Scenario: Ask mode prompts for writes
- GIVEN the permission mode is set to `Ask`
- WHEN a write-affecting tool such as `write_file` is requested
- THEN the system prompts the user for approval before running the tool

#### Scenario: Auto mode approves writes
- GIVEN the permission mode is set to `Auto`
- WHEN a write-affecting tool such as `edit_file` is requested
- THEN the system allows the tool to run without user interaction

#### Scenario: Deny mode blocks writes
- GIVEN the permission mode is set to `Deny`
- WHEN any write-affecting tool such as `shell`, `write_file`, `edit_file`, `batch_edit`, or `spawn_agent` is requested
- THEN the system blocks the tool

### Requirement: Tool grant classes
The system SHALL group semantically-equivalent write tools into grant classes. Granting one tool in a class SHALL grant every tool in that class for the same approval scope.

#### Scenario: Granting one edit-class tool grants the whole class
- GIVEN `edit_file`, `write_file`, and `batch_edit` belong to the same grant class
- WHEN the user grants approval for `edit_file`
- THEN `write_file` and `batch_edit` are also considered approved in the same scope
- AND `spawn_agent` remains unapproved unless granted separately

### Requirement: Grouped batch prompt
The permission manager SHALL collect every pending write request in a turn and present a single grouped prompt. The response SHALL provide a per-request approval result in the same order as the input.

#### Scenario: One prompt per turn for multiple writes
- GIVEN three pending write requests in one turn
- WHEN the permission manager resolves the batch
- THEN the user sees exactly one prompt that lists all three requests
- AND the result contains one boolean for each request, preserving order

### Requirement: Session pre-grant
The system SHALL pre-grant write tools for the duration of a session when the user invokes `/init`.

#### Scenario: Init pre-grants write tools
- GIVEN the agent receives the `/init` command
- WHEN the command completes
- THEN write-affecting tools are approved for the remainder of the session without per-tool prompting

### Requirement: Secret scanner coverage
The secret scanner SHALL detect at least 25 distinct secret pattern categories. Detected secrets SHALL be removable from content in-place by redaction.

#### Scenario: Scan detects common secret patterns
- GIVEN content containing examples from the following categories:
  - Anthropic (`sk-ant-`)
  - OpenAI (`sk-proj-`, `sk-or-`)
  - Stripe
  - Google (`aiza`)
  - GitHub (`ghp_`, `ghs_`, `gho_`, `ghu_`, `ghr_`, `github_pat_`)
  - GitLab (`glpat-`)
  - npm (`_authtoken=`)
  - AWS (`akia`, `aws_secret_access_key`)
  - GCP service account JSON
  - Azure
  - Slack (`xoxb-`, `xoxp-`, `xapp-`)
  - database connection strings (`postgres://`, `mysql://`, `mongodb://`, `redis://`)
  - private key blocks (`-----begin`)
  - JWT (`eyjh`, `eyja`)
  - generic secret fields (`password=`, `api_key=`, `secret=`, `access_token=`, `authorization: bearer `)
- WHEN the scanner runs
- THEN each secret category is reported with its line number and a masked preview

#### Scenario: Redact strips secret lines in-place
- GIVEN a string containing one or more detected secret lines
- WHEN redaction is applied
- THEN the secret lines are removed or replaced within the same string
- AND the returned content contains no recoverable secret material from those lines

### Requirement: Entropy false-positive suppression
The high-entropy heuristic SHALL flag secrets that are long, high-entropy, and mixed-character-class, and SHALL NOT flag all-lowercase hexadecimal strings that match the shape of git SHAs.

#### Scenario: Git SHA avoids false positive
- GIVEN a line containing only a 40-character all-lowercase hexadecimal git SHA
- WHEN the entropy heuristic evaluates the line
- THEN the line is not reported as a secret

#### Scenario: High-entropy secret is detected
- GIVEN a line containing a long, high-entropy, mixed-character-class string that is not all-lowercase hex
- WHEN the entropy heuristic evaluates the line
- THEN the line is reported as a secret

### Requirement: Shell sandbox
The shell sandbox SHALL support three modes: `Off`, `Workdir`, and `Container`. In `Container` mode the system SHALL wrap the command in a container runtime with no network access and project-root workdir. If no container runtime is available, the system SHALL fall back to `Workdir` with a warning.

#### Scenario: Container mode with runtime available
- GIVEN a container runtime such as Docker or Podman is installed
- WHEN a shell command runs in `Container` mode
- THEN the command executes inside a container launched with `--network none`
- AND the working directory inside the container is the project root

#### Scenario: Container mode falls back without runtime
- GIVEN no container runtime is available on the host
- WHEN a shell command runs in `Container` mode
- THEN the command runs in `Workdir` mode
- AND a warning is emitted explaining the fallback

### Requirement: Undo ledger
The system SHALL record the pre-edit contents of a file before applying an edit. The `/undo` command SHALL restore the most recent snapshot, optionally for a specific path.

#### Scenario: Snapshot before edit
- GIVEN a file exists and is about to be edited
- WHEN the edit tool runs
- THEN the file's content before the edit is stored in the undo ledger

#### Scenario: Undo restores by path
- GIVEN a snapshot exists for `/tmp/example.txt`
- WHEN the user issues `/undo /tmp/example.txt`
- THEN the file is restored to its snapshotted content

#### Scenario: Undo restores most recent snapshot
- GIVEN multiple snapshots exist
- WHEN the user issues `/undo` with no argument
- THEN the most recently snapshotted file is restored

### Requirement: Audit trail
Every permission decision SHALL be appended to `~/.inout/audit.jsonl`. Each record SHALL include the tool name, the decision, and the reason when the request is blocked.

#### Scenario: Allowed decision is audited
- GIVEN a write tool is approved by `Auto` mode, session grant, or batch prompt
- WHEN the decision is made
- THEN a JSONL record is appended to `~/.inout/audit.jsonl`
- AND the record contains the tool name and decision

#### Scenario: Blocked decision is audited
- GIVEN a write tool is blocked by `Deny` mode or user refusal
- WHEN the decision is made
- THEN a JSONL record is appended to `~/.inout/audit.jsonl`
- AND the record contains the tool name, decision, and reason

### Requirement: Project linting gate
The whole workspace SHALL pass `cargo clippy --all-targets -- -D warnings`.

#### Scenario: Clippy passes
- GIVEN a clean checkout of the workspace
- WHEN `cargo clippy --all-targets -- -D warnings` runs
- THEN it exits successfully with no warnings treated as errors
