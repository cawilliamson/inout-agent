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
    let mut agent = Agent::new(
        Config {
            repo_root: PathBuf::from("."),
            llm_provider: "replay".to_string(),
            model: "replay".to_string(),
            max_turns: 20,
            bash: BashConfig::default(),
        },
        Box::new(llm::ReplayLlmClient::new(vec![
            LlmResponse { content: "done".to_string(), tool_calls: vec![] }
        ])),
    );
    let reply = agent.run_turn("hello".to_string()).await?;
    println!("{}", reply);
    Ok(())
}