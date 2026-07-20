// context: build a tui view spec from the conversation snapshot.
//
// the binary invokes the registered `context` view with a snapshot map:
//   #{ messages: [#{ role, content, tool_calls, tool_call_id }], max_turns }
// and renders the returned view-spec map as an interactive split-pane viewer.
// this script owns the *computation* (turn grouping, token estimates); rust
// owns rendering and keyboard interaction.

// rough token estimate: chars / 4. good enough for a budget inspector.
fn tokens(text) {
    if type_of(text) != "string" { return 0; }
    let n = text.len();
    return n / 4;
}

// first ~60 chars of a string, for the turn-list preview.
fn preview(text) {
    if type_of(text) != "string" { return ""; }
    let chars = text.to_chars();
    if chars.len() <= 60 { return text; }
    let out = "";
    let i = 0;
    while i < 60 {
        out += chars[i];
        i += 1;
    }
    return out;
}

// find the tool name for a tool_call_id by scanning backwards through messages.
fn tool_name_for(messages, id) {
    let i = 0;
    while i < messages.len() {
        let m = messages[i];
        if m.tool_calls.len() > 0 {
            let j = 0;
            while j < m.tool_calls.len() {
                let tc = m.tool_calls[j];
                if tc.id == id { return tc.name; }
                j += 1;
            }
        }
        i += 1;
    }
    return "";
}

// build the blocks array for one turn (messages[start..end]).
fn build_blocks(messages, start, end) {
    let blocks = [];
    let i = start;
    while i < end {
        let m = messages[i];
        if m.role == "user" {
            if m.content.len() > 0 {
                push(blocks, #{ kind: "user_text", text: m.content, tokens: tokens(m.content) });
            }
        } else if m.role == "assistant" {
            if m.content.len() > 0 {
                push(blocks, #{ kind: "assistant_text", text: m.content, tokens: tokens(m.content) });
            }
            let j = 0;
            while j < m.tool_calls.len() {
                let tc = m.tool_calls[j];
                let json = tc.arguments_json;
                let t = tokens(json);
                push(blocks, #{ kind: "tool_call", name: tc.name, input_json: json, tokens: t });
                j += 1;
            }
        } else if m.role == "tool" {
            let name = tool_name_for(messages, m.tool_call_id);
            push(blocks, #{ kind: "tool_result", tool_name: name, content: m.content, tokens: tokens(m.content) });
        }
        i += 1;
    }
    return blocks;
}

// count total chars across all messages (for proportional token scaling).
fn total_chars(messages) {
    let total = 0;
    let i = 0;
    while i < messages.len() {
        let m = messages[i];
        total += m.content.len();
        let j = 0;
        while j < m.tool_calls.len() {
            total += m.tool_calls[j].arguments_json.len();
            j += 1;
        }
        i += 1;
    }
    return total;
}

// char count for a slice of messages.
fn slice_chars(messages, start, end) {
    let total = 0;
    let i = start;
    while i < end {
        let m = messages[i];
        total += m.content.len();
        let j = 0;
        while j < m.tool_calls.len() {
            total += m.tool_calls[j].arguments_json.len();
            j += 1;
        }
        i += 1;
    }
    return total;
}

fn register(api) {
    api.register_view("context", "Context Viewer", |snap| {
        let messages = snap.messages;
        let max_turns = snap.max_turns;

        // locate every user turn (role == "user" with non-empty content)
        let turn_starts = [];
        let i = 0;
        while i < messages.len() {
            if messages[i].role == "user" {
                push(turn_starts, i);
            }
            i += 1;
        }

        let total_turns = turn_starts.len();
        // sliding window: only the last `max_turns` turns are "in window"
        let window_start = if total_turns > max_turns { total_turns - max_turns } else { 0 };

        let total_c = total_chars(messages);
        // approximate total tokens across the conversation
        let total_tokens = total_c / 4;

        let turns = [];
        let t = 0;
        while t < total_turns {
            let start = turn_starts[t];
            let end = if t + 1 < total_turns { turn_starts[t + 1] } else { messages.len() };
            let msg_count = end - start;

            let user_content = "";
            if start < messages.len() {
                user_content = messages[start].content;
            }

            let chars = slice_chars(messages, start, end);
            let tok = chars / 4;
            let in_window = t >= window_start;

            let blocks = build_blocks(messages, start, end);

            push(turns, #{
                msg_index: start,
                msg_count: msg_count,
                preview: preview(user_content),
                tokens_est: tok,
                in_window: in_window,
                blocks: blocks,
            });

            t += 1;
        }

        // rough context limit — 128k is a sane default when unknown
        let limit = 128000;
        let pct = if limit > 0 { (total_tokens * 100) / limit } else { 0 };
        if pct > 100 { pct = 100; }

        return #{
            turns: turns,
            total_tokens: total_tokens,
            limit_tokens: limit,
            context_pct: pct,
        };
    });
}