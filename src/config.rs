use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub repo_root: PathBuf,
    pub llm_provider: String,
    pub model: String,
    pub max_turns: usize,
    pub bash: BashConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BashConfig {
    pub allowlist: Vec<String>,
    pub full: bool,
    pub timeout_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            repo_root: PathBuf::from("."),
            llm_provider: "anthropic".to_string(),
            model: "claude-sonnet-4-5-20250929".to_string(),
            max_turns: 20,
            bash: BashConfig::default(),
        }
    }
}

impl Default for BashConfig {
    fn default() -> Self {
        Self {
            allowlist: vec![
                "cargo".to_string(),
                "cat".to_string(),
                "diff".to_string(),
                "echo".to_string(),
                "git".to_string(),
                "ls".to_string(),
                "rg".to_string(),
                "wc".to_string(),
            ],
            full: false,
            timeout_secs: 30,
        }
    }
}