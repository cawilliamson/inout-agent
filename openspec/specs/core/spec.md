# Core Specification

## Purpose
The core crate is the substrate: agent loop, data types, provider trait, replay client, hook bus, tool and command registries, extension trait and loader, rhai runtime, minimal config, and in-memory session. Tools, jail, audit, persistence, context management, skills, HTTP provider, and observability are all extensions. Core must build and run with no extension crates on its dependency list.

## Requirements

### Requirement: Independent core build
The core crate SHALL build with no extension crates in its dependency graph.

#### Scenario: Clean core crate builds alone
- GIVEN a workspace containing only the core crate and its declared third-party dependencies
- WHEN `cargo build -p inout-core` is run
- THEN the build succeeds without any first-party extension crate being compiled or required

### Requirement: Core dependency allow-list
The core crate SHALL depend only on `serde`, `serde_json`, `tokio`, `async-trait`, `anyhow`, `thiserror`, and `rhai`.

#### Scenario: Cargo.toml dependency audit
- GIVEN the core crate's `Cargo.toml`
- WHEN the dependency list is inspected
- THEN it contains exactly the crates named above
- AND it does not contain `reqwest`, `rusqlite`, `tree-sitter`, `ratatui`, `crossterm`, or any other first-party extension crate

### Requirement: State machine transitions
The session state machine SHALL transition only through legal states and events.

#### Scenario: User message starts a turn
- GIVEN a session in the `awaiting_user` state
- WHEN a user message is received
- THEN the session transitions to the `thinking` state

#### Scenario: Tool calls from thinking
- GIVEN a session in the `thinking` state
- WHEN the assistant requests one or more tool calls
- THEN the session transitions to the `tool_running` state

#### Scenario: Final response from thinking
- GIVEN a session in the `thinking` state
- WHEN the assistant produces text without further tool calls
- THEN the session transitions to the `responding` state

#### Scenario: Tool results return to thinking
- GIVEN a session in the `tool_running` state
- WHEN all tool results have been collected
- THEN the session transitions back to the `thinking` state

#### Scenario: Turn completion returns to idle
- GIVEN a session in the `responding` state
- WHEN the response is delivered
- THEN the session transitions to the `awaiting_user` state

#### Scenario: Illegal transition is rejected
- GIVEN a session in the `awaiting_user` state
- WHEN an event that is not a user message is applied
- THEN the transition is rejected
- AND the session remains in the `awaiting_user` state

### Requirement: Replay client ordering
The replay client SHALL return each pre-recorded response in the order provided and SHALL panic when the queue is exhausted.

#### Scenario: Responses returned in order
- GIVEN a replay client loaded with responses `A`, `B`, `C`
- WHEN the client is invoked three times in succession
- THEN it returns `A`, then `B`, then `C`

#### Scenario: Empty queue panics
- GIVEN a replay client with no remaining responses
- WHEN it is invoked again
- THEN the call panics

### Requirement: Hook bus ordering and observer immutability
The hook bus SHALL fire handlers in registration order. Observers SHALL receive every emitted event but SHALL NOT influence subsequent processing.

#### Scenario: Handlers fire in registration order
- GIVEN handlers registered as `first`, `second`, `third` for the same event type
- WHEN that event is emitted
- THEN the `first` handler runs before `second`, and `second` before `third`

#### Scenario: Observers are read-only
- GIVEN an observer registered for an event type
- WHEN that event is emitted
- THEN the observer receives the event
- AND any mutation attempted by the observer does not affect the event seen by later handlers or the final result

### Requirement: Active tool set validation
The tool registry SHALL reject attempts to set the active tool set when an unknown name is supplied or when duplicate names are supplied.

#### Scenario: Unknown tool name rejected
- GIVEN a registry containing tools named `read` and `write`
- WHEN `set_active` is called with names `["read", "bash"]`
- THEN the call returns an error
- AND the active set remains unchanged

#### Scenario: Duplicate tool name rejected
- GIVEN a registry containing a tool named `read`
- WHEN `set_active` is called with names `["read", "read"]`
- THEN the call returns an error
- AND the active set remains unchanged

### Requirement: Extension registration surface
An extension SHALL be able to register a tool, a command, and a hook in a single call to the extension API.

#### Scenario: Combined extension registration
- GIVEN an extension implementation that calls `register_tool`, `register_command`, and `on` inside its registration method
- WHEN the extension is registered with an extension API instance
- THEN the tool appears in the tool registry
- AND the command appears in the command registry
- AND the hook handler is subscribed to the hook bus

### Requirement: Script extension loading
The script extension SHALL parse a `.rhai` file and register its contributions through the same extension API used by Rust extensions.

