# MCP Specification

## Purpose
The system acts as a Model Context Protocol client using stdio transport and JSON-RPC 2.0. MCP servers are lazy-loaded: they remain pending until the model explicitly requests a connection, then the system spawns the server process, completes the handshake, fetches the available tools, and registers them within a single turn. Configuration is compatible with other agents, allowing shared global and project files.

## Requirements

### Requirement: Optional MCP build
The system SHALL compile the core crate without the MCP extension crate, and the binary SHALL compile with the MCP feature disabled.

#### Scenario: Core crate builds without MCP extension
- GIVEN the MCP extension crate is not in the core dependency tree
- WHEN the core crate is built
- THEN the build succeeds

#### Scenario: Binary builds with MCP feature disabled
- GIVEN the MCP feature is disabled in the binary crate
- WHEN the binary is built
- THEN the build succeeds

### Requirement: MCP feature builds cleanly
The MCP feature SHALL build with zero warnings under the project's clippy configuration, both enabled and disabled.

#### Scenario: Clippy with MCP feature enabled
- GIVEN the MCP feature is enabled
- WHEN `cargo clippy --all-targets -- -D warnings` runs
- THEN the command exits successfully

#### Scenario: Clippy with MCP feature disabled
- GIVEN the MCP feature is disabled
- WHEN `cargo clippy --all-targets -- -D warnings` runs
- THEN the command exits successfully

### Requirement: MCP config loading
The system SHALL load MCP server configuration from a global file at `~/.inout/mcp.json` and a project file at `.mcp.json`. When both files define a server with the same name, the project file entry SHALL override the global entry.

#### Scenario: Global config loads
- GIVEN a global `~/.inout/mcp.json` containing a server named `fetch`
- WHEN configuration is loaded
- THEN the `fetch` server is available

#### Scenario: Project config overrides global
- GIVEN a global `~/.inout/mcp.json` defining server `fetch` with command `global-bin`
- AND a project `.mcp.json` defining server `fetch` with command `project-bin`
- WHEN configuration is loaded
- THEN the resolved command for `fetch` is `project-bin`

### Requirement: MCP config validation
The system SHALL validate each server entry's command. Commands containing shell metacharacters or path traversal SHALL be rejected. Entries without a command SHALL be silently skipped to preserve compatibility with foreign agent configuration.

#### Scenario: Shell metacharacters rejected
- GIVEN a config entry with command `npx ; rm -rf /`
- WHEN the entry is validated
- THEN validation fails

#### Scenario: Path traversal rejected
- GIVEN a config entry with command `../../bin/mcp-server`
- WHEN the entry is validated
- THEN validation fails

#### Scenario: Entries without command skipped
- GIVEN a config entry that has only a URL and no command field
- WHEN the configuration is loaded
- THEN the entry is skipped without error

### Requirement: MCP server protocol
The system SHALL communicate with a spawned MCP server over stdio using JSON-RPC 2.0. It SHALL support the `tools/list` method to discover tools and the `tools/call` method to invoke them.

#### Scenario: List tools from server
- GIVEN a running MCP server exposing two tools
- WHEN the system sends a `tools/list` request
- THEN the response contains the two tool definitions

#### Scenario: Call tool on server
- GIVEN a running MCP server with a tool named `read_file`
- WHEN the system sends a `tools/call` request for `read_file` with valid arguments
- THEN the server returns a result for that tool

### Requirement: Lazy MCP server loading
MCP servers SHALL remain pending at startup; the system SHALL spawn zero server processes until a connection is explicitly requested.

#### Scenario: No processes at startup
- GIVEN two servers configured in `mcp.json`
- WHEN the agent starts
- THEN no MCP server processes are spawned

### Requirement: MCP connect tool
The system SHALL expose an `mcp_connect` tool that accepts a server name. When called, the system SHALL spawn the corresponding server process, perform the JSON-RPC handshake, fetch the tool list, and register the discovered tools within the current turn.

#### Scenario: Connect to named server
- GIVEN a configured server named `filesystem`
- WHEN the model calls `mcp_connect("filesystem")`
- THEN the server process spawns
- AND the handshake completes
- AND the tools are registered
- AND all of the above completes within one agentic turn

#### Scenario: Connect returns tool summary
- GIVEN a configured server named `filesystem` exposing tools `read_file`, `write_file`, and `list_directory`
- WHEN the model calls `mcp_connect("filesystem")`
- THEN the tool result lists the available tools

### Requirement: MCP tool proxy
The system SHALL wrap each discovered remote tool so that it implements the `Tool` interface. Tool invocations SHALL be proxied to the server via JSON-RPC `tools/call`. The permission class for every MCP tool SHALL be `Network`.

#### Scenario: MCP tool is dispatchable
- GIVEN a registered MCP tool named `read_file` backed by the `filesystem` server
- WHEN the agent dispatches `read_file` with arguments `{ "path": "/tmp/foo" }`
- THEN the server receives a `tools/call` request for `read_file`
- AND the returned result is forwarded to the agent

#### Scenario: MCP tool requires network permission
- GIVEN any registered MCP tool
- WHEN its permission class is queried
- THEN the class is `Network`
