# AST Code Index Specification

## Purpose
An optional sqlite-backed AST symbol index, built via tree-sitter. It is gated behind an `ast` feature flag that is default-off, so the core loop builds and runs with the index disabled.

## Requirements

### Requirement: Feature flag isolation
The core crate SHALL build and run without the AST extension crate or feature enabled. The `ast` feature SHALL be default-off. The v1.0 agent loop SHALL remain intact when the feature is disabled.

#### Scenario: Build without AST feature
GIVEN the core crate source tree
WHEN `cargo check -p inout-core` runs without the `ast` feature
THEN compilation succeeds with no warnings treated as errors

#### Scenario: v1.0 loop runs without index
GIVEN a build of the binary without the `ast` feature
WHEN the agent processes a single turn with a tool call
THEN the turn completes without requiring a code index

### Requirement: Extension crate exists
The AST index SHALL live in a first-party extension crate. The binary crate MAY feature-gate inclusion of this extension under the `ast` flag.

#### Scenario: AST extension crate compiles
GIVEN the extension crate source tree
WHEN `cargo check -p inout-ext-ast` runs
THEN compilation succeeds with no warnings treated as errors

### Requirement: Code index storage
The system SHALL persist the code index in an sqlite database at `.inout/code.db` inside the current working directory. The system SHALL also support opening an in-memory code index for tests.

#### Scenario: Open in-memory index
GIVEN `CodeIndex::open_in_memory()`
WHEN called from a test
THEN it returns a usable, empty index with a live sqlite connection

#### Scenario: Open on-disk index
GIVEN a project root directory
WHEN the code index is opened
THEN an sqlite database exists at `.inout/code.db`

### Requirement: Symbol data model
The index SHALL store `Symbol` records with fields: `path` (file path), `line` (1-based line number), `kind` (e.g. function, struct, trait, class, method, enum), `name` (symbol name), and an optional `signature`.

#### Scenario: Index a function symbol
GIVEN a Rust source file containing `pub fn foo() -> u32`
WHEN the file is indexed
THEN a `Symbol` with kind `function`, name `foo`, and a populated signature is stored

### Requirement: Callsite data model
The index SHALL store `CallSite` records with fields: `call_path` (file path), `line`, `callee_name`, and an optional `qualifier`.

#### Scenario: Index a function call
GIVEN a source file containing `repo::bar()`
WHEN the file is indexed
THEN a `CallSite` with callee_name `bar` and qualifier `repo` is stored

### Requirement: Import data model
The index SHALL store `Import` records with fields: `path` (file path), `imported_name`, `kind`, and `line`.

#### Scenario: Index a use statement
GIVEN a Rust source file containing `use std::collections::HashMap`
WHEN the file is indexed
THEN an `Import` with imported_name `HashMap` is stored for that path

### Requirement: Type edge data model
The index SHALL store `TypeEdge` records with fields: `child_name`, `parent_name`, and `kind` (`implements` or `extends`).

#### Scenario: Index an implementation
GIVEN a Rust source file containing `impl MyTrait for MyStruct`
WHEN the file is indexed
THEN a `TypeEdge` with child_name `MyStruct`, parent_name `MyTrait`, and kind `implements` is stored

#### Scenario: Index class inheritance
GIVEN a Java source file containing `class B extends A`
WHEN the file is indexed
THEN a `TypeEdge` with child_name `B`, parent_name `A`, and kind `extends` is stored

### Requirement: Language coverage
The system SHALL index source files for the following languages using tree-sitter parsers: Rust, Python, TypeScript, JavaScript, Go, and Java.

#### Scenario: Rust source is indexed
GIVEN a `.rs` file with functions, structs, and trait implementations
WHEN the file is indexed
THEN symbols, callsites, imports, and type edges are extracted

#### Scenario: Python source is indexed
GIVEN a `.py` file with function and class definitions
WHEN the file is indexed
THEN symbols, callsites, and imports are extracted

#### Scenario: TypeScript source is indexed
GIVEN a `.ts` file with functions, classes, and imports
WHEN the file is indexed
THEN symbols, callsites, imports, and type edges are extracted

#### Scenario: JavaScript source is indexed
GIVEN a `.js` file with functions and imports
WHEN the file is indexed
THEN symbols, callsites, and imports are extracted

#### Scenario: Go source is indexed
GIVEN a `.go` file with functions, methods, and imports
WHEN the file is indexed
THEN symbols, callsites, and imports are extracted

#### Scenario: Java source is indexed
GIVEN a `.java` file with classes, methods, and inheritance
WHEN the file is indexed
THEN symbols, callsites, imports, and type edges are extracted

### Requirement: Definition lookup
`global_find_definition` SHALL return all `Symbol` records matching a given name. If no symbols match, the system SHALL fall back to a grep search over the working tree.

#### Scenario: Indexed definition found
GIVEN an indexed symbol named `UserRepository`
WHEN `global_find_definition("UserRepository")` is called
THEN the matching symbol is returned

#### Scenario: Miss falls back to grep
GIVEN a symbol named `legacy_fn` that is not in the index
WHEN `global_find_definition("legacy_fn")` is called
THEN the system searches the working tree with grep
AND returns the grep results

### Requirement: Incremental reindex on file write
The system SHALL reindex a file after every successful file write observed by the index integration point. Reindexing SHALL be incremental: it replaces the prior data for that file rather than rebuilding the whole index.

