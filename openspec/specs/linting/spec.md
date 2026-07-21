# Linting Specification

## Purpose

The strict Rust discipline the previous agent harness lacked. Lints are load-bearing: every commit must stay green. This specification wires clippy, rustfmt, cargo-deny, cargo-husky, and CI together so formatting, style, correctness, supply-chain, and panic-free rules are enforced automatically.

## Requirements

### Requirement: inout-core passes clippy alone

`inout-core` MUST build and pass clippy with no extension crates on its dependency list.

#### Scenario: Lean core clippy run

- GIVEN a workspace containing only `inout-core` and its declared dependencies
- WHEN `cargo clippy --all-targets --no-default-features -- -D warnings` runs in the workspace root
- THEN the command exits with status 0
- AND no lint warnings are emitted

### Requirement: rustfmt checks pass

`cargo fmt --check` MUST exit green for the entire workspace.

#### Scenario: Formatted workspace

- GIVEN the workspace source tree after any edit
- WHEN `cargo fmt --check` runs
- THEN the command exits with status 0
- AND no formatting diff is reported

### Requirement: All-features clippy passes

`cargo clippy --all-targets --all-features -- -D warnings` MUST exit green for the whole workspace.

#### Scenario: Full clippy run

- GIVEN all workspace crates with all features enabled
- WHEN `cargo clippy --all-targets --all-features -- -D warnings` runs
- THEN the command exits with status 0
- AND no lint warnings are emitted

### Requirement: No-default-features clippy passes

`cargo clippy --all-targets --no-default-features -- -D warnings` MUST exit green, ensuring the lean core still compiles without default features.

#### Scenario: Lean clippy run

- GIVEN all workspace crates with default features disabled
- WHEN `cargo clippy --all-targets --no-default-features -- -D warnings` runs
- THEN the command exits with status 0
- AND no lint warnings are emitted

### Requirement: cargo-deny checks pass

`cargo deny check` MUST exit green with no advisories and no license issues.

#### Scenario: Clean supply-chain audit

- GIVEN a populated Cargo.lock and deny configuration
- WHEN `cargo deny check` runs
- THEN the command exits with status 0
- AND no vulnerability, yanked, unlicensed, or disallowed-license findings are reported

### Requirement: All-features tests pass

`cargo test --all-features` MUST exit green for the whole workspace.

#### Scenario: Full test run

- GIVEN all workspace crates with all features enabled
- WHEN `cargo test --all-features` runs
- THEN the command exits with status 0
- AND no test failures are reported

### Requirement: No-default-features tests pass

`cargo test --no-default-features` MUST exit green for the whole workspace.

#### Scenario: Lean test run

- GIVEN all workspace crates with default features disabled
- WHEN `cargo test --no-default-features` runs
- THEN the command exits with status 0
- AND no test failures are reported

### Requirement: Pre-commit hook blocks bad commits

A pre-commit hook MUST run `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`, and MUST block the commit when either command fails.

#### Scenario: Pre-commit catches formatting error

- GIVEN one or more source files that fail `cargo fmt --check`
- WHEN a `git commit` is attempted
- THEN the pre-commit hook runs
- AND the commit is rejected before any objects are created

#### Scenario: Pre-commit catches clippy warning

- GIVEN source code that triggers a clippy lint under `-D warnings`
- WHEN a `git commit` is attempted
- THEN the pre-commit hook runs
- AND the commit is rejected before any objects are created

### Requirement: CI runs all lint and test checks

CI MUST run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo deny check`, `cargo test --all-features`, and `cargo test --no-default-features` on every push and every pull request.

#### Scenario: Push to default branch

- GIVEN a push to the default branch
- WHEN the CI workflow executes
- THEN the lint job and the test job both run
- AND all checks complete before the workflow reports success

#### Scenario: Pull request opened or updated

- GIVEN a pull request against the default branch
- WHEN the CI workflow executes
- THEN the lint job and the test job both run
- AND all checks complete before the workflow reports success

### Requirement: No unwrap, expect, or dbg in production code

Non-test code MUST contain zero `unwrap()`, `expect()`, and `dbg!()` calls. Test code is exempt.

#### Scenario: Production source is panic-free

- GIVEN a scan of all Rust source files outside `#[cfg(test)]` modules and `tests/` directories
- WHEN the scan searches for `unwrap(`, `expect(`, and `dbg!(`
- THEN zero matches are found in production code

### Requirement: clippy.toml enforces strict style limits

A workspace `clippy.toml` MUST exist and set thresholds for type complexity, cognitive complexity, argument count, line count, and other strictness knobs.

#### Scenario: clippy.toml is present and effective

- GIVEN a `clippy.toml` in the workspace root with strict thresholds configured
- WHEN clippy runs on any crate
- THEN the configured thresholds are enforced

### Requirement: rustfmt.toml configures formatting

A workspace `rustfmt.toml` MUST exist and configure edition 2021, max width 100, import grouping, and related formatting rules.

#### Scenario: rustfmt.toml is present and effective

- GIVEN a `rustfmt.toml` in the workspace root with formatting rules configured
- WHEN `cargo fmt --check` runs
- THEN the configured rules are enforced

### Requirement: Workspace lints table denies unsafe and panic patterns

The root `Cargo.toml` MUST declare a `[workspace.lints]` table that denies `unsafe_code`, `unwrap_used`, `expect_used`, and `dbg_macro`, warns on `pedantic` and `nursery`, and inherits into every crate.

#### Scenario: Crate inherits workspace lints

- GIVEN a `[workspace.lints]` table in the root `Cargo.toml`
- AND a crate `Cargo.toml` with `[lints] workspace = true`
- WHEN clippy or rustc compiles the crate
- THEN the workspace lint levels are applied

### Requirement: deny.toml blocks advisories, bad licenses, and wildcard sources

A workspace `deny.toml` MUST exist and deny vulnerabilities, yanked crates, unlicensed crates, copyleft licenses, wildcard dependencies, and unknown registries or git sources.

#### Scenario: deny.toml is present and enforced

- GIVEN a `deny.toml` in the workspace root
- WHEN `cargo deny check` runs
- THEN the configured advisory, license, ban, and source rules are enforced

### Requirement: .cargo/config.toml sets default flags and aliases

The workspace MUST contain `.cargo/config.toml` that sets default `rustflags = ["-D", "warnings"]` and provides aliases for lint and audit commands.

#### Scenario: Default rustflags deny warnings

- GIVEN `.cargo/config.toml` with `build.rustflags = ["-D", "warnings"]`
- WHEN any cargo build, check, or test command runs without overriding rustflags
- THEN compiler warnings are treated as errors

### Requirement: Test code allows unwrap and dbg

Test modules annotated with `#[cfg(test)]` SHALL permit `unwrap()`, `expect()`, and `dbg!()` without triggering lint failures.

#### Scenario: Test unwrap is allowed

- GIVEN a test module annotated with `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::dbg_macro)]`
- WHEN `cargo test` runs
- THEN unwrap, expect, and dbg calls inside the test module do not cause clippy errors
