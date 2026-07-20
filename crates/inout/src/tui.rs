use std::io::stdout;
use std::sync::Arc;

use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use ratatui::Terminal;

use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, Gauge, List, ListItem, ListState, Padding, Paragraph, Scrollbar,
    ScrollbarOrientation, ScrollbarState, Wrap,
};
use ratatui::Frame;
use tokio::sync::{mpsc, Mutex};
use tokio::time::Duration;

use crate::history::Role;
use crate::llm::{LlmClient, StreamEvent};
use crate::Agent;
use inout_core::config::Config;
use inout_core::tools::ToolCall;
use inout_core::{CommandAction, CommandContext, ViewBlock, ViewSpec};

// events from the background streaming task to the UI loop
enum UiEvent {
    Delta(String),
    Reasoning(String),
    RoundDone,
    ToolResult { name: String, preview: String },
    Status(String),
    Error(String),
    TurnDone,
    ExtensionLoaded(String),
}

#[derive(Clone)]
struct TuiMessage {
    role: Role,
    content: String,
}

struct ContextViewerState {
    view_name: String,
    spec: ViewSpec,
    selected: usize,
    detail_scroll: u16,
    detail_focus: bool,
    confirm_drop: bool,
    confirm_clear: bool,
}

pub struct TuiAgent {
    agent: Arc<Mutex<Agent>>,
    messages: Vec<TuiMessage>,
    streaming: String,
    reasoning: String,
    input: String,
    status: String,
    busy: bool,
    chat_scroll: u16,
    follow_bottom: bool,
    ui_rx: Option<mpsc::Receiver<UiEvent>>,
    model_name: String,
    context_viewer: Option<ContextViewerState>,
    reasoning_visible: bool,
    cursor_visible: bool,
    slash_selection: usize,
    // cached values updated from async context — draw reads these instead of locking
    cached_cwd: String,
    cached_tool_count: usize,
    cached_ext_count: usize,
    cached_command_names: Vec<String>,
    cached_total_tokens: usize,
    cached_limit_tokens: usize,
    cached_context_pct: usize,
    cached_cost: String,
    cached_last_tokens: usize,
}

impl TuiAgent {
    pub fn new(agent: Agent) -> Self {
        let model_name = agent.config.model.clone();
        let cwd = agent.config.repo_root.display().to_string();
        Self {
            agent: Arc::new(Mutex::new(agent)),
            messages: Vec::new(),
            streaming: String::new(),
            reasoning: String::new(),
            input: String::new(),
            status: "ready".to_string(),
            busy: false,
            chat_scroll: 0,
            follow_bottom: true,
            ui_rx: None,
            model_name,
            context_viewer: None,
            reasoning_visible: true,
            cursor_visible: true,
            slash_selection: 0,
            cached_cwd: cwd,
            cached_tool_count: 0,
            cached_ext_count: 0,
            cached_command_names: Vec::new(),
            cached_total_tokens: 0,
            cached_limit_tokens: 128_000,
            cached_context_pct: 0,
            cached_cost: "0.000000".to_string(),
            cached_last_tokens: 0,
        }
    }

    /// refresh cached values from the agent. must be called from an async
    /// context (uses `.lock().await`). draw methods read only cached fields.
    async fn refresh_cache(&mut self) {
        let agent = self.agent.lock().await;
        self.cached_cwd = agent.config.repo_root.display().to_string();
        self.cached_tool_count = agent.tools.schemas().len();
        self.cached_ext_count =
            if agent.extensions_loaded { agent.commands.names().len() } else { 0 };
        self.cached_command_names = agent.commands.names();
        self.model_name = agent.config.model.clone();

        // token estimate from history message char counts
        let total_chars: usize =
            agent.history.messages.iter().map(|m| m.content.chars().count()).sum();
        self.cached_total_tokens = total_chars / 4;
        self.cached_limit_tokens = 128_000;
        let pct = if self.cached_limit_tokens > 0 {
            ((self.cached_total_tokens as f64 / self.cached_limit_tokens as f64) * 100.0) as usize
        } else {
            0
        };
        self.cached_context_pct = pct;
    }

    async fn open_view(&mut self, name: &str) {
        let spec = {
            let agent = self.agent.lock().await;
            agent.build_view(name)
        };
        match spec {
            Some(spec) => {
                let selected = spec.turns.len().saturating_sub(1);
                self.context_viewer = Some(ContextViewerState {
                    view_name: name.to_string(),
                    spec,
                    selected,
                    detail_scroll: 0,
                    detail_focus: false,
                    confirm_drop: false,
                    confirm_clear: false,
                });
            }
            None => {
                self.messages.push(TuiMessage {
                    role: Role::Assistant,
                    content: format!("no '{name}' view registered (load extensions first)"),
                });
            }
        }
    }

