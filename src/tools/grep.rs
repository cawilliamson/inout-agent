use serde_json::{json, Value};

use crate::jail::Jail;
use crate::tools::{Tool, ToolError};

pub struct Grep {
    jail: Jail,
}

impl Grep {
    pub fn new(jail: Jail) -> Self { Self { jail } }
}

#[async_trait::async_trait]
impl Tool for Grep {
    fn name(&self) -> &'static str { "grep" }

    fn schema(&self) -> Value {
        json!({
            "name": "grep",
            "description": "search for a regex pattern under the repo root using ripgrep",
            "input_schema": {
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "path": { "type": "string" }
                },
                "required": ["pattern"]
            }
        })
    }

    async fn run(&self, args: Value) -> Result<String, ToolError> {
        let pattern = args["pattern"].as_str().ok_or_else(|| ToolError::InvalidArgs("pattern required".into()))?;
        let path_arg = args["path"].as_str().unwrap_or(".");
        let target = self.jail.resolve(path_arg).map_err(|e| ToolError::Jail(e.to_string()))?;
        let output = tokio::process::Command::new("rg")
            .arg("--line-number")
            .arg("--fixed-strings")
            .arg(pattern)
            .arg(&target)
            .output()
            .await?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if !output.status.success() && output.status.code() != Some(1) {
            return Err(ToolError::Command(stderr));
        }
        Ok(stdout)
    }
}