#### Scenario: Reindex after write
GIVEN a file `src/lib.rs` already in the index
WHEN `src/lib.rs` is written
THEN the index updates the symbols, callsites, imports, and type edges for `src/lib.rs` without reindexing other files

### Requirement: Background indexer
The system SHALL run a background indexer that scans the working tree and reindexes changed files. The default interval SHALL be 120 seconds. The interval SHALL be configurable through an environment variable.

#### Scenario: Default background interval
GIVEN no indexer interval environment variable is set
WHEN the background indexer starts
THEN it runs the first scan after 120 seconds

#### Scenario: Custom background interval
GIVEN `INOUT_INDEX_INTERVAL_SECONDS=30` is set
WHEN the background indexer starts
THEN it runs the first scan after 30 seconds

#### Scenario: Background indexer catches stale file
GIVEN a file changed outside the agent (e.g. by `git checkout`)
WHEN the background indexer next scans
THEN it reindexes the changed file

### Requirement: Context packing
`pack_context` SHALL return a list of files selected from the index to satisfy a task description, with a total estimated token cost within the provided token budget.

#### Scenario: Pack within budget
GIVEN a task description and a token budget of 4000 tokens
WHEN `pack_context` is called
THEN it returns files whose estimated combined token cost is at most 4000 tokens

#### Scenario: Empty task returns empty pack
GIVEN an empty task description
WHEN `pack_context` is called
THEN it returns an empty file list

### Requirement: Quality report
`QualityReport` SHALL compute and expose the following metrics: god objects (symbols with more than 15 methods), high coupling (symbols with the highest reference counts), dead code candidates (symbols not referenced by any callsite or import), and an overall quality score from 0 to 100.

#### Scenario: Detect a god object
GIVEN a class with 16 methods in the index
WHEN a quality report is generated
THEN the class appears in `god_objects` with its method count

#### Scenario: Detect high coupling
GIVEN a symbol referenced by 50 callsites
WHEN a quality report is generated
THEN the symbol appears in `high_coupling` with its reference count

#### Scenario: Detect dead code
GIVEN a private function that is never called or imported
WHEN a quality report is generated
THEN the function appears in `dead_code`

#### Scenario: Score range
GIVEN any indexed project
WHEN a quality report is generated
THEN the `score` field is between 0 and 100 inclusive

### Requirement: Index usage logging
Every index-powered tool call SHALL log whether it was a hit or a miss, the tool name, the query, and, on a miss, whether a grep fallback was used.

#### Scenario: Logged hit
GIVEN `global_find_definition("UserRepository")` returns indexed results
WHEN the call completes
THEN a log line records the hit, the tool name `find_definition`, the query `UserRepository`, and the result count

#### Scenario: Logged miss with fallback
GIVEN `global_find_definition("legacy_fn")` finds no indexed results
WHEN the call falls back to grep
THEN a log line records the miss, the tool name `find_definition`, the query `legacy_fn`, and that a grep fallback occurred

### Requirement: Powered tools
The system SHALL provide the following tools, powered by the code index when available and falling back to grep where applicable: `code_map`, `find_definition`, `find_references`, `who_calls`, `file_imports`, `where_imported`, `find_subtypes`, `find_supertypes`, `pack_context`, `ripple_analysis`, `get_diagnostics`, `lsp_definition`, and `lsp_type_at`.

#### Scenario: find_definition powered tool
GIVEN a query symbol name
WHEN the `find_definition` tool is dispatched
THEN it returns the symbol definitions from the index or a grep fallback

#### Scenario: find_references powered tool
GIVEN a query symbol name
WHEN the `find_references` tool is dispatched
THEN it returns callsites referencing the symbol from the index

#### Scenario: pack_context powered tool
GIVEN a task description
WHEN the `pack_context` tool is dispatched
THEN it returns a file list within the requested token budget

### Requirement: Slash commands
The system SHALL expose `/index` subcommands: `reindex`, `stats`, and `quality`.

#### Scenario: Reindex command
GIVEN the `/index reindex` command
WHEN it is run
THEN the system rebuilds the index for the working tree

#### Scenario: Stats command
GIVEN the `/index stats` command
WHEN it is run
THEN it returns counts of symbols, callsites, imports, and type edges in the index

#### Scenario: Quality command
GIVEN the `/index quality` command
WHEN it is run
THEN it renders the quality report in the user interface

### Requirement: Standalone indexing
The binary SHALL support a `--index-only` mode that builds the index with no session and no LLM calls.

#### Scenario: Run standalone index
GIVEN a project directory
WHEN `inout --index-only` is run
THEN the code index is built
AND the process exits without opening a session or calling an LLM

### Requirement: Clippy green
The code SHALL pass `cargo clippy --all-targets -- -D warnings` both with and without the `ast` feature enabled.

#### Scenario: Clippy with ast feature
GIVEN the workspace with the `ast` feature enabled
WHEN `cargo clippy --all-targets --features ast -- -D warnings` runs
THEN it exits with no warnings or errors

#### Scenario: Clippy without ast feature
GIVEN the workspace with the `ast` feature disabled
WHEN `cargo clippy --all-targets -- -D warnings` runs
THEN it exits with no warnings or errors
