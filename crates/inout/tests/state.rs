#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
use inout::state::{Event, State};
use inout_testing::{scenario, then, when};

#[test]
fn state_transitions_legal() {
    let mut s = scenario!("core", "State machine transitions", "User message starts a turn");
    when!(s, "a session in the awaiting_user state receives a user message", {
        let next = State::AwaitingUser.next(Event::UserMessage);
        then!(s, "the session transitions to the thinking state", {
            assert_eq!(next, State::Thinking);
        });
    });
    let mut s = scenario!("core", "State machine transitions", "Tool calls from thinking");
    when!(s, "a session in the thinking state receives tool calls", {
        let next = State::Thinking.next(Event::ToolCalls);
        then!(s, "the session transitions to the tool_running state", {
            assert_eq!(next, State::ToolRunning);
        });
    });
    let mut s = scenario!("core", "State machine transitions", "Tool results return to thinking");
    when!(s, "a session in the tool_running state collects all tool results", {
        let next = State::ToolRunning.next(Event::ToolsDone);
        then!(s, "the session transitions back to the thinking state", {
            assert_eq!(next, State::Thinking);
        });
    });
    let mut s = scenario!("core", "State machine transitions", "Final response from thinking");
    when!(s, "a session in the thinking state produces text without further tool calls", {
        let next = State::Thinking.next(Event::FinalResponse);
        then!(s, "the session transitions to the responding state", {
            assert_eq!(next, State::Responding);
        });
    });
    let mut s = scenario!("core", "State machine transitions", "Turn completion returns to idle");
    when!(s, "a session in the responding state delivers the response", {
        let next = State::Responding.next(Event::TurnComplete);
        then!(s, "the session transitions to the awaiting_user state", {
            assert_eq!(next, State::AwaitingUser);
        });
    });
}

#[test]
fn illegal_transitions_preserve_state() {
    let mut s = scenario!("core", "State machine transitions", "Illegal transition is rejected");
    when!(s, "an awaiting_user session receives a non-user event", {
        let next = State::AwaitingUser.next(Event::ToolCalls);
        then!(s, "the session remains in the awaiting_user state", {
            assert_eq!(next, State::AwaitingUser);
        });
    });
    when!(s, "a tool_running session receives a user message", {
        let next = State::ToolRunning.next(Event::UserMessage);
        then!(s, "the session remains in the tool_running state", {
            assert_eq!(next, State::ToolRunning);
        });
    });
    when!(s, "a responding session receives a final response event", {
        let next = State::Responding.next(Event::FinalResponse);
        then!(s, "the session remains in the responding state", {
            assert_eq!(next, State::Responding);
        });
    });
}