#### Scenario: Valid rhai file registers via ExtensionApi
- GIVEN a file `custom.rhai` containing a `register(api)` function that calls `api.register_tool`
- WHEN `ScriptExtension::from_file` is called for that file and the extension is registered
- THEN the tool appears in the tool registry
- AND the extension name matches the file stem

### Requirement: Agent loop full turn
The agent loop SHALL run a complete turn from user message through provider tool calls, tool execution, and final assistant response when using a replay client and a registered test tool.

#### Scenario: Full turn with tool use
- GIVEN an agent configured with a replay client that returns tool calls and then a final response
- AND a test tool registered and active
- WHEN `run_turn` is invoked with a user message
- THEN the provider is called once for the assistant request
- AND the test tool is dispatched with the provided arguments
- AND the final assistant text response is returned
- AND all messages are appended to the session in the correct order

### Requirement: Agent loop hook transitions
The agent loop SHALL emit `before_agent_start`, `context`, `after_provider_response`, `tool_call`, `tool_result`, and `turn_end` hooks during a turn.

#### Scenario: All expected hooks fire
- GIVEN an agent with handlers subscribed to each of the six hook types
- WHEN `run_turn` completes a full turn that includes a tool call
- THEN `before_agent_start` fires
- AND `context` fires before the provider call
- AND `after_provider_response` fires after the provider call
- AND `tool_call` fires for the dispatched tool
- AND `tool_result` fires after tool execution
- AND `turn_end` fires before the method returns

### Requirement: Core linting
The core crate SHALL pass `cargo clippy` with warnings denied.

#### Scenario: Clippy green on core alone
- GIVEN the core crate source and tests
- WHEN `cargo clippy --all-targets -p inout-core -- -D warnings` is run
- THEN the command exits successfully with zero warnings

### Requirement: Data types
The core SHALL provide data types for messages, content blocks, usage, LLM requests, LLM responses, permission classes, and tool errors.

#### Scenario: Content block variants are distinguishable
- GIVEN a text content block and a tool-use content block
- WHEN their serialized or pattern-matched representations are compared
- THEN they are not equal
- AND the text block carries text
- AND the tool-use block carries tool name, identifier, and input

#### Scenario: Tool result captures success and failure
- GIVEN a successful tool execution returning `"ok"`
- WHEN a tool result content block is created
- THEN it carries the matching tool-use identifier and content
- AND it is marked as not an error

#### Scenario: Tool error carries descriptive message
- GIVEN a tool execution failure with message `"disk full"`
- WHEN a tool error is produced
- THEN its display representation contains `"disk full"`

### Requirement: Provider trait
The core SHALL define an asynchronous provider trait that accepts an LLM request and returns an LLM response, without providing a network implementation.

#### Scenario: Replay client implements provider trait
- GIVEN a type implementing the provider trait
- WHEN it is passed to the agent constructor
- THEN the agent can invoke it during a turn
- AND the trait remains defined in the core crate alone

### Requirement: Minimal configuration
The core configuration SHALL expose fields for model, provider, repository root, extension paths, maximum turns, and an extension-specific extra map.

#### Scenario: Config loads required fields
- GIVEN a configuration source containing `model`, `provider`, `repo_root`, `max_turns`, and `extension_paths`
- WHEN the configuration is parsed
- THEN all top-level fields are accessible
- AND extension-specific sub-tables are preserved inside the extra map

#### Scenario: Extra config preserved for extensions
- GIVEN a configuration source containing an `[extensions.audit]` table
- WHEN the configuration is parsed by the core
- THEN the audit table is available under `config.extra["extensions"]["audit"]`

### Requirement: In-memory session
The core session SHALL store messages in memory and expose the current state.

#### Scenario: Messages append and retrieve
- GIVEN a new session
- WHEN messages are appended one by one
- THEN `messages()` returns them in the order appended

#### Scenario: Session state tracks transitions
- GIVEN a new session
- WHEN a legal state transition succeeds
- THEN `state()` returns the new state
- AND when an illegal transition is rejected, `state()` remains unchanged

### Requirement: Agent loop structure
The agent loop SHALL run turns, fire hooks at each transition, and enforce a maximum turn limit.

#### Scenario: Max turns guard prevents runaway loop
- GIVEN a replay client that always returns tool calls and a configured `max_turns` of 3
- WHEN `run_turn` is invoked
- THEN the loop terminates no later than after 3 iterations
- AND a terminal response or error is returned instead of continuing indefinitely

#### Scenario: Hook fired at every transition
- GIVEN a replay client and subscribed transition hook handlers
- WHEN `run_turn` runs to completion
- THEN a hook fires at each transition between session states
