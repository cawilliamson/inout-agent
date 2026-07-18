use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::BashConfig;
use crate::jail::Jail;

mod bash;
mod edit;
mod grep;
mod read;
mod write;

pub use bash::Bash;
pub use edit::Edit;
pub use grep::Grep;
pub use read::Read;
pub use write::Write;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn schema(&self) -> Value;
    async fn run(&self, args: Value) -> Result<String, ToolError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    #[error("jail violation: {0}")]
    Jail(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("command failed: {0}")]
    Command(String),
}

pub struct Registry {
    pub jail: Jail,
    pub bash_config: BashConfig,
    pub tools: Vec<Box<dyn Tool>>,
}

impl Registry {
    pub fn default(repo_root: PathBuf, bash_config: BashConfig) -> Self {
        let jail = Jail::new(repo_root);
        Self {
            jail: jail.clone(),
            bash_config: bash_config.clone(),
            tools: vec![
                Box::new(Read::new(jail.clone())),
                Box::new(Write::new(jail.clone())),
                Box::new(Edit::new(jail.clone())),
                Box::new(Grep::new(jail.clone())),
                Box::new(Bash::new(bash_config, Duration::from_secs(30))),
            ],
        }
    }

    pub fn schemas(&self) -> Vec<Value> {
        self.tools.iter().map(|t| t.schema()).collect()
    }

    pub async fn dispatch(&self, call: &ToolCall) -> String {
        let tool = self.tools.iter().find(|t| t.name() == call.name);
        match tool {
            Some(t) => match t.run(call.arguments.clone()).await {
                Ok(s) => s,
                Err(e) => format!("error: {e}"),
            },
            None => format!("error: unknown tool {}", call.name),
        }
    }
}