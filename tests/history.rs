use twobobs::history::{History, Role};
use twobobs::tools::ToolCall;

#[test]
fn system_prompt_prepended_once() {
    let mut h = History::with_system_prompt(20, "you are a helpful agent".to_string());
    h.append_user("hi".to_string());
    h.append_assistant("hello".to_string());

    let req = h.to_request("m", &[]);
    // first message is system, content matches, only one system message
    assert_eq!(req.messages.len(), 3);
    assert_eq!(req.messages[0].role, Role::System);
    assert_eq!(req.messages[0].content, "you are a helpful agent");
    assert_eq!(req.messages[1].role, Role::User);
    assert_eq!(req.messages[2].role, Role::Assistant);
    let system_count = req.messages.iter().filter(|m| m.role == Role::System).count();
    assert_eq!(system_count, 1);
}

#[test]
fn no_system_prompt_when_none() {
    let mut h = History::new(20);
    h.append_user("hi".to_string());
    let req = h.to_request("m", &[]);
    assert_eq!(req.messages.len(), 1);
    assert_eq!(req.messages[0].role, Role::User);
}

#[test]
fn system_prompt_not_mutated_by_repeated_calls() {
    let mut h = History::with_system_prompt(20, "sys".to_string());
    h.append_user("a".to_string());
    let _ = h.to_request("m", &[]);
    let _ = h.to_request("m", &[]);
    // underlying messages vec never stores the system message
    assert_eq!(h.messages.len(), 1);
    assert_eq!(h.messages[0].role, Role::User);
}

#[test]
fn jsonl_roundtrip_preserves_messages() {
    let mut h = History::new(20);
    h.append_user("hello".to_string());
    h.append_assistant("world".to_string());
    let jsonl = h.to_jsonl().unwrap();
    let restored = History::from_jsonl(&jsonl).unwrap();
    assert_eq!(restored.messages.len(), 2);
    assert_eq!(restored.messages[0].content, "hello");
    assert_eq!(restored.messages[1].content, "world");
}