    async fn rebuild_context_viewer(&mut self) {
        let name = self.context_viewer.as_ref().map(|v| v.view_name.clone()).unwrap_or_default();
        let spec = {
            let agent = self.agent.lock().await;
            agent.build_view(&name)
        };
        match spec {
            Some(spec) if !spec.turns.is_empty() => {
                if let Some(viewer) = self.context_viewer.as_mut() {
                    viewer.selected = viewer.selected.min(spec.turns.len().saturating_sub(1));
                    viewer.spec = spec;
                    viewer.detail_scroll = 0;
                    viewer.confirm_drop = false;
                    viewer.confirm_clear = false;
                }
            }
            _ => {
                self.context_viewer = None;
            }
        }
    }

    async fn rebuild_messages_from_history(&mut self) {
        let messages: Vec<TuiMessage> = {
            let agent = self.agent.lock().await;
            agent
                .history
                .messages
                .iter()
                .map(|m| TuiMessage { role: m.role, content: m.content.clone() })
                .collect()
        };
        self.messages = messages;
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        let mut terminal = setup_terminal()?;
        let mut events = EventStream::new();
        self.status = format!(
            "model={} cwd={} — type a message, enter to send, ctrl-c to quit",
            {
                let a = self.agent.lock().await;
                a.config.model.clone()
            },
            {
                let a = self.agent.lock().await;
                a.config.repo_root.display().to_string()
            }
        );
        // tick for spinner animation and cursor blink
        // lazy-load extensions in the background once the ui is up
        self.start_extension_load().await;

        // tick for spinner animation and cursor blink
        let mut ticker = tokio::time::interval(Duration::from_millis(120));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            terminal.draw(|f| self.draw(f))?;
            tokio::select! {
                maybe_evt = events.next() => {
                    let Some(Ok(evt)) = maybe_evt else { break; };
                    if !self.handle_event(evt).await? { break; }
                }
                Some(ui_evt) = async {
                    match &mut self.ui_rx {
                        Some(rx) => rx.recv().await,
                        None => None,
                    }
                } => {
                    if !self.handle_ui_event(ui_evt).await? { break; }
                }
                _ = ticker.tick() => {
                    self.cursor_visible = !self.cursor_visible;
                }
            }
        }
        restore_terminal()?;
        Ok(())
    }

    fn draw(&self, f: &mut Frame<'_>) {
        if let Some(viewer) = &self.context_viewer {
            self.draw_context_viewer(f, viewer);
            return;
        }

        let area = f.area();
        let chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);

        self.draw_header_bar(f, chunks[0]);
        self.draw_chat_area(f, chunks[1]);
        self.draw_context_meter(f, chunks[2]);
        self.draw_input_area(f, chunks[3]);
        self.draw_footer_bar(f, chunks[4]);
    }

    fn draw_header_bar(&self, f: &mut Frame<'_>, area: Rect) {
        let sep = Span::styled(" │ ", Style::default().fg(Color::DarkGray));
        let header = Line::from(vec![
            Span::styled("◆ ", Style::default().fg(Color::Cyan)),
            Span::styled(self.model_name.clone(), Style::default().fg(Color::Cyan)),
            sep.clone(),
            Span::styled(self.cached_cwd.clone(), Style::default().fg(Color::White)),
            sep.clone(),
            Span::styled(
                format!("{} tools", self.cached_tool_count),
                Style::default().fg(Color::White),
            ),
            sep.clone(),
            Span::styled(
                format!("{} extensions", self.cached_ext_count),
                Style::default().fg(Color::White),
            ),
            sep,
            Span::styled(
                format!("{}%", self.cached_context_pct),
                Style::default().fg(Color::White),
            ),
        ]);
        let bar = Paragraph::new(Text::from(vec![header]))
            .style(Style::default().bg(Color::Black))
            .alignment(Alignment::Left);
        f.render_widget(bar, area);
    }

    fn draw_chat_area(&self, f: &mut Frame<'_>, area: Rect) {
        let mut lines: Vec<Line<'_>> = Vec::new();
        let width = area.width as usize;

        for m in &self.messages {
            render_message(&mut lines, m, &self.model_name, width);
        }

        // reasoning line above the streaming answer while busy
        if self.busy && !self.reasoning.is_empty() && self.reasoning_visible {
            let spinner = thinking_spinner();
            let reason = format!("{spinner} reasoning… {}", self.reasoning);
            lines.push(Line::from(""));
            lines.push(Line::styled(
                reason,
                Style::default()
                    .fg(Color::DarkGray)
                    .bg(Color::Black)
                    .add_modifier(Modifier::ITALIC),
            ));
        }

        // live streaming text
        if self.busy && !self.streaming.is_empty() {
            let mut text = self.streaming.clone();
            if self.cursor_visible {
                text.push('▋');
            }
            render_message(
                &mut lines,
                &TuiMessage { role: Role::Assistant, content: text },
                &self.model_name,
                width,
            );
        }

        // thinking indicator before any content arrives
        if self.busy && self.streaming.is_empty() && self.reasoning.is_empty() {
            let spinner = thinking_spinner();
            lines.push(Line::from(""));
            lines.push(Line::styled(
                format!("  {spinner} thinking…"),
                Style::default().fg(Color::DarkGray).bg(Color::Black),
            ));
        }

        let total = lines.len();
        let height = area.height as usize;
        let max_scroll = total.saturating_sub(height).min(u16::MAX as usize) as u16;
        let scroll = if self.follow_bottom { max_scroll } else { self.chat_scroll.min(max_scroll) };

        let chat = Paragraph::new(lines)
            .style(Style::default().fg(Color::White).bg(Color::Black))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0));
        f.render_widget(chat, area);

        // vertical scrollbar when content overflows
        if total > height {
            let mut state = ScrollbarState::new(total).position(scroll as usize);
            let scrollbar = Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None);
            f.render_stateful_widget(
                scrollbar,
                area.inner(Margin { vertical: 0, horizontal: 0 }),
                &mut state,
            );
        }
    }

    fn draw_context_meter(&self, f: &mut Frame<'_>, area: Rect) {
        let (total, limit, pct) = self.context_tokens();
        let ratio = if limit == 0 { 0.0 } else { (total as f64 / limit as f64).min(1.0) };
        let colour = if pct < 50 {
            Color::Green
        } else if pct < 80 {
            Color::Yellow
        } else {
            Color::Red
        };
        let gauge = Gauge::default()
            .block(Block::default().padding(Padding::horizontal(1)))
            .gauge_style(Style::default().fg(colour).bg(Color::Black))
            .ratio(ratio)
            .label(format!("{total} / {limit} tokens ({pct}%)"));
        f.render_widget(gauge, area);
    }

    fn draw_input_area(&self, f: &mut Frame<'_>, area: Rect) {
        let input_text = format!("> {}", self.input);
        let mut text = input_text.clone();
        if self.cursor_visible {
            text.push('▋');
        }

        let chunks = Layout::vertical([Constraint::Length(1), Constraint::Length(2)]).split(area);
        let input = Paragraph::new(Text::from(vec![Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Cyan)),
            Span::styled(self.input.clone(), Style::default().fg(Color::White)),
            if self.cursor_visible {
                Span::styled("▋", Style::default().fg(Color::Cyan))
            } else {
                Span::styled("", Style::default())
            },
        ])]))
        .style(Style::default().bg(Color::Black));
        f.render_widget(input, chunks[0]);

        // slash command autocomplete
        if let Some(prefix) = self.slash_prefix() {
            let suggestions = self.slash_suggestions(prefix);
            if !suggestions.is_empty() {
                let visible: Vec<String> = suggestions
                    .into_iter()
                    .enumerate()
                    .take(8)
                    .map(|(i, name)| {
                        if i == self.slash_selection {
                            format!("▸ /{name}  ")
                        } else {
                            format!("  /{name}  ")
                        }
                    })
                    .collect();
                let line = Line::styled(
                    visible.join("").trim_end().to_string(),
                    Style::default().fg(Color::DarkGray).bg(Color::Black),
                );
                let para = Paragraph::new(Text::from(vec![line]));
                f.render_widget(para, chunks[1]);
            }
        }
    }

    fn draw_footer_bar(&self, f: &mut Frame<'_>, area: Rect) {
        let sep = Span::styled(" │ ", Style::default().fg(Color::DarkGray));
        let footer = Line::from(vec![
            Span::styled("enter send", Style::default().fg(Color::White)),
            sep.clone(),
            Span::styled("↑↓ scroll", Style::default().fg(Color::White)),
            sep.clone(),
            Span::styled("/help commands", Style::default().fg(Color::White)),
            sep.clone(),
            Span::styled("r toggle reasoning", Style::default().fg(Color::White)),
            sep.clone(),
            Span::styled(format!("${}", self.last_cost()), Style::default().fg(Color::White)),
            sep,
            Span::styled(format!("{} tok", self.last_tokens()), Style::default().fg(Color::White)),
        ]);
        let bar = Paragraph::new(Text::from(vec![footer]))
            .style(Style::default().bg(Color::Black))
            .alignment(Alignment::Left);
        f.render_widget(bar, area);
    }

    fn slash_prefix(&self) -> Option<String> {
        if !self.input.starts_with('/') {
            return None;
        }
        let after = &self.input[1..];
        // only show autocomplete when there is no space yet
        if after.contains(' ') {
            return None;
        }
        Some(after.to_string())
    }

    fn slash_suggestions(&self, prefix: String) -> Vec<String> {
        let mut matches: Vec<String> = self
            .cached_command_names
            .iter()
            .filter(|n| n.starts_with(&prefix))
            .take(8)
            .cloned()
            .collect();
        matches.sort();
        matches
    }

    fn context_tokens(&self) -> (usize, usize, usize) {
        (self.cached_total_tokens, self.cached_limit_tokens, self.cached_context_pct)
    }

    fn last_cost(&self) -> String {
        self.cached_cost.clone()
    }

    fn last_tokens(&self) -> usize {
        self.cached_last_tokens
    }

    fn build_detail_lines(&self, viewer: &ContextViewerState) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let Some(turn) = viewer.spec.turns.get(viewer.selected) else {
            return lines;
        };
        for block in &turn.blocks {
            match block {
                ViewBlock::UserText { text, tokens } => {
                    lines.push(Line::from(format!("user (~{tokens}t)")));
                    lines.push(Line::from(text.clone()));
                }
                ViewBlock::AssistantText { text, tokens } => {
                    lines.push(Line::from(format!("assistant (~{tokens}t)")));
                    lines.push(Line::from(text.clone()));
                }
                ViewBlock::ToolCall { name, input_json, tokens } => {
                    lines.push(Line::from(format!("tool call: {name} (~{tokens}t)")));
                    lines.push(Line::from(input_json.clone()));
                }
                ViewBlock::ToolResult { tool_name, content, tokens } => {
                    lines.push(Line::from(format!("tool result: {tool_name} (~{tokens}t)")));
                    lines.push(Line::from(content.clone()));
                }
            }
            lines.push(Line::from(""));
        }
        lines
    }

    fn draw_context_viewer(&self, f: &mut Frame<'_>, viewer: &ContextViewerState) {
        let area = f.area();
        let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).split(area);
        let hchunks = Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(chunks[0]);

        // left pane: turn list with a status footer inside the border
        let left_block = Block::default().borders(Borders::ALL).title("Turns");
        let left_inner = left_block.inner(hchunks[0]);
        f.render_widget(left_block, hchunks[0]);
        let left_chunks =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(left_inner);

        let items: Vec<ListItem<'_>> = viewer
            .spec
            .turns
            .iter()
            .enumerate()
            .map(|(i, turn)| {
                let base = format!("[turn {}] {} (~{}t)", i, turn.preview, turn.tokens_est);
                let text = if i == viewer.selected { base } else { format!("  {base}") };
                let style = if i == viewer.selected {
                    Style::default().fg(Color::Yellow)
                } else if !turn.in_window {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(Line::from(text)).style(style)
            })
            .collect();
        let list = List::new(items)
            .highlight_symbol("▶ ")
            .highlight_style(Style::default().fg(Color::Yellow));
        let mut list_state = ListState::default();
        list_state.select(Some(viewer.selected));
        f.render_stateful_widget(list, left_chunks[0], &mut list_state);

        let status_text = format!(
            "Total: {}t / {}t ({}%)",
            viewer.spec.total_tokens, viewer.spec.limit_tokens, viewer.spec.context_pct
        );
        let status_para =
            Paragraph::new(status_text).style(Style::default().fg(Color::Yellow).bg(Color::Black));
        f.render_widget(status_para, left_chunks[1]);

        // right pane: detail view of the selected turn
        let right_block = Block::default().borders(Borders::ALL).title("Detail");
        let detail_lines = self.build_detail_lines(viewer);
        let detail_height = hchunks[1].height.saturating_sub(2) as usize;
        let max_scroll =
            detail_lines.len().saturating_sub(detail_height).min(u16::MAX as usize) as u16;
        let scroll = viewer.detail_scroll.min(max_scroll);
        let detail = Paragraph::new(detail_lines)
            .block(right_block)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0));
        f.render_widget(detail, hchunks[1]);

        // bottom row: confirmation prompt or help
        let bottom_text = if viewer.confirm_drop {
            "Drop this turn? (y/n)".to_string()
        } else if viewer.confirm_clear {
            "Clear all history? (y/n)".to_string()
        } else {
            "↑↓ navigate · Tab focus · d drop · c clear · Esc close".to_string()
        };
        let bottom =
            Paragraph::new(bottom_text).style(Style::default().fg(Color::Yellow).bg(Color::Black));
        f.render_widget(bottom, chunks[1]);
    }

    async fn handle_event(&mut self, evt: Event) -> anyhow::Result<bool> {
        if let Event::Key(k) = evt {
            if k.kind != KeyEventKind::Press {
                return Ok(true);
            }
            // quit shortcuts are always active, even when the context viewer is open
            if k.modifiers.contains(KeyModifiers::CONTROL)
                && (k.code == KeyCode::Char('c') || k.code == KeyCode::Char('d'))
            {
                return Ok(false);
            }
            if self.context_viewer.is_some() {
                return self.handle_context_viewer_key(k).await;
            }

            // slash autocomplete navigation
            if self.slash_prefix().is_some() {
                match k.code {
                    KeyCode::Tab => {
                        if let Some(prefix) = self.slash_prefix() {
                            let suggestions = self.slash_suggestions(prefix);
                            if !suggestions.is_empty() {
                                let idx = self.slash_selection.min(suggestions.len() - 1);
                                self.input = format!("/{} ", suggestions[idx]);
                                self.slash_selection = 0;
                            }
                        }
                        return Ok(true);
                    }
                    KeyCode::Up => {
                        if self.slash_selection > 0 {
                            self.slash_selection -= 1;
                        }
                        return Ok(true);
                    }
                    KeyCode::Down => {
                        self.slash_selection += 1;
                        return Ok(true);
                    }
                    KeyCode::Esc => {
                        self.input.clear();
                        self.slash_selection = 0;
                        return Ok(true);
                    }
                    _ => {}
                }
            }

            match k.code {
                KeyCode::Enter if !self.busy && !self.input.is_empty() => {
                    let prompt = std::mem::take(&mut self.input);
                    self.slash_selection = 0;
                    self.messages.push(TuiMessage { role: Role::User, content: prompt.clone() });
                    if let Some(rest) = prompt.strip_prefix('/') {
                        let mut parts = rest.splitn(2, ' ');
                        let cmd_name = parts.next().unwrap_or_default().to_string();
                        let args = parts.next().unwrap_or_default().to_string();
                        let (ctx, commands) = {
                            let agent = self.agent.lock().await;
                            let ctx = CommandContext {
                                model: agent.config.model.clone(),
                                system_prompt: agent
                                    .history
                                    .system_prompt
                                    .clone()
                                    .unwrap_or_default(),
                                args: args.clone(),
                                snapshot: agent.build_conversation_snapshot(),
                            };
                            (ctx, agent.commands.clone())
                        };
                        match commands.dispatch(&cmd_name, &ctx) {
                            Err(e) => {
                                self.messages.push(TuiMessage {
                                    role: Role::System,
                                    content: format!("{e}"),
                                });
                            }
                            Ok(result) => {
                                self.messages.push(TuiMessage {
                                    role: Role::System,
                                    content: result.message,
                                });
                                match result.action {
                                    None => {}
                                    Some(CommandAction::OpenView(name)) => {
                                        self.open_view(&name).await;
                                    }
                                    Some(CommandAction::ClearHistory) => {
                                        let mut agent = self.agent.lock().await;
                                        agent.history.clear_messages();
                                        drop(agent);
                                        self.messages.clear();
                                        self.refresh_cache().await;
                                    }
                                    Some(CommandAction::UndoLastTurn) => {
                                        let mut agent = self.agent.lock().await;
                                        agent.history.drop_last_turn();
                                        drop(agent);
                                        self.rebuild_messages_from_history().await;
                                        self.refresh_cache().await;
                                    }
                                    Some(CommandAction::SetModel(m)) => {
                                        let mut agent = self.agent.lock().await;
                                        if let Some(cfg) = Arc::get_mut(&mut agent.config) {
                                            cfg.model = m.clone();
                                            self.model_name = m;
                                        } else {
                                            self.messages.push(TuiMessage {
                                                role: Role::System,
                                                content: format!(
                                                    "model will change to {m} on restart"
                                                ),
                                            });
                                        }
                                        drop(agent);
                                        self.refresh_cache().await;
                                    }
                                    Some(CommandAction::ReloadExtensions) => {
                                        self.start_extension_load().await;
                                    }
                                    Some(CommandAction::Exit) => {
                                        return Ok(false);
                                    }
                                    Some(CommandAction::RunTurn(text)) => {
                                        self.messages.push(TuiMessage {
                                            role: Role::User,
                                            content: text.clone(),
                                        });
                                        self.busy = true;
                                        self.streaming.clear();
                                        self.reasoning.clear();
                                        self.follow_bottom = true;
                                        self.status = "thinking".to_string();
                                        self.start_turn(text).await;
                                    }
                                }
                            }
                        }
                    } else {
                        self.busy = true;
                        self.streaming.clear();
                        self.reasoning.clear();
                        self.follow_bottom = true;
                        self.status = "thinking".to_string();
                        self.start_turn(prompt).await;
                    }
                }
                // scroll keys work anytime, even while busy
                KeyCode::Up => {
                    self.follow_bottom = false;
                    self.chat_scroll = self.chat_scroll.saturating_sub(1);
                }
                KeyCode::Down => {
                    self.chat_scroll = self.chat_scroll.saturating_add(1);
                    // follow_bottom re-armed only via End / PageBottom; Down alone advances
                }
                KeyCode::PageUp => {
                    self.follow_bottom = false;
                    self.chat_scroll = self.chat_scroll.saturating_sub(10);
                }
                KeyCode::PageDown => {
                    self.chat_scroll = self.chat_scroll.saturating_add(10);
                }
                KeyCode::End => {
                    self.follow_bottom = true;
                }
                KeyCode::Home => {
                    self.follow_bottom = false;
                    self.chat_scroll = 0;
                }
                KeyCode::Char('r') if !self.busy => {
                    self.reasoning_visible = !self.reasoning_visible;
                }
                KeyCode::Char(c) if !self.busy && !k.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.input.push(c);
                }
                KeyCode::Backspace if !self.busy => {
                    self.input.pop();
                }
                _ => {}
            }
        }
        Ok(true)
    }

    async fn handle_context_viewer_key(&mut self, k: KeyEvent) -> anyhow::Result<bool> {
        let Some(viewer) = self.context_viewer.as_mut() else {
            return Ok(true);
        };
        match k.code {
            KeyCode::Esc => {
                if viewer.confirm_drop || viewer.confirm_clear {
                    viewer.confirm_drop = false;
                    viewer.confirm_clear = false;
                } else {
                    self.context_viewer = None;
                }
            }
            KeyCode::Char(c) if (c == 'n' || c == 'N') => {
                viewer.confirm_drop = false;
                viewer.confirm_clear = false;
            }
            KeyCode::Char(c) if (c == 'y' || c == 'Y') => {
                if viewer.confirm_drop {
                    if let Some(turn) = viewer.spec.turns.get(viewer.selected).cloned() {
                        let mut agent = self.agent.lock().await;
                        agent.history.drop_range(turn.msg_index, turn.msg_count);
                        drop(agent);
                        self.rebuild_context_viewer().await;
                    }
                } else if viewer.confirm_clear {
                    let mut agent = self.agent.lock().await;
                    agent.history.clear_messages();
                    drop(agent);
                    self.rebuild_context_viewer().await;
                }
            }
            KeyCode::Tab => {
                viewer.detail_focus = !viewer.detail_focus;
            }
            KeyCode::Up
                if !viewer.detail_focus && !viewer.confirm_drop && !viewer.confirm_clear =>
            {
                viewer.selected = viewer.selected.saturating_sub(1);
                viewer.detail_scroll = 0;
            }
            KeyCode::Down
                if !viewer.detail_focus && !viewer.confirm_drop && !viewer.confirm_clear =>
            {
                if viewer.selected + 1 < viewer.spec.turns.len() {
                    viewer.selected += 1;
                }
                viewer.detail_scroll = 0;
            }
            KeyCode::PageUp if viewer.detail_focus => {
                viewer.detail_scroll = viewer.detail_scroll.saturating_sub(10);
            }
            KeyCode::PageDown if viewer.detail_focus => {
                viewer.detail_scroll = viewer.detail_scroll.saturating_add(10);
            }
            KeyCode::Char('d')
                if !viewer.detail_focus && !viewer.confirm_drop && !viewer.confirm_clear =>
            {
                viewer.confirm_drop = true;
            }
            KeyCode::Char('c')
                if !viewer.detail_focus && !viewer.confirm_clear && !viewer.confirm_drop =>
            {
                viewer.confirm_clear = true;
            }
            _ => {}
        }
        Ok(true)
    }

    // spawn background task that drives one full turn (possibly multiple tool rounds)
    async fn start_turn(&mut self, prompt: String) {
        let (tx, rx) = mpsc::channel::<UiEvent>(64);
        self.ui_rx = Some(rx);
        let agent = self.agent.clone();

        tokio::spawn(async move {
            run_turn_streamed(agent, prompt, tx).await;
        });
    }

    // spawn background task that loads extensions and signs events
    async fn start_extension_load(&mut self) {
        let (tx, rx) = mpsc::channel::<UiEvent>(64);
        self.ui_rx = Some(rx);
        let agent = self.agent.clone();
        self.status = "loading extensions…".to_string();

        tokio::task::spawn_blocking(move || {
            let observe: Arc<dyn Fn(String) + Send + Sync> = Arc::new(move |msg| {
                if let Some(rest) = msg.strip_prefix("extension_loaded:") {
                    let _ = tx.blocking_send(UiEvent::ExtensionLoaded(rest.to_string()));
                }
            });
            let mut a = agent.blocking_lock();
            a.load_extensions_with(observe);
        });
    }

    async fn handle_ui_event(&mut self, evt: UiEvent) -> anyhow::Result<bool> {
        match evt {
            UiEvent::Delta(delta) => {
                self.streaming.push_str(&delta);
            }
            UiEvent::Reasoning(delta) => {
                self.reasoning.push_str(&delta);
            }
            UiEvent::RoundDone => {
                // keep reasoning bubble visible until next turn starts;
                // only flush the assistant answer here.
                if !self.streaming.is_empty() {
                    self.messages.push(TuiMessage {
                        role: Role::Assistant,
                        content: std::mem::take(&mut self.streaming),
                    });
                }
            }
            UiEvent::ToolResult { name, preview } => {
                self.messages
                    .push(TuiMessage {
                        role: Role::Tool, content: format!("{name} → {preview}")
                    });
            }
            UiEvent::Status(s) => {
                self.status = s;
            }
            UiEvent::Error(e) => {
                self.messages
                    .push(TuiMessage { role: Role::Assistant, content: format!("error: {e}") });
                self.streaming.clear();
                self.reasoning.clear();
                self.busy = false;
                self.ui_rx = None;
            }
            UiEvent::TurnDone => {
                self.busy = false;
                self.streaming.clear();
                self.reasoning.clear();
                self.ui_rx = None;
                self.refresh_cache().await;
            }
            UiEvent::ExtensionLoaded(name) => {
                self.messages.push(TuiMessage {
                    role: Role::Tool,
                    content: format!("loaded extension: {name}"),
                });
                if self.status.starts_with("loading extensions") {
                    self.status = "ready".to_string();
                }
                self.refresh_cache().await;
            }
        }
        Ok(true)
    }
}

