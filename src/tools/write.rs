use serde_json::{json, Value};

use crate::jail::Jail;
use crate::tools::{Tool, ToolError};

pub struct Write {
    jail: Jail,
}

impl Write {
    pub fn new(jail: Jail) -> Self { Self { jail } }
}

#[async_trait::async_trait]
impl Tool for Write {
    fn name(&self) -> &'static str { "write" }

    fn schema(&self) -> Value {
        json!({
            "name": "write",
            "description": "write content to a file, creating parent directories as needed",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }
        })
    }

    async fn run(&self, args: Value) -> Result<String, ToolError> {
        let path = args["path"].as_str().ok_or_else(|| ToolError::InvalidArgs("path required".into()))?;
        let content = args["content"].as_str().ok_or_else(|| ToolError::InvalidArgs("content required".into()))?;
        let target = self.jail.resolve(path).map_err(|e| ToolError::Jail(e.to_string()))?;
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&target, content.as_bytes()).await?;
        Ok(format!("wrote {} bytes to {}", content.len(), target.display()))
    }
}