//! minimal hook bus.
//!
//! v1.0 is intentionally tiny: typed events can be observed and emitted.
//! v2.x observability extension replaces this with a full subscriber bus.

use tokio::sync::broadcast;

/// an event that can be observed by subscribers.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum HookEvent {
    /// a tool was called.
    ToolCall {
        /// tool name.
        name: String,
        /// parsed arguments.
        args: serde_json::Value,
    },
    /// a tool returned.
    ToolResult {
        /// tool name.
        name: String,
        /// tool output.
        output: String,
        /// error message if the tool failed.
        error: Option<String>,
    },
    /// an llm request is about to be sent.
    BeforeProviderPayload(serde_json::Value),
    /// a lifecycle log line.
    Lifecycle(String),
}

/// a simple typed broadcast bus.
#[derive(Clone, Debug)]
pub struct HookBus {
    sender: broadcast::Sender<HookEvent>,
}

impl HookBus {
    /// create a bus with the given buffer capacity.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// emit an event to all active subscribers.
    pub fn emit(&self, event: HookEvent) {
        // best-effort broadcast; dropping lagged subscribers is fine.
        let _ = self.sender.send(event);
    }

    /// subscribe to events.
    pub fn subscribe(&self) -> broadcast::Receiver<HookEvent> {
        self.sender.subscribe()
    }
}

impl Default for HookBus {
    fn default() -> Self {
        Self::new(128)
    }
}
