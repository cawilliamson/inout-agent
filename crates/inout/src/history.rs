use serde::{Deserialize, Serialize};

use inout_core::tools::ToolCall;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub reasoning: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct History {
    pub messages: Vec<Message>,
    pub max_turns: usize,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

impl History {
    pub fn new(max_turns: usize) -> Self {
        Self { messages: Vec::new(), max_turns, system_prompt: None }
    }

    pub fn with_system_prompt(max_turns: usize, system_prompt: String) -> Self {
        Self { messages: Vec::new(), max_turns, system_prompt: Some(system_prompt) }
    }

    pub fn set_system_prompt(&mut self, prompt: String) {
        self.system_prompt = Some(prompt);
    }

    pub fn append_user(&mut self, content: String) {
        self.messages.push(Message {
            role: Role::User,
            content,
            tool_calls: Vec::new(),
            tool_call_id: None,
            reasoning: String::new(),
        });
    }

    pub fn append_assistant(&mut self, content: String) {
        self.messages.push(Message {
            role: Role::Assistant,
            content,
            tool_calls: Vec::new(),
            tool_call_id: None,
            reasoning: String::new(),
        });
    }

    pub fn append_assistant_with_reasoning(
        &mut self,
        content: String,
        reasoning: String,
        tool_calls: Vec<ToolCall>,
    ) {
        self.messages.push(Message {
            role: Role::Assistant,
            content,
            tool_calls,
            tool_call_id: None,
            reasoning,
        });
    }

    pub fn append_assistant_with_tools(&mut self, content: String, tool_calls: Vec<ToolCall>) {
        self.messages.push(Message {
            role: Role::Assistant,
            content,
            tool_calls,
            tool_call_id: None,
            reasoning: String::new(),
        });
    }

    pub fn append_tool_result(&mut self, tool_call_id: String, content: String) {
        self.messages.push(Message {
            role: Role::Tool,
            content,
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id),
            reasoning: String::new(),
        });
    }

    pub fn to_request(&self, model: &str, tool_schemas: &[serde_json::Value]) -> LlmRequest {
        let mut messages: Vec<Message> = Vec::new();
        if let Some(prompt) = &self.system_prompt {
            if !prompt.is_empty() {
                messages.push(Message {
                    role: Role::System,
                    content: prompt.clone(),
                    tool_calls: Vec::new(),
                    tool_call_id: None,
                    reasoning: String::new(),
                });
            }
        }
        messages.extend(self.messages.iter().cloned());
        LlmRequest { model: model.to_string(), messages, tools: tool_schemas.to_vec() }
    }

    pub fn to_jsonl(&self) -> anyhow::Result<String> {
        Ok(self
            .messages
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()?
            .join("\n"))
    }

    pub fn from_jsonl(s: &str) -> anyhow::Result<Self> {
        let mut messages = Vec::new();
        for line in s.lines() {
            if line.is_empty() {
                continue;
            }
            messages.push(serde_json::from_str(line)?);
        }
        Ok(Self { messages, max_turns: 20, system_prompt: None })
    }

    /// drop `count` messages starting at `index`. used by the context viewer
    /// to remove a turn from history. preserves the system prompt.
    pub fn drop_range(&mut self, index: usize, count: usize) {
        if index >= self.messages.len() {
            return;
        }
        let end = (index + count).min(self.messages.len());
        self.messages.drain(index..end);
    }

    /// clear all conversation messages, preserving the system prompt.
    pub fn clear_messages(&mut self) {
        self.messages.clear();
    }

    /// drop the last conversation turn: find the last user message, remove
    /// it and everything after it. preserves the system prompt.
    pub fn drop_last_turn(&mut self) {
        // scan backwards for the last user message
        let last_user = self.messages.iter().rposition(|m| m.role == Role::User);
        if let Some(idx) = last_user {
            self.messages.truncate(idx);
        }
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
