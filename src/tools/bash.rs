use std::time::Duration;

use serde_json::{json, Value};

use crate::config::BashConfig;
use crate::tools::{Tool, ToolError};

pub struct Bash {
    config: BashConfig,
    timeout: Duration,
}

impl Bash {
    pub fn new(config: BashConfig, timeout: Duration) -> Self { Self { config, timeout } }
}

#[async_trait::async_trait]
impl Tool for Bash {
    fn name(&self) -> &'static str { "bash" }

    fn schema(&self) -> Value {
        json!({
            "name": "bash",
            "description": "run an allowed shell command in the repo root",
            "input_schema": {
                "type": "object",
                "properties": {
                    "command": { "type": "string" }
                },
                "required": ["command"]
            }
        })
    }

    async fn run(&self, args: Value) -> Result<String, ToolError> {
        let command = args["command"].as_str().ok_or_else(|| ToolError::InvalidArgs("command required".into()))?;
        let words = shell_words::split(command).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        if words.is_empty() {
            return Err(ToolError::InvalidArgs("empty command".into()));
        }
        let binary = &words[0];

        let blocklist = [
            "rm", "dd", "mkfs", "parted", "fastboot", "shutdown", "reboot",
            "sudo", "curl", "wget", "chmod", "chown",
        ];
        if blocklist.contains(&binary.as_str()) {
            return Err(ToolError::InvalidArgs(format!("blocked binary: {binary}")));
        }
        if command.contains("rm -rf") {
            return Err(ToolError::InvalidArgs("rm -rf blocked".into()));
        }
        if command.contains('`') || command.contains("$(") {
            return Err(ToolError::InvalidArgs("command substitution blocked".into()));
        }

        if !self.config.full {
            if !self.config.allowlist.iter().any(|allowed| allowed == binary) {
                return Err(ToolError::InvalidArgs(format!("{binary} not in allowlist")));
            }
            if words.iter().any(|w| w.contains('>')) {
                return Err(ToolError::InvalidArgs("shell redirection blocked in safe mode".into()));
            }
        }

        let output = tokio::time::timeout(self.timeout, tokio::process::Command::new(binary)
            .args(&words[1..])
            .kill_on_drop(true)
            .output()).await
            .map_err(|_| ToolError::Command("timeout".into()))?
            .map_err(|e| ToolError::Io(e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if !output.status.success() {
            return Err(ToolError::Command(format!("exit {:?}: {stderr}", output.status.code())));
        }
        Ok(stdout)
    }
}