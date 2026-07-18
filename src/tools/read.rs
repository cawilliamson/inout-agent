use serde_json::{json, Value};

use crate::jail::Jail;
use crate::tools::{Tool, ToolError};

pub struct Read {
    jail: Jail,
}

impl Read {
    pub fn new(jail: Jail) -> Self { Self { jail } }
}

#[async_trait::async_trait]
impl Tool for Read {
    fn name(&self) -> &'static str { "read" }

    fn schema(&self) -> Value {
        json!({
            "name": "read",
            "description": "read file contents, optionally limited by line range",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "offset": { "type": "integer" },
                    "limit": { "type": "integer" }
                },
                "required": ["path"]
            }
        })
    }

    async fn run(&self, args: Value) -> Result<String, ToolError> {
        let path = args["path"].as_str().ok_or_else(|| ToolError::InvalidArgs("path required".into()))?;
        let target = self.jail.resolve(path).map_err(|e| ToolError::Jail(e.to_string()))?;
        let text = tokio::fs::read_to_string(target).await?;
        let offset = args["offset"].as_u64().unwrap_or(1) as usize;
        let limit = args["limit"].as_u64().unwrap_or(0) as usize;
        if offset <= 1 && limit == 0 {
            return Ok(text);
        }
        let lines: Vec<&str> = text.lines().collect();
        let start = offset.saturating_sub(1);
        let end = if limit == 0 { lines.len() } else { (start + limit).min(lines.len()) };
        Ok(lines[start..end].join("\n"))
    }
}