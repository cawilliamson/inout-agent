# Proposal: Streaming TUI with system prompt and cost tracking

## Intent

Add a terminal UI with streaming LLM responses, a default system prompt, and per-turn cost display. The agent needed a usable interface beyond CLI print.

## Scope

- ratatui-based TUI.
- SSE streaming from OpenAI-compatible endpoints.
- System prompt injection.
- Reasoning/thinking display.
- Cost tracking: input/output tokens and USD estimate.

Out of scope: context management, skills, extensions.

## Approach

- ratatui + crossterm for the TUI.
- SSE parser for streaming chunks.
- System prompt prepended to LLM requests.
- Cost computed from token counts and model rates.
