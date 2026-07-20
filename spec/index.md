# inout openspec index

project: inout
version: 0.3.0-dev
created: 2026-07-19

## what this is

these openspecs capture the best features mined from two reference agents, reframed around an extension-first architecture:

- **pi** (`earendil-works/pi`) — typescript agent harness. modular, extendable, bloated. we lift the extension surfaces and the observability/hook model, drop the runtime fat.
- **zap** (`zap-coding-agent/zap-coding-agent`) — rust-native terminal agent. lean, single-binary, good security defaults. we lift the rust patterns, the raw-stream logging, the skill injection, the edit ledger; we tighten the linting zap never bothered to configure.

inout's rule: **bare core, everything is an extension, rust-native, strict linting.** the core is a substrate — loop, types, registries, hook bus, extension loader, rhai runtime. every tool, every hook, every piece of persistence, every provider, every observability surface is an extension. extensions are rust crates (first-party, compiled in) or rhai scripts (user-authored, runtime-loaded). both share one `ExtensionApi`.

## architecture at a glance

```
crates/
├── inout-core/               # substrate: loop, types, registries, hook bus, loader, rhai
├── inout-ext-builtin-tools/  # read, write, edit, grep, bash
├── inout-ext-jail/           # path canonicalisation hook
├── inout-ext-permissions/    # ask/auto/deny modes
├── inout-ext-sessions/       # jsonl persistence, branching, compaction
├── inout-ext-context/        # sliding window, drop summarizer, edit ledger
├── inout-ext-skills/         # trigger matching, budget ranking
├── inout-ext-secret-scanner/ # 25+ patterns, redaction
├── inout-ext-observability/  # trace/span bus, Redacted<T>
├── inout-ext-undo/           # snapshot before edits, /undo
├── inout-ext-http-provider/  # anthropic + openai wire format, caching, retry
├── inout-ext-ast/            # tree-sitter + sqlite (feature-gated)
├── inout-ext-mcp/            # json-rpc stdio (feature-gated)
└── inout/                    # binary: wires core + extensions
```

## spec layout

| file | version | what it specifies |
|---|---|---|
| `v1.0.md` | 0.3.0 | core: loop, types, provider trait, replay client, hook bus, tool/command registries, extension trait, rhai loader, minimal config, minimal session |
| `v2.0-extensions.md` | 0.3.0 | hook bus, tool/command traits, extension trait, extension api, rust crate extensions, rhai script extensions, shared event surface |
| `v2.1-observability.md` | 0.3.0 | observability extension: trace/span bus, raw-stream capture, audit, per-turn cost, Redacted<T> |
| `v2.2-skills.md` | 0.3.0 | skills extension: trigger matching, always-on budget, skill trace, stack detection |
| `v2.3-context.md` | 0.3.0 | context extension: system prompt assembly, sliding window, drop summarizer, edit ledger, casual turn, compaction |
| `v2.4-security.md` | 0.3.0 | permissions + secret-scanner + undo extensions: permission modes, 25+ patterns, shell sandbox, undo ledger |
| `v2.5-sessions.md` | 0.3.0 | sessions extension: session entry tree, session repo trait, jsonl repo, branching, compaction, continuity handoff |
| `v2.6-code-index.md` | 0.3.0 | ast extension (feature-gated): sqlite + tree-sitter symbol index, background indexer, quality report |
| `v2.7-mcp.md` | 0.3.0 | mcp extension (feature-gated): lazy-loaded stdio client, mcp tool wrapper, cross-agent config compat |
| `v2.8-linting.md` | 0.3.0 | clippy, rustfmt, deny, cargo-husky, ci — strict rust discipline zap lacks |
| `v2.9-llm-client.md` | 0.3.0 | http-provider extension: provider trait impl, anthropic prompt caching, openai-compat, replay, retry, model routing |
| `v2.10-scripting.md` | 0.3.0 | rhai scripting tier: script discovery, ExtensionApi surface, host functions, type conversions, hot-reload, sandboxing |

## versioning

- v0.3.0 — the extension-first rewrite. core is substrate; everything else is an extension.
- each spec is independently implementable. order is suggested, not required.
- a spec is "done" when its acceptance criteria pass and the lint config in `v2.8-linting.md` is green.

## principles (carry across all specs)

1. **bare core.** the core crate contains only the substrate: loop, types, provider trait, replay client, hook bus, registries, extension loader, rhai runtime, minimal config, minimal session. no tools, no jail, no audit, no persistence, no context management, no skills, no http provider, no observability. all of those are extensions.
2. **everything is an extension.** the five tools, the jail, the permissions, the audit log, the session persistence, the context manager, the skills system, the secret scanner, the observability bus (raw-stream capture included), the undo ledger, the http provider — all extensions. first-party extensions are rust crates compiled in; user extensions are rhai scripts loaded at runtime.
3. **rust-native.** no embedded scripting for first-party code. extensions that need c deps (tree-sitter, sqlite), async runtime, or subprocess management are rust crates. user extensions that need none of those can be rhai scripts. pi's ts extension model does not survive translation — we take the *event surface*, not the runtime.
4. **one extension api.** rust crates and rhai scripts register via the same `ExtensionApi`. the `ToolRegistry`, `CommandRegistry`, and `HookBus` are shared. an extension is an extension regardless of language.
5. **no optional deps in the core crate.** `reqwest`, `rusqlite`, `tree-sitter`, `ratatui`, `crossterm`, `axum` live in extension crates, not in `inout-core`. the core depends only on `serde`, `serde_json`, `tokio`, `async-trait`, `anyhow`, `thiserror`, `rhai`.
6. **observability is a subscriber bus, not a vendor sdk.** pi got this right. we mirror it — as an extension, not core.
7. **everything is auditable.** every tool call, every provider request, every permission decision hits the audit log — via the audit extension's `observe` handler.
8. **lints are load-bearing.** `cargo clippy --all-targets -- -D warnings` is green on every commit, with and without every feature flag. see `v2.8-linting.md`.