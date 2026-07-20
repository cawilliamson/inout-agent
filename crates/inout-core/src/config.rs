//! shared configuration structs.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// global agent configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// root directory the agent treats as the project.
    pub repo_root: PathBuf,
    /// llm provider identifier (e.g. `anthropic`, `llmgateway`).
    pub llm_provider: String,
    /// model identifier passed to the provider.
    pub model: String,
    /// maximum conversation turns before compaction.
    pub max_turns: usize,
    /// bash execution policy.
    pub bash: BashConfig,
    /// whether to emit trace/spans to the observability bus.
    pub observability: bool,
    /// rhai scripting policy.
    pub scripts: ScriptConfig,
    /// additional directories to scan for `.rhai` extensions.
    pub extension_paths: Vec<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            repo_root: PathBuf::from("."),
            llm_provider: String::from("anthropic"),
            model: String::from("claude-sonnet-4-5-20250929"),
            max_turns: 20,
            bash: BashConfig::default(),
            observability: false,
            scripts: ScriptConfig::default(),
            extension_paths: Vec::new(),
        }
    }
}

/// rhai scripting policy.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScriptConfig {
    /// allow scripts to write files.
    pub allow_write: bool,
    /// allow scripts to run shell commands.
    pub allow_shell: bool,
    /// allow scripts to make network requests.
    pub allow_network: bool,
}

/// bash tool policy.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BashConfig {
    /// if true, any binary on `$PATH` is allowed (dangerous).
    pub full: bool,
    /// allowed binaries in safe mode.
    pub allowlist: Vec<String>,
    /// command timeout in seconds.
    pub timeout_secs: u64,
}

impl Default for BashConfig {
    fn default() -> Self {
        Self { full: false, allowlist: Self::safe_defaults(), timeout_secs: 30 }
    }
}

impl BashConfig {
    /// default safe-mode allowlist.
    pub fn safe_defaults() -> Vec<String> {
        ["cargo", "cat", "diff", "echo", "git", "ls", "rg", "wc"]
            .iter()
            .map(ToString::to_string)
            .collect()
    }
}
