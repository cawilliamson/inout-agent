# Extensions Specification

## Purpose
The extension surface: a typed hook bus, tool and command registries, the `Extension` trait, the `ExtensionApi`, and a shared registration surface used by both compiled rust crate extensions and runtime-loaded rhai script extensions.

## Requirements

### Requirement: Hook bus emits handlers in order
`HookBus::emit` SHALL fire every registered handler for the event type in the order it was registered. Observers SHALL receive the event read-only; their return values SHALL be ignored and SHALL NOT affect the event or subsequent handlers.

#### Scenario: Handlers fire in registration order
- GIVEN three handlers registered for `context` in order A, B, C
- WHEN `HookBus::emit` is called for a `context` event
- THEN A runs before B
- AND B runs before C

#### Scenario: Observers cannot mutate events
- GIVEN an observer registered with `HookBus::observe` that mutates the passed event
- WHEN `HookBus::emit` is called for that event type
- THEN the mutation performed by the observer is not visible to later handlers

### Requirement: Transform events chain
Transform events SHALL pass the previous handler's output to the next handler. Handler B for a transform event SHALL operate on the result returned by handler A.

#### Scenario: Context handler sees prior handler output
- GIVEN a `context` handler A that rewrites `messages` to `["A"]`
- AND a `context` handler B that appends `"B"` to `messages`
- WHEN `HookBus::emit` is called for a `context` event
- THEN B receives a `messages` value containing both `"A"` and `"B"`

### Requirement: Block events early-exit
Block events SHALL stop processing on the first handler that returns a block. No later handler for that event type SHALL run once a block has been produced.

#### Scenario: tool_call returns on first block
- GIVEN a `tool_call` handler A that returns `{ block: false }`
- AND a `tool_call` handler B that returns `{ block: true, reason: "blocked" }`
- AND a `tool_call` handler C registered after B
- WHEN `HookBus::emit` is called for a `tool_call` event
- THEN A runs
- AND B runs
- AND C does not run
- AND the result indicates the tool is blocked with reason `"blocked"`

### Requirement: Tool registry active set validation
`ToolRegistry::set_active` SHALL reject any tool name that is not registered. It SHALL also reject duplicate names in the supplied list.

#### Scenario: Unknown tool name rejected
- GIVEN a `ToolRegistry` containing tools named `read` and `write`
- WHEN `set_active` is called with `["read", "edit"]`
- THEN the call returns an error because `edit` is not registered

#### Scenario: Duplicate tool name rejected
- GIVEN a `ToolRegistry` containing tools named `read` and `write`
- WHEN `set_active` is called with `["read", "read"]`
- THEN the call returns an error because `read` is duplicated

### Requirement: Tool trait default methods compile
The `Tool` trait SHALL provide default method bodies for every method other than `name`, `schema`, and `run`. A type implementing only `name`, `schema`, and `run` SHALL compile.

#### Scenario: Minimal tool implementation compiles
- GIVEN a struct `MinimalTool` that implements only `name`, `schema`, and `run`
- WHEN the project is compiled
- THEN compilation succeeds without `permission_class`, `affected_path`, `retry_safe`, or `shows_inline_output` overrides

### Requirement: Rust extension can register multiple surface items
The rust `Extension::register` method SHALL be able to add a tool, register a command, and subscribe to a hook in a single call to `ExtensionApi`.

#### Scenario: Single extension registers tool, command, and hook
- GIVEN a rust extension whose `register` implementation calls `api.tools.register(...)`, `api.commands.register(...)`, and `api.hooks.on(...)`
- WHEN the extension is loaded
- THEN the tool appears in the `ToolRegistry`
- AND the command appears in the `CommandRegistry`
- AND the hook handler is registered on the `HookBus`

### Requirement: Rhai script extension loads and runs register
`ScriptExtension` SHALL parse a `.rhai` file into an isolated engine and call the script's `register(api)` function, passing a script-facing `ExtensionApi` object.

#### Scenario: Script with register function loads
- GIVEN a file `custom.rhai` containing a valid `register(api)` function
- WHEN `ScriptExtension` loads the file
- THEN the script parses successfully
- AND `register(api)` is invoked

#### Scenario: Script without register function is skipped
- GIVEN a file `empty.rhai` containing no `register` function
- WHEN `ScriptExtension` loads the file
- THEN the file is skipped
- AND no error aborts loading

### Requirement: Rhai script registers a dispatchable tool
A rhai script SHALL be able to register a tool through the script `ExtensionApi` and have that tool dispatched by `ToolRegistry`.

#### Scenario: Script tool is dispatchable
- GIVEN a script `foo.rhai` that calls `api.register_tool("my_tool", ...)` with a handler returning `"result"`
- WHEN the agent dispatches `my_tool`
- THEN the returned tool result content is `"result"`

### Requirement: Rhai script registers a tool_call blocker
A rhai script SHALL be able to register a `tool_call` hook that returns a block, preventing the matched tool from executing.

#### Scenario: Script hook blocks bash
- GIVEN a script that registers `api.on("tool_call", fn(event) { if event.name == "bash" { return #{ block: true, reason: "blocked by script" }; } return #{ block: false }; })`
- WHEN the agent dispatches a `bash` tool call
- THEN the tool is blocked
- AND the reason `"blocked by script"` is returned

