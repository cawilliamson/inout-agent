// fullview.rhai — full view mode extension
//
// registers the `full` view: shows everything sent to the model including
// system prompt, reasoning blocks, tool calls, tool results. one turn per
// user/assistant pair, with all intermediate tool rounds inline.

fn register(api) {
    api.register_view("full", "full view — all llm traffic", |snap| {
        let messages = snap.messages;
        let system_prompt = if messages.len() > 0 && messages[0].role == "system" {
            messages[0].content
        } else {
            ""
        };

        // gather content messages (skip the system prompt message at index 0)
        let content_msgs = [];
        let i = 0;
        while i < messages.len() {
            if i == 0 && messages[i].role == "system" {
                i += 1;
                continue;
            }
            content_msgs.push(messages[i]);
            i += 1;
        }

        // group into turns: each turn starts at a user message and includes
        // all following assistant/tool messages until the next user message.
        let turns = [];
        let idx = 0;
        let total_tokens = 0;
        let limit_tokens = 128000;

        while idx < content_msgs.len() {
            let msg = content_msgs[idx];
            if msg.role != "user" {
                idx += 1;
                continue;
            }

            let start = idx;
            let user_text = msg.content;
            let user_tokens = user_text.len() / 4;
            let blocks = [];
            blocks.push(#{
                kind: "user_text",
                text: user_text,
                tokens: user_tokens,
            });
            total_tokens += user_tokens;

            idx += 1;
            // collect assistant + tool messages until next user
            while idx < content_msgs.len() && content_msgs[idx].role != "user" {
                let m = content_msgs[idx];

                if m.reasoning.len() > 0 {
                    let r_tokens = m.reasoning.len() / 4;
                    blocks.push(#{
                        kind: "assistant_text",
                        text: "[reasoning] " + m.reasoning,
                        tokens: r_tokens,
                    });
                    total_tokens += r_tokens;
                }

                if m.content.len() > 0 {
                    let c_tokens = m.content.len() / 4;
                    blocks.push(#{
                        kind: "assistant_text",
                        text: m.content,
                        tokens: c_tokens,
                    });
                    total_tokens += c_tokens;
                }

                // tool calls
                let tc = m.tool_calls;
                let j = 0;
                while j < tc.len() {
                    let tc_item = tc[j];
                    let arg_str = tc_item.arguments_json;
                    let tc_tokens = (tc_item.name.len() + arg_str.len()) / 4;
                    blocks.push(#{
                        kind: "tool_call",
                        name: tc_item.name,
                        input_json: arg_str,
                        tokens: tc_tokens,
                    });
                    total_tokens += tc_tokens;
                    j += 1;
                }

                // tool result
                if m.role == "tool" {
                    let tr_tokens = m.content.len() / 4;
                    blocks.push(#{
                        kind: "tool_result",
                        tool_name: "tool",
                        content: m.content,
                        tokens: tr_tokens,
                    });
                    total_tokens += tr_tokens;
                }

                idx += 1;
            }

            let preview = if user_text.len() > 60 {
                user_text.substring(0, 60) + "…"
            } else {
                user_text
            };

            let turn_tokens = 0;
            let bi = 0;
            while bi < blocks.len() {
                turn_tokens += blocks[bi].tokens;
                bi += 1;
            }

            turns.push(#{
                msg_index: start,
                msg_count: idx - start,
                preview: preview,
                tokens_est: turn_tokens,
                in_window: true,
                blocks: blocks,
            });
        }

        // prepend system prompt as a pseudo-turn if present
        if system_prompt.len() > 0 {
            let sp_tokens = system_prompt.len() / 4;
            total_tokens += sp_tokens;
            turns.insert(0, #{
                msg_index: 0,
                msg_count: 0,
                preview: "[system prompt]",
                tokens_est: sp_tokens,
                in_window: true,
                blocks: [#{
                    kind: "assistant_text",
                    text: system_prompt,
                    tokens: sp_tokens,
                }],
            });
        }

        let context_pct = if limit_tokens > 0 {
            (total_tokens * 100) / limit_tokens
        } else {
            0
        };

        #{
            turns: turns,
            total_tokens: total_tokens,
            limit_tokens: limit_tokens,
            context_pct: context_pct,
        }
    });

    // /full command — opens the full view overlay
    api.register_command("full", "show full llm traffic view", |ctx| {
        #{
            message: "opening full view…",
            action: "open_view:full",
        }
    });
}