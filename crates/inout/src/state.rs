use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum State {
    AwaitingUser,
    Thinking,
    ToolRunning,
    Responding,
}

impl State {
    pub fn next(self, event: Event) -> Self {
        match (self, event) {
            (State::AwaitingUser, Event::UserMessage) => State::Thinking,
            (State::Thinking, Event::ToolCalls) => State::ToolRunning,
            (State::Thinking, Event::FinalResponse) => State::Responding,
            (State::ToolRunning, Event::ToolsDone) => State::Thinking,
            (State::Responding, Event::TurnComplete) => State::AwaitingUser,
            (s, _) => s, // illegal transition ignored, kept deterministic
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    UserMessage,
    ToolCalls,
    ToolsDone,
    FinalResponse,
    TurnComplete,
}