// background streaming driver — shares Agent via Arc<Mutex<Agent>>
// lock is held only during req build, history append, tool dispatch — never across .recv().await
async fn run_turn_streamed(agent: Arc<Mutex<Agent>>, prompt: String, tx: mpsc::Sender<UiEvent>) {
    if let Err(e) = run_turn_inner(&agent, prompt, &tx).await {
        let _ = tx.send(UiEvent::Error(e.to_string())).await;
    }
    let _ = tx.send(UiEvent::TurnDone).await;
}

async fn run_turn_inner(
    agent: &Arc<Mutex<Agent>>,
    prompt: String,
    tx: &mpsc::Sender<UiEvent>,
) -> anyhow::Result<()> {
    {
        let mut a = agent.lock().await;
        a.history.append_user(prompt);
    }

    loop {
        // build request under lock, then release
        let req = {
            let a = agent.lock().await;
            a.history.to_request(&a.config.model, &a.tools.schemas())
        };
        // start stream under lock (needs &self), then release lock during .recv().await
        let mut rx = {
            let a = agent.lock().await;
            a.llm.complete_stream(req).await?
        };

        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut last_cost: Option<crate::llm::CallCost> = None;
        while let Some(evt) = rx.recv().await {
            match evt {
                StreamEvent::Content(delta) => {
                    content.push_str(&delta);
                    let _ = tx.send(UiEvent::Delta(delta)).await;
                }
                StreamEvent::Reasoning(delta) => {
                    reasoning.push_str(&delta);
                    let _ = tx.send(UiEvent::Reasoning(delta)).await;
                }
                StreamEvent::ToolCallStart(tc) => {
                    tool_calls.push(tc);
                }
                StreamEvent::ToolCallDelta(_) => {}
                StreamEvent::Cost(c) => {
                    last_cost = Some(c);
                }
                StreamEvent::Done => break,
                StreamEvent::Error(e) => {
                    anyhow::bail!("stream error: {e}");
                }
            }
        }

        let _ = tx.send(UiEvent::RoundDone).await;

        if let Some(c) = &last_cost {
            let _ = tx
                .send(UiEvent::Status(format!(
                    "{} cost ${:.6} (in={} out={} tok)",
                    {
                        let a = agent.lock().await;
                        a.config.model.clone()
                    },
                    c.total_cost,
                    c.input_tokens,
                    c.output_tokens
                )))
                .await;
        }

        // append assistant + dispatch tools under lock
        if tool_calls.is_empty() {
            let mut a = agent.lock().await;
            a.history.append_assistant_with_reasoning(content, reasoning, Vec::new());
            return Ok(());
        }
        {
            let mut a = agent.lock().await;
            a.history.append_assistant_with_reasoning(content, reasoning, tool_calls.clone());
        }
        for call in &tool_calls {
            let result = {
                let a = agent.lock().await;
                a.tools.dispatch_call(call).await.unwrap_or_else(|e| format!("error: {e}"))
            };
            let preview: String = result.chars().take(200).collect();
            let _ = tx.send(UiEvent::ToolResult { name: call.name.clone(), preview }).await;
            let mut a = agent.lock().await;
            a.history.append_tool_result(call.id.clone(), result);
        }
    }
}

