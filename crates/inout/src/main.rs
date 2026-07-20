//! inout: minimal rust-native ai agent — io on the command line (binary entry point).

#![allow(missing_docs)]
#![allow(missing_debug_implementations)]
#![allow(clippy::print_stdout)]
#![allow(clippy::print_stderr)]

use std::path::PathBuf;

use anyhow::Result;
use inout::llm::LlmClient;
use inout::{llm, tui, Agent};
use inout_core::config::Config;

#[tokio::main]
async fn main() -> Result<()> {
    let model = std::env::var("IO_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
    let repo_root = std::env::var("IO_REPO_ROOT").unwrap_or_else(|_| ".".to_string());
    let config = Config {
        repo_root: PathBuf::from(repo_root).canonicalize()?,
        llm_provider: String::from("llmgateway"),
        model,
        ..Config::default()
    };
    let llm: Box<dyn LlmClient> = Box::new(llm::HttpLlmClient::from_env().await?);

    let args: Vec<String> = std::env::args().collect();
    // collect non-flag prompt args
    let prompt_args: Vec<String> =
        args.iter().skip(1).filter(|a| !a.starts_with("--")).cloned().collect();

    // default: launch TUI when no prompt given
    if prompt_args.is_empty() {
        tui::run(config, llm).await?;
        return Ok(());
    }

    let mut agent = Agent::new(config, llm);
    agent.load_extensions();
    let prompt = prompt_args.join(" ");
    let streaming = std::env::var("IO_STREAM").map(|v| v == "1").unwrap_or(true);
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
        let mut tool_calls: Vec<inout_core::tools::ToolCall> = Vec::new();
        while let Some(evt) = rx.recv().await {
            match evt {
                llm::StreamEvent::Content(delta) => {
                    content.push_str(&delta);
                    print!("{delta}");
                    std::io::stdout().flush()?;
                }
                llm::StreamEvent::Reasoning(delta) => {
                    eprint!("\x1b[2m{delta}\x1b[0m");
                    std::io::stderr().flush()?;
                }
                llm::StreamEvent::ToolCallStart(tc) => {
                    tool_calls.push(tc);
                }
                llm::StreamEvent::ToolCallDelta(_) => {}
                llm::StreamEvent::Cost(c) => {
                    eprintln!("[cost] {} model={}", c.format(), agent.config.model);
                }
                llm::StreamEvent::Done => break,
                llm::StreamEvent::Error(e) => {
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
            let result =
                agent.tools.dispatch_call(call).await.unwrap_or_else(|e| format!("error: {e}"));
            eprintln!("[result] {} -> {}", call.name, result.chars().take(100).collect::<String>());
            agent.history.append_tool_result(call.id.clone(), result);
        }
    }
}
