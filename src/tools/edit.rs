use serde_json::{json, Value};

use crate::jail::Jail;
use crate::tools::{Tool, ToolError};

pub struct Edit {
    jail: Jail,
}

impl Edit {
    pub fn new(jail: Jail) -> Self { Self { jail } }
}

#[async_trait::async_trait]
impl Tool for Edit {
    fn name(&self) -> &'static str { "edit" }

    fn schema(&self) -> Value {
        json!({
            "name": "edit",
            "description": "replace all occurrences of old_text with new_text in a file",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "old_text": { "type": "string" },
                    "new_text": { "type": "string" }
                },
                "required": ["path", "old_text", "new_text"]
            }
        })
    }

    async fn run(&self, args: Value) -> Result<String, ToolError> {
        let path = args["path"].as_str().ok_or_else(|| ToolError::InvalidArgs("path required".into()))?;
        let old_text = args["old_text"].as_str().ok_or_else(|| ToolError::InvalidArgs("old_text required".into()))?;
        let new_text = args["new_text"].as_str().ok_or_else(|| ToolError::InvalidArgs("new_text required".into()))?;
        let target = self.jail.resolve(path).map_err(|e| ToolError::Jail(e.to_string()))?;
        let content = tokio::fs::read_to_string(&target).await?;
        if !content.contains(old_text) {
            return Err(ToolError::InvalidArgs("old_text not found".into()));
        }
        let updated = content.replace(old_text, new_text);
        tokio::fs::write(&target, updated.as_bytes()).await?;
        Ok(format!("updated {}", target.display()))
    }
}