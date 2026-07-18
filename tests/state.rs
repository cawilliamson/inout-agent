use twobobs::state::{Event, State};

#[test]
fn state_transitions_legal() {
    let s = State::AwaitingUser.next(Event::UserMessage);
    assert_eq!(s, State::Thinking);

    let s = s.next(Event::ToolCalls);
    assert_eq!(s, State::ToolRunning);

    let s = s.next(Event::ToolsDone);
    assert_eq!(s, State::Thinking);

    let s = s.next(Event::FinalResponse);
    assert_eq!(s, State::Responding);

    let s = s.next(Event::TurnComplete);
    assert_eq!(s, State::AwaitingUser);
}

#[test]
fn illegal_transitions_preserve_state() {
    let s = State::AwaitingUser.next(Event::ToolCalls);
    assert_eq!(s, State::AwaitingUser);

    let s = State::ToolRunning.next(Event::UserMessage);
    assert_eq!(s, State::ToolRunning);

    let s = State::Responding.next(Event::FinalResponse);
    assert_eq!(s, State::Responding);
}