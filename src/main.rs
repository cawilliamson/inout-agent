pub mod tui;
// twobobs: minimal rust-native ai agent
// v1 scope: conversation loop, 5 tools, single agent, jsonl history, vcr tests.
// not v1: streaming, subagents, hub, mcp, browser, debug, lsp, ast, skills,
// managed skills, todos, learn, slash commands, parallel tool calls.

pub mod config;
pub mod history;
pub mod jail;
pub mod llm;
pub mod state;
pub mod tools;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use config::{BashConfig, Config};
use history::{History, LlmResponse};
use llm::LlmClient;
use state::State;

pub struct Agent {
    pub config: Arc<Config>,
    pub history: History,
    pub state: State,
    pub llm: Box<dyn LlmClient>,
    pub tools: tools::Registry,
}

impl Agent {
    pub fn new(config: Config, llm: Box<dyn LlmClient>) -> Self {
        let repo_root = config.repo_root.clone();
        let max_turns = config.max_turns;
        let tools = tools::Registry::default(repo_root, config.bash.clone());
        Self {
            config: Arc::new(config),
            history: History::new(max_turns),
            state: State::AwaitingUser,
            llm,
            tools,
        }
    }

    pub async fn run_turn(&mut self, user_msg: String) -> Result<String> {
        self.state = State::Thinking;
        self.history.append_user(user_msg);

        loop {
            let req = self.history.to_request(&self.config.model, &self.tools.schemas());
            let resp = self.llm.complete(req).await?;

            if resp.tool_calls.is_empty() {
                self.state = State::Responding;
                self.history.append_assistant(resp.content.clone());
                return Ok(resp.content);
            }

            self.state = State::ToolRunning;
            self.history.append_assistant_with_tools(resp.content.clone(), resp.tool_calls.clone());

            for call in &resp.tool_calls {
                let result = self.tools.dispatch(call).await;
                self.history.append_tool_result(call.id.clone(), result);
            }

            self.state = State::Thinking;
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let model = std::env::var("TWOBOBS_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
    let repo_root = std::env::var("TWOBOBS_REPO_ROOT").unwrap_or_else(|_| ".".to_string());
    let config = Config {
        repo_root: PathBuf::from(repo_root).canonicalize()?,
        llm_provider: "llmgateway".to_string(),
        model,
        max_turns: 20,
        bash: BashConfig::default(),
    };
    let llm: Box<dyn LlmClient> = Box::new(llm::HttpLlmClient::from_env().await?);

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--tui") {
        tui::run(config, llm).await?;
        return Ok(());
    }

    let mut agent = Agent::new(config, llm);
    let prompt = args.get(1).cloned().unwrap_or_else(|| "say hello world".to_string());
    let streaming = std::env::var("TWOBOBS_STREAM").map(|v| v == "1").unwrap_or(false);
    if streaming {
        run_stream(&mut agent, prompt).await?;
    } else {
        let reply = agent.run_turn(prompt).await?;
        println!("{reply}");
    }
    Ok(())
}

async fn run_stream(agent: &mut Agent, prompt: String) -> Result<()> {
    use std::io::Write as _;
    agent.history.append_user(prompt);
    loop {
        let req = agent.history.to_request(&agent.config.model, &agent.tools.schemas());
        let mut rx = agent.llm.complete_stream(req).await?;
        let mut content = String::new();
        let mut tool_calls: Vec<crate::tools::ToolCall> = Vec::new();
        while let Some(evt) = rx.recv().await {
            match evt {
                crate::llm::StreamEvent::Content(delta) => {
                    content.push_str(&delta);
                    print!("{delta}");
                    std::io::stdout().flush()?;
                }
                crate::llm::StreamEvent::ToolCallStart(tc) => {
                    tool_calls.push(tc);
                }
                crate::llm::StreamEvent::ToolCallDelta(_) => {}
                crate::llm::StreamEvent::Cost(c) => {
                    eprintln!("[cost] {} model={}", c.format(), agent.config.model);
                }
                crate::llm::StreamEvent::Done => break,
                crate::llm::StreamEvent::Error(e) => {
                    return Err(anyhow::anyhow!("stream error: {e}"));
                }
            }
        }
        println!();
        if tool_calls.is_empty() {
            agent.history.append_assistant(content);
            return Ok(());
        }
        agent.history.append_assistant_with_tools(content, tool_calls.clone());
        for call in &tool_calls {
            let result = agent.tools.dispatch(call).await;
            eprintln!("[result] {} -> {}", call.name, result.chars().take(100).collect::<String>());
            agent.history.append_tool_result(call.id.clone(), result);
        }
    }
}