### Requirement: Rhai script unregisters a first-party tool
The script `ExtensionApi` SHALL expose a method to unregister a tool by name, including tools registered by first-party rust extensions.

#### Scenario: Script removes builtin read tool
- GIVEN a loaded first-party extension that registers a tool named `read`
- AND a script that calls `api.unregister_tool("read")`
- WHEN the script is loaded after the first-party extension
- THEN `read` is no longer present in the `ToolRegistry`

### Requirement: Core crate has no extension crate dependencies
The `inout-core` crate SHALL compile with no first-party extension crates on its dependency list.

#### Scenario: Core manifest contains no extension crates
- GIVEN the `inout-core` `Cargo.toml`
- WHEN its `[dependencies]` and `[dev-dependencies]` are inspected
- THEN no crate named `inout-ext-*` is listed

### Requirement: Clippy passes
The workspace SHALL build with clippy warnings denied.

#### Scenario: Clippy is green
- GIVEN a clean checkout
- WHEN `cargo clippy --all-targets -- -D warnings` is run
- THEN the command exits successfully

### Requirement: Hook event types are defined
The system SHALL define the following event types with the stated semantics: `before_agent_start`, `context`, `before_provider_request`, `before_provider_payload`, `before_provider_headers`, `after_provider_response`, `tool_call`, `tool_result`, `message_end`, `turn_end`, `agent_end`, `session_start`, `session_end`, `model_select`.

#### Scenario: All event types are registerable
- GIVEN the hook bus
- WHEN a handler is registered for each event type
- THEN registration succeeds for every listed type

#### Scenario: Transform event types chain
- GIVEN two handlers registered for `context`, `before_agent_start`, `before_provider_request`, `before_provider_payload`, or `model_select`
- WHEN the event fires
- THEN the second handler receives the output of the first handler

#### Scenario: Block event types early-exit
- GIVEN two handlers registered for `tool_call`
- WHEN the first handler returns a block
- THEN the second handler does not run

### Requirement: Tool trait surface is defined
The `Tool` trait SHALL specify `name`, `schema`, `run`, `permission_class`, `affected_path`, `retry_safe`, and `shows_inline_output`. `schema` and `run` are required; the remainder have defaults.

#### Scenario: Tool exposes metadata
- GIVEN a tool implementation
- WHEN its `name`, `schema`, `permission_class`, `retry_safe`, and `shows_inline_output` are queried
- THEN each returns the declared value or its default

#### Scenario: Tool declares affected path
- GIVEN a tool whose `affected_path` returns `Some("/tmp/foo")` for the supplied args
- WHEN a hook inspects the affected path before dispatch
- THEN the returned path is `/tmp/foo`

### Requirement: Command registry surface is defined
The system SHALL provide a `CommandRegistry` that registers named slash commands and dispatches them with mutable access to session state.

#### Scenario: Command registers and dispatches
- GIVEN a command named `/todo` registered with a handler returning `"ok"`
- WHEN the command is dispatched
- THEN the output is `"ok"`
- AND the handler had mutable access to session, tools, hooks, and config

### Requirement: ExtensionApi surface is shared
The `ExtensionApi` SHALL expose the `ToolRegistry`, `CommandRegistry`, `HookBus`, `Config`, and `Session` to rust extension implementations.

#### Scenario: Rust extension can access all surfaces
- GIVEN an `ExtensionApi` value
- WHEN an extension reads `api.tools`, `api.commands`, `api.hooks`, `api.config`, and `api.session`
- THEN all fields are accessible

### Requirement: Registration order and override semantics are defined
Extensions SHALL load in the order: first-party rust extensions, feature-gated rust extensions, user-global rhai scripts, project-local rhai scripts. Transform hooks SHALL use last-loaded-wins chaining; block hooks SHALL allow earlier-loaded handlers to short-circuit later ones.

#### Scenario: User script overrides first-party transform
- GIVEN a first-party rust extension that registers a `context` transform
- AND a user rhai script that registers a `context` transform returning `{ messages: ["script wins"] }`
- WHEN the user script loads after the first-party extension
- THEN the final `messages` value is `["script wins"]`

#### Scenario: User script blocks first-party tool call
- GIVEN a first-party rust extension that registers a `tool_call` handler allowing all calls
- AND a user rhai script that registers a `tool_call` handler returning `{ block: true }` for `bash`
- WHEN the user script loads after the first-party extension
- AND a `bash` tool call is emitted
- THEN the `bash` tool call is blocked

### Requirement: Extension naming is defined
Rust extension crate names SHALL follow `inout-ext-<kebab-name>`. `Extension::name()` SHALL return `<kebab-name>`. Rhai script file names SHALL be `<kebab-name>.rhai`, and the registered extension name SHALL be the file stem.

#### Scenario: Rust crate name matches extension name
- GIVEN a crate named `inout-ext-builtin-tools`
- WHEN its `Extension::name()` is queried
- THEN it returns `"builtin-tools"`

#### Scenario: Script file stem becomes extension name
- GIVEN a file `my-glue.rhai`
- WHEN it is loaded as a script extension
- THEN its registered name is `"my-glue"`
