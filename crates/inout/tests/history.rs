#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
use inout::history::{History, Role};
use inout_testing::{scenario, then, when};

#[test]
fn system_prompt_prepended_once() {
    let mut s = scenario!("core", "Minimal configuration", "Config loads required fields");
    let mut h = History::with_system_prompt(20, "you are a helpful agent".to_string());
    h.append_user("hi".to_string());
    h.append_assistant("hello".to_string());
    when!(s, "to_request is called on a history with a system prompt", {
        let req = h.to_request("m", &[]);
        then!(s, "the system message is prepended exactly once before the user and assistant messages", {
            assert_eq!(req.messages.len(), 3);
            assert_eq!(req.messages[0].role, Role::System);
            assert_eq!(req.messages[0].content, "you are a helpful agent");
            assert_eq!(req.messages[1].role, Role::User);
            assert_eq!(req.messages[2].role, Role::Assistant);
            let system_count = req.messages.iter().filter(|m| m.role == Role::System).count();
            assert_eq!(system_count, 1);
        });
    });
}

#[test]
fn no_system_prompt_when_none() {
    let mut s = scenario!("core", "Minimal configuration", "Config loads required fields");
    let mut h = History::new(20);
    h.append_user("hi".to_string());
    when!(s, "to_request is called on a history with no system prompt", {
        let req = h.to_request("m", &[]);
        then!(s, "only the user message is present", {
            assert_eq!(req.messages.len(), 1);
            assert_eq!(req.messages[0].role, Role::User);
        });
    });
}

#[test]
fn system_prompt_not_mutated_by_repeated_calls() {
    let mut s = scenario!("core", "Minimal configuration", "Config loads required fields");
    let mut h = History::with_system_prompt(20, "sys".to_string());
    h.append_user("a".to_string());
    when!(s, "to_request is called twice on the same history", {
        let _ = h.to_request("m", &[]);
        let _ = h.to_request("m", &[]);
        then!(s, "the underlying messages vec never stores the system message", {
            assert_eq!(h.messages.len(), 1);
            assert_eq!(h.messages[0].role, Role::User);
        });
    });
}

#[test]
fn jsonl_roundtrip_preserves_messages() {
    let mut s = scenario!("sessions", "SessionEntry jsonl round-trip", "Deserialize every entry kind");
    let mut h = History::new(20);
    h.append_user("hello".to_string());
    h.append_assistant("world".to_string());
    when!(s, "the history is serialized to jsonl and deserialized back", {
        let jsonl = h.to_jsonl().unwrap();
        let restored = History::from_jsonl(&jsonl).unwrap();
        then!(s, "the restored history equals the original", {
            assert_eq!(restored.messages.len(), 2);
            assert_eq!(restored.messages[0].content, "hello");
            assert_eq!(restored.messages[1].content, "world");
        });
    });
}