// render a message with a left-border accent, role label, and indented continuation.
// width is the available inner width of the chat area. lines are pushed into `lines`.
fn render_message(lines: &mut Vec<Line<'_>>, m: &TuiMessage, model_name: &str, width: usize) {
    let (colour, label, prefix) = match m.role {
        Role::User => (Color::Cyan, "you".to_string(), " · "),
        Role::Assistant => (Color::Blue, model_name.to_string(), " · "),
        Role::Tool => {
            // tool messages already include `name → preview` from the streaming path
            return render_tool_card(lines, m, width);
        }
        Role::System => (Color::DarkGray, "sys".to_string(), " "),
    };

    let style = Style::default().fg(colour).bg(Color::Black);
    let mut first = true;
    for raw in m.content.lines() {
        if raw.is_empty() {
            lines.push(Line::styled(
                format!("│{:<width$}", "", width = width.saturating_sub(1)),
                style,
            ));
            continue;
        }
        // manual char-boundary-respecting wrap
        let mut start = 0;
        let bytes = raw.as_bytes();
        while start < bytes.len() {
            let end = (start + width.saturating_sub(1)).min(bytes.len());
            let mut e = end;
            while e > start && !raw.is_char_boundary(e) {
                e -= 1;
            }
            if e == start {
                e = end;
            }
            let chunk = &raw[start..e];
            if first {
                let head = format!("│ {label}{prefix}{chunk}");
                lines.push(Line::styled(head, style));
                first = false;
            } else {
                let indent = label.chars().count() + prefix.chars().count() + 3;
                lines.push(Line::styled(
                    format!(
                        "│{}{:remaining$}",
                        " ".repeat(indent),
                        chunk,
                        remaining = width.saturating_sub(indent)
                    ),
                    style,
                ));
            }
            start = e;
        }
    }
    if m.content.is_empty() {
        lines.push(Line::styled(format!("│ {label}{prefix}"), style));
    }
    lines.push(Line::from(""));
}

