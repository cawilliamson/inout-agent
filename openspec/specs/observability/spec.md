# Observability Specification

## Purpose
Make the agent observable without binding inout-core to any vendor. The system SHALL emit stable, structured lifecycle events and capture the raw provider payload, so callers can trace execution, audit decisions, and inspect LLM traffic. A first-party observability extension provides the default subscriber bus, raw stream files, audit log, and per-turn cost display.

## Requirements

### Requirement: Core builds without observability extension
inout-core SHALL compile and pass clippy without depending on inout-ext-observability. The observability surface is a trait and event bus that lives in core; concrete sinks and hooks are implemented by the extension.

#### Scenario: Core compiles standalone
- GIVEN the inout-core crate is built
- WHEN inout-ext-observability is not in the dependency graph
- THEN the build succeeds
- AND `cargo clippy --all-targets -- -D warnings` passes for inout-core

---

### Requirement: Observability trait and subscriber bus
inout-core SHALL define an `Observability` trait with a default in-memory subscriber bus. The bus SHALL allow any number of subscribers to receive events. The trait methods SHALL provide access to the current span context, run a closure under a new context, and emit events.

#### Scenario: Default bus exists without subscribers
- GIVEN an `Observability` implementation created with default settings
- WHEN no subscribers have been added
- THEN `has_subscribers()` returns false
- AND `emit()` does not panic

#### Scenario: Subscriber receives an emitted event
- GIVEN a subscriber added to the default bus
- WHEN an `ObservabilityEvent` is emitted
- THEN the subscriber receives the event exactly once

#### Scenario: Multiple subscribers receive an event
- GIVEN two subscribers added to the default bus
- WHEN an `ObservabilityEvent` is emitted
- THEN both subscribers receive the event

---

### Requirement: Span context propagation
Every event SHALL carry a span context containing `trace_id`, `span_id`, and optional `parent_span_id`. A `trace_span` helper SHALL create a child span, run a closure under that context, and emit `start` and `end` (or `error`) events.

#### Scenario: `trace_span` emits start and end on success
- GIVEN a `trace_span` call around a closure that returns a value
- WHEN the closure completes successfully
- THEN an event with kind `start` is emitted with a new `span_id` and the current span as `parent_span_id`
- AND an event with kind `end` is emitted with the same `span_id` and `trace_id`
- AND the closure's return value is returned to the caller

#### Scenario: `trace_span` emits start and error on failure
- GIVEN a `trace_span` call around a closure that returns an error
- WHEN the closure returns an error
- THEN an event with kind `start` is emitted
- AND an event with kind `error` is emitted with the same `span_id`, `trace_id`, and an error payload
- AND the closure's error is returned to the caller

#### Scenario: Nested spans carry the correct parent
- GIVEN an outer `trace_span` around an inner `trace_span`
- WHEN both complete
- THEN the inner start event's `parent_span_id` equals the outer start event's `span_id`
- AND the inner `trace_id` equals the outer `trace_id`

---

### Requirement: Event kinds and event names
The system SHALL emit four event kinds: `start`, `end`, `error`, and `event`. The default extension SHALL emit at least the following event names: `inout.agent.prompt`, `inout.agent.turn`, `inout.agent.tool_call`, `inout.agent.session.append_entry`, `inout.ai.provider.request`, `inout.ai.provider.first_token`, `inout.ai.provider.usage`, and `inout.ai.provider.retry`.

#### Scenario: `start` and `end` events have matching IDs
- GIVEN an event name is emitted under `trace_span`
- WHEN the span ends
- THEN the `start` and `end` events share the same `span_id`, `trace_id`, and `name`

#### Scenario: `error` event replaces `end` on failure
- GIVEN an event name is emitted under `trace_span`
- WHEN the span errors
- THEN an `error` event is emitted instead of an `end` event
- AND no `end` event for that `span_id` is emitted

#### Scenario: `event` kind is emitted for point-in-time occurrences
- GIVEN a subscriber is registered for point-in-time events
- WHEN the system emits a non-span event such as `inout.ai.provider.first_token`
- THEN the subscriber receives an event with kind `event`
- AND the event includes the current `trace_id`, `span_id`, and timestamp

---

### Requirement: Raw provider payload capture
The extension SHALL register a `before_provider_payload` hook that writes the full provider request payload to `~/.inout/llm_requests/<ts>_<slug>.json`, where `<ts>` is a timestamp in milliseconds and `<slug>` is derived from the model name.

#### Scenario: Hook writes payload file
- GIVEN a `before_provider_payload` hook is installed
- WHEN a provider call is made for model `openai/gpt-4o`
- THEN a file matching `~/.inout/llm_requests/<ts>_openai_gpt_4o.json` is created
- AND the file contains the full request payload as pretty-printed JSON

#### Scenario: Filename is safe for filesystems
- GIVEN a provider call for model `anthropic/claude-3.5-sonnet`
- WHEN the hook derives the filename slug
- THEN the filename contains no path separators, colons, spaces, or other unsafe characters

---

### Requirement: Direction log for provider calls
The extension SHALL append one line per provider request and one line per provider response to `~/.inout/llm.log`. Each request line SHALL contain `REQUEST`, the model identifier, and the path to the payload file. Each response line SHALL contain `RESPONSE`, the model identifier, and the response status.

#### Scenario: One request/response pair per provider call
- GIVEN a provider call is made and succeeds
- WHEN the extension processes the call
- THEN `~/.inout/llm.log` contains exactly one `REQUEST` line for that call
- AND `~/.inout/llm.log` contains exactly one `RESPONSE` line for that call
- AND the `REQUEST` line references the payload file written by the `before_provider_payload` hook

