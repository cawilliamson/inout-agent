# Proposal: Initial project scaffold

## Intent

Bootstrap a minimal Rust agent project with cargo workspace, initial spec documents, and basic project structure.

## Scope

### In scope

- Cargo workspace setup with a single binary crate (`twobobs`).
- Initial v1.0 OpenSpec document describing the target system.
- Basic project layout: `src/`, `tests/`, `spec/`.
- Foundational data types (`Message`, `Role`, `ContentBlock`) and supporting modules (`config`, `history`, `jail`, `llm`, `state`, `tools`).
- A minimal `main.rs` entry point.

### Out of scope

- All runtime features are deferred to later changes (streaming, TUI, extensions, skills, sessions, context, MCP, code indexing, etc.).
- Multi-crate workspace split.
- Linting/tooling configuration beyond what cargo itself provides.

## Approach

Create a single-crate binary with basic types and a placeholder `main`. Use the initial spec document to capture the intended v1.0 architecture, even though the scaffold implements only the smallest usable slice.
