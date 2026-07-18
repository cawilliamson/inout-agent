use serde::{Deserialize, Serialize};

use crate::tools::ToolCall;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tool_call_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct History {
    pub messages: Vec<Message>,
    pub max_turns: usize,
}

impl History {
    pub fn new(max_turns: usize) -> Self {
        Self { messages: Vec::new(), max_turns }
    }

    pub fn append_user(&mut self, content: String) {
        self.messages.push(Message { role: Role::User, content, tool_calls: Vec::new(), tool_call_id: None });
    }

    pub fn append_assistant(&mut self, content: String) {
        self.messages.push(Message { role: Role::Assistant, content, tool_calls: Vec::new(), tool_call_id: None });
    }

    pub fn append_assistant_with_tools(&mut self, content: String, tool_calls: Vec<ToolCall>) {
        self.messages.push(Message { role: Role::Assistant, content, tool_calls, tool_call_id: None });
    }

    pub fn append_tool_result(&mut self, tool_call_id: String, content: String) {
        self.messages.push(Message { role: Role::Tool, content, tool_calls: Vec::new(), tool_call_id: Some(tool_call_id) });
    }

    pub fn to_request(&self, model: &str, tool_schemas: &[serde_json::Value]) -> LlmRequest {
        LlmRequest {
            model: model.to_string(),
            messages: self.messages.clone(),
            tools: tool_schemas.to_vec(),
        }
    }

    pub fn to_jsonl(&self) -> anyhow::Result<String> {
        Ok(self.messages.iter().map(|m| serde_json::to_string(m)).collect::<Result<Vec<_>, _>>()?.join("\n"))
    }

    pub fn from_jsonl(s: &str) -> anyhow::Result<Self> {
        let mut messages = Vec::new();
        for line in s.lines() {
            if line.is_empty() { continue; }
            messages.push(serde_json::from_str(line)?);
        }
        Ok(Self { messages, max_turns: 20 })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
}