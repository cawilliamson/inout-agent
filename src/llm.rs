use async_trait::async_trait;
use serde_json::Value;

use crate::history::{LlmRequest, LlmResponse};

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse>;
}

pub struct HttpLlmClient {
    api_key: String,
    provider: String,
    model: String,
    client: reqwest::Client,
}

impl HttpLlmClient {
    pub fn new(api_key: String, provider: String, model: String) -> Self {
        Self { api_key, provider, model, client: reqwest::Client::new() }
    }
}

#[async_trait]
impl LlmClient for HttpLlmClient {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let _ = req;
        let _ = (&self.api_key, &self.provider, &self.model);
        // provider-specific request construction and response parsing go here.
        // left as a real stub because v1 is built against replay llm first.
        todo!("wire anthropic/openai http client")
    }
}

#[derive(Clone, Debug, Default)]
pub struct ReplayLlmClient {
    pub responses: Vec<LlmResponse>,
    pub index: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl ReplayLlmClient {
    pub fn new(responses: Vec<LlmResponse>) -> Self {
        Self { responses, index: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)) }
    }
}

#[async_trait]
impl LlmClient for ReplayLlmClient {
    async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let idx = self.index.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.responses.get(idx).cloned()
            .ok_or_else(|| anyhow::anyhow!("replay exhausted at index {idx}"))
    }
}

pub fn schema_for_tool(name: &str, description: &str, args: Value) -> Value {
    let mut schema = serde_json::Map::new();
    schema.insert("type".into(), "function".into());
    let mut function = serde_json::Map::new();
    function.insert("name".into(), name.into());
    function.insert("description".into(), description.into());
    function.insert("parameters".into(), args);
    schema.insert("function".into(), Value::Object(function));
    Value::Object(schema)
}

#[derive(Clone, Debug, Default)]
pub struct ToolUseParser;

impl ToolUseParser {
    pub fn parse(content: &str, tool_schemas: &[Value]) -> Vec<Value> {
        let _ = content;
        let _ = tool_schemas;
        // placeholder until concrete format (anthropic xml, openai json) chosen.
        vec![]
    }
}