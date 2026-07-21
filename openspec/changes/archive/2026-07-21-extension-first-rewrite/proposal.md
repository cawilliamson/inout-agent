# Proposal: Extension-first architecture rewrite

## Intent

Restructure the project from a single crate into an extension-first workspace. Build `inout-core` as the substrate (types, config, hooks, tools, extension trait, scripting). Add a sessions extension (jsonl repo, compaction, continuity, branching). Add a skills extension (loader, trigger, budget, scope, trace). Add Rhai script extensions (bash, read, write, edit, grep, glob, context, fullview, commands). Redesign the TUI with cached values, slash commands, a context viewer, and full view.

## Scope

### In scope

- Workspace restructure into `crates/inout-core`, `crates/inout`, `crates/inout-ext-sessions`, and `crates/inout-ext-skills`.
- Core substrate: types, config, hook bus, tool registry, extension trait, Rhai scripting runtime.
- Sessions: jsonl append-only repo, session entry tree, branching, compaction, continuity handoff files.
- Skills: frontmatter parsing, trigger matching, budget ranking, stack detection, skill trace.
- Rhai extensions: bash, read, write, edit, grep, glob, context view, full view, commands.
- LLM client: streaming with reasoning support, cost tracking.
- TUI: cached values (fix `blocking_lock` crash), slash command suggestions, context viewer overlay, full view inline rendering, context meter, reasoning toggle.
- Linting: `clippy.toml`, `rustfmt.toml`, `deny.toml`, `.cargo/config.toml`.

### Out of scope

- Observability extension.
- Permissions extension.
- Secret scanner.
- Undo ledger.
- HTTP provider extension.
- AST code index.
- MCP extension.

## Approach

Mine Pi for extension surfaces, observability model, and session tree. Mine Zap for Rust patterns, raw-stream logging, skill injection, edit ledger, and security stack. `inout-core` is the substrate; everything else is an extension. First-party functionality lives in Rust crates; user-facing customisation is delivered through Rhai scripts.