// render a tool card on a single line with a ⚡ prefix and yellow left border.
fn render_tool_card(lines: &mut Vec<Line<'_>>, m: &TuiMessage, width: usize) {
    let style = Style::default().fg(Color::Yellow).bg(Color::Black);
    let prefix = "│ ⚡ ";
    let prefix_len = prefix.chars().count();
    let text = m.content.clone();
    let mut start = 0;
    let bytes = text.as_bytes();
    let inner = width.saturating_sub(prefix_len);
    let mut first = true;
    while start < bytes.len() {
        let end = (start + inner).min(bytes.len());
        let mut e = end;
        while e > start && !text.is_char_boundary(e) {
            e -= 1;
        }
        if e == start {
            e = end;
        }
        let chunk = &text[start..e];
        lines.push(Line::styled(format!("{}{}", prefix, chunk), style));
        if first {
            first = false;
        }
        start = e;
    }
    if text.is_empty() {
        lines.push(Line::styled(format!("{}…", prefix), style));
    }
    lines.push(Line::from(""));
}

fn thinking_spinner() -> char {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let frames = ['◜', '◝', '◞', '◟'];
    let idx = ((nanos / 250_000_000) as usize) % frames.len();
    frames[idx]
}

fn setup_terminal() -> anyhow::Result<Terminal<CrosstermBackend<std::io::Stdout>>> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    Ok(Terminal::new(backend)?)
}

fn restore_terminal() -> anyhow::Result<()> {
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;
    Ok(())
}

pub async fn run(config: Config, llm: Box<dyn LlmClient>) -> anyhow::Result<()> {
    let agent = Agent::new(config, llm);
    TuiAgent::new(agent).run().await
}
