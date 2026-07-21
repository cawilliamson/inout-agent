# Scripting Specification

## Purpose
The rhai scripting tier: runtime-loaded `.rhai` extensions that register tools, commands, and hooks through the same `ExtensionApi` used by rust crate extensions. This is the user-facing extension surface. First-party rust crates handle performance-critical or security-critical features; rhai scripts handle custom glue, project-specific workflows, and third-party integrations.

## Requirements

### Requirement: Script discovery
The system SHALL load rhai scripts from `~/.inout/extensions/*.rhai` (user-global), `.inout/extensions/*.rhai` (project-local), and paths listed in `config.extension_paths`. User-global scripts load first, then project-local, then additional paths. Within a directory, files are sorted alphabetically. A script's registered name is its file stem.

#### Scenario: User-global script loads
- GIVEN a file `~/.inout/extensions/foo.rhai` containing a valid `register(api)` function
- WHEN the agent starts up
- THEN the script is loaded and registered under the name `foo`

#### Scenario: Project-local script loads after user-global
- GIVEN a user-global script `~/.inout/extensions/foo.rhai` and a project-local script `.inout/extensions/foo.rhai`
- WHEN the agent starts up
- THEN the project-local script loads after the user-global script
- AND the project-local registration wins on transform hooks (last-loaded wins)

#### Scenario: Invalid path is skipped
- GIVEN a non-existent directory in `config.extension_paths`
- WHEN the agent loads extensions
- THEN the path is skipped without error

### Requirement: Script lifecycle
The system SHALL load each script in an isolated `rhai::Engine`, parse the AST, register host functions, and call the script's `register(api)` function. A panic or exception in one script SHALL NOT affect other scripts.

#### Scenario: Parse error in one script does not prevent others loading
- GIVEN two scripts `a.rhai` (valid) and `b.rhai` (syntax error) in the same directory
- WHEN the agent loads extensions
- THEN script `a` registers successfully
- AND script `b` is skipped with a log message

#### Scenario: Script without register function is skipped
- GIVEN a `.rhai` file with no `register` function
- WHEN the agent loads extensions
- THEN the script is skipped with a warning

### Requirement: ExtensionApi surface for scripts
The system SHALL expose an `api` object to scripts inside `register(api)` with keys: `register_tool`, `unregister_tool`, `register_command`, `unregister_command`, `on`, `observe`, `config`, `host`.

#### Scenario: Script registers a tool
- GIVEN a script calling `api.register_tool("my_grep", "desc", schema_json, fn(args) { ... })`
- WHEN the script is registered
- THEN the tool appears in the `ToolRegistry` and is dispatchable

#### Scenario: Script registers a slash command
- GIVEN a script calling `api.register_command("todo", "desc", fn(args) { ... })`
- WHEN the script is registered
- THEN the command appears in the `CommandRegistry` and is dispatchable

#### Scenario: Script subscribes to a hook
- GIVEN a script calling `api.on("tool_call", fn(event) { ... })`
- WHEN the hook fires
- THEN the script's handler receives the event map

#### Scenario: Script observes all events read-only
- GIVEN a script calling `api.observe(fn(event) { ... })`
- WHEN any event fires
- THEN the observer handler receives the event map
- AND the return value is ignored

### Requirement: Host functions with permissions
The system SHALL expose host functions via `api.host` with permission gating: `read_file` (Read), `write_file` (Write), `run_command` (Shell), `http_get` (Network), `http_post` (Network), `now_unix_ms` (Read), `log` (Read), `config_get` (Read). Read is allowed by default. Write, Shell, and Network require env flags or config flags.

#### Scenario: Read host function works by default
- GIVEN a script calling `api.host.read_file("path")`
- WHEN no permission flags are set
- THEN the file contents are returned

#### Scenario: Write host function blocked without flag
- GIVEN a script calling `api.host.write_file("path", "content")`
- WHEN `INOUT_SCRIPTS_ALLOW_WRITE` is not set and `config.scripts.allow_write` is not true
- THEN the engine throws an error
- AND the error is caught and returned as an error tool result

#### Scenario: Shell host function blocked without flag
- GIVEN a script calling `api.host.run_command("ls", [])`
- WHEN `INOUT_SCRIPTS_ALLOW_SHELL` is not set and `config.scripts.allow_shell` is not true
- THEN the engine throws an error

#### Scenario: Network host function blocked without flag
- GIVEN a script calling `api.host.http_get("https://example.com")`
- WHEN `INOUT_SCRIPTS_ALLOW_NETWORK` is not set and `config.scripts.allow_network` is not true
- THEN the engine throws an error

### Requirement: Event maps
The system SHALL pass events to rhai scripts as maps containing `type: string` plus event-type-specific keys. Transform events return a map that replaces the event for the next handler. The `tool_call` event SHALL support `#{ block: true, reason: "..." }` or `#{ block: false }` returns.

#### Scenario: tool_call hook blocks a tool
- GIVEN a script with `api.on("tool_call", fn(event) { if event.name == "bash" { return #{ block: true, reason: "scripts cannot invoke shell" }; } return #{ block: false }; })`
- WHEN the agent dispatches a `bash` tool call
- THEN the tool call is blocked
- AND the reason is returned

#### Scenario: tool_call hook allows a tool
- GIVEN the same script
- WHEN the agent dispatches a non-`bash` tool call
- THEN the tool call proceeds

### Requirement: Tool handler signature
The system SHALL call tool handlers with `fn(args: Map) -> String`. `args` is a map parsed from the LLM's tool-use JSON. The return string becomes the tool result content. Throwing an exception produces an error tool result.

#### Scenario: Tool handler returns string
- GIVEN a registered rhai tool returning `"result text"` from its handler
- WHEN the agent dispatches the tool
- THEN the tool result content is `"result text"`

#### Scenario: Tool handler throws exception
- GIVEN a registered rhai tool throwing an exception in its handler
- WHEN the agent dispatches the tool
- THEN the tool result is an error containing the exception message

### Requirement: Command handler signature
The system SHALL call command handlers with `fn(args: Array) -> String`. The binary prints the returned string.

#### Scenario: Command handler returns string
- GIVEN a registered rhai command returning `"output"` from its handler
- WHEN the binary dispatches the command
- THEN `"output"` is printed

### Requirement: Hot-reload
The system SHALL support hot-reload during development when `INOUT_SCRIPTS_HOT_RELOAD=1` is set. On each new turn, the core re-discovers and re-parses scripts. Changed scripts are re-registered. Unchanged scripts are kept. In production, scripts load once at startup.

#### Scenario: Changed script is re-registered
- GIVEN `INOUT_SCRIPTS_HOT_RELOAD=1` and a script `foo.rhai` that has been modified on disk
- WHEN a new turn starts
- THEN `foo.rhai` is re-parsed and re-registered

#### Scenario: Unchanged script is not re-compiled
- GIVEN `INOUT_SCRIPTS_HOT_RELOAD=1` and a script `bar.rhai` that has not changed
- WHEN a new turn starts
- THEN `bar.rhai` is not re-parsed

### Requirement: Script isolation
The system SHALL give each script its own `rhai::Engine` instance. The Engine is `sync`-enabled because scripts may be invoked from async hooks.

#### Scenario: One script crashing does not affect others
- GIVEN two scripts `a.rhai` and `b.rhai` with separate engines
- WHEN script `a` panics during a hook invocation
- THEN script `b` continues to function normally