#### Scenario: Failed response is still logged
- GIVEN a provider call that returns a non-success status
- WHEN the extension processes the call
- THEN a `RESPONSE` line is appended with the failure status

---

### Requirement: Audit log structure
The extension SHALL append one JSON line per tool call and one JSON line per permission decision to `~/.inout/audit.jsonl`. Each record SHALL contain: `ts` in RFC 3339 format, `event` (`tool_call` or `permission_decision`), optional `tool`, optional `permission`, optional `affected_path`, optional `exit_code`, optional `duration_ms`, and optional `trace_id` and `span_id`.

#### Scenario: Tool call produces an audit record
- GIVEN a tool named `bash` is invoked with exit code `0`
- WHEN the extension processes the tool call
- THEN `~/.inout/audit.jsonl` gains a record with `event: "tool_call"`, `tool: "bash"`, and `exit_code: 0`
- AND the record includes the current `trace_id` and `span_id`

#### Scenario: Permission decision produces an audit record
- GIVEN a permission is requested for a tool
- WHEN the permission is granted
- THEN `~/.inout/audit.jsonl` gains a record with `event: "permission_decision"` and the decision value

---

### Requirement: Redaction is safe by default
`Redacted<T>` with the default policy SHALL never serialize prompt or completion content. The default policy SHALL drop the value. Fields marked sensitive, including prompts, completions, tool arguments, tool results, shell output, provider request/response bodies, API keys, and headers, SHALL be redacted unless explicitly opted in.

#### Scenario: Default redaction removes content
- GIVEN a `Redacted<Value>` created for a prompt payload with the default policy
- WHEN it is serialized to JSON
- THEN the output contains no prompt text

#### Scenario: Completion content is also redacted
- GIVEN a `Redacted<Value>` created for a completion payload with the default policy
- WHEN it is serialized to JSON
- THEN the output contains no completion text

---

### Requirement: Content capture is opt-in
The system SHALL enable content capture only when `INOUT_OBSERVE_CONTENT=1` is set. When enabled, `Redacted<T>` may contain the raw value under the unsafe policy. When disabled, even unsafe policy values SHALL be omitted from serialized output.

#### Scenario: Content captured when env flag is set
- GIVEN `INOUT_OBSERVE_CONTENT=1`
- WHEN a prompt payload is redacted with the unsafe policy
- THEN the serialized output includes the prompt text

#### Scenario: Content omitted when env flag is unset
- GIVEN `INOUT_OBSERVE_CONTENT` is unset or not `1`
- WHEN a prompt payload is redacted with the unsafe policy
- THEN the serialized output omits the prompt text

---

### Requirement: Log rotation on startup
The extension SHALL rotate logs on startup. `~/.inout/llm.log` and files in `~/.inout/llm_requests/` SHALL be trimmed to records/files from the last 24 hours. `~/.inout/audit.jsonl` SHALL be trimmed to records from the last 7 days.

#### Scenario: Old llm.log entries are removed
- GIVEN `~/.inout/llm.log` contains a line older than 24 hours
- WHEN the extension starts
- THEN that line is removed
- AND lines newer than 24 hours remain

#### Scenario: Old llm_requests files are deleted
- GIVEN `~/.inout/llm_requests/` contains a payload file older than 24 hours
- WHEN the extension starts
- THEN that file is deleted
- AND files newer than 24 hours remain

#### Scenario: Old audit records are removed
- GIVEN `~/.inout/audit.jsonl` contains a record older than 7 days
- WHEN the extension starts
- THEN that record is removed
- AND records newer than 7 days remain

---

### Requirement: Per-turn cost display
The extension SHALL compute a `TurnCost` for every turn from the LLM `Usage` response, skill trace token counts, and an estimate of the context window. `TurnCost` SHALL contain `input`, `output`, `cache_read`, `cache_write`, `skills_tokens`, `msg_tokens_estimate`, `ctx_pct`, and `usd`.

#### Scenario: Cost computed from usage
- GIVEN a completed turn with `Usage` reporting 100 input tokens, 50 output tokens, and a known per-token price
- WHEN the extension computes `TurnCost`
- THEN `TurnCost.input` equals 100
- AND `TurnCost.output` equals 50
- AND `TurnCost.usd` equals `(100 * price_in + 50 * price_out) / 1_000_000`

#### Scenario: Cache tokens are tracked
- GIVEN a completed turn with `Usage` reporting 20 cache_read tokens and 10 cache_write tokens
- WHEN the extension computes `TurnCost`
- THEN `TurnCost.cache_read` equals 20
- AND `TurnCost.cache_write` equals 10

#### Scenario: Skill tokens contribute to cost
- GIVEN a completed turn where skill trace reports 30 tokens consumed by skills
- WHEN the extension computes `TurnCost`
- THEN `TurnCost.skills_tokens` equals 30
- AND `TurnCost.usd` includes the skill token cost if skills are priced separately

#### Scenario: Context percentage is estimated
- GIVEN a model with a known context window and a turn with an estimated message token count
- WHEN the extension computes `TurnCost`
- THEN `TurnCost.msg_tokens_estimate` is populated
- AND `TurnCost.ctx_pct` is the percentage of the context window used, capped at 100

---

### Requirement: Clippy green
The entire workspace SHALL pass `cargo clippy --all-targets -- -D warnings`.

#### Scenario: CI lint runs clean
- GIVEN a clean checkout
- WHEN `cargo clippy --all-targets -- -D warnings` is run
- THEN it exits with status 0 and no warnings
