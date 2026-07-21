//! bdd scenario harness for inout.
//!
//! each test binds to an openspec requirement and named scenario. on panic the
//! scenario header is printed to stderr before the panic propagates, so test
//! failures carry their spec provenance. steps (`given!`, `when!`, `then!`)
//! are sugar that logs the step label and nothing more -- the real assertion
//! work stays in the test body, so there is no `catch_unwind` or `unwindsafe`
//! dance and async tests work unchanged.
//!
//! example:
//!
//! ```ignore
//! use inout_testing::{scenario, given, when, then};
//!
//! #[test]
//! fn user_message_starts_a_turn() {
//!     let mut s = scenario!("core", "State machine transitions", "User message starts a turn");
//!     given!(s, "a session in the awaiting_user state");
//!     when!(s, "a user message is received");
//!     then!(s, "the session transitions to the thinking state", {
//!         assert_eq!(State::AwaitingUser.next(Event::UserMessage), State::Thinking);
//!     });
//! }
//! ```

use std::cell::Cell;
use std::fmt;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

static HOOK_INSTALLED: Once = Once::new();
static RECORDING: AtomicBool = AtomicBool::new(false);

/// scenario guard. drop prints the scenario header to stderr when the
/// dropping thread is unwinding, so the header appears above the panic
/// backtrace in test output.
#[derive(Debug)]
pub struct Scenario {
    spec: &'static str,
    requirement: &'static str,
    name: &'static str,
    steps: Vec<&'static str>,
}

impl Scenario {
    /// build a scenario guard. also prints the header once to stderr so it
    /// shows under `--nocapture` even on passing tests.
    pub fn new(spec: &'static str, requirement: &'static str, name: &'static str) -> Self {
        install_panic_hook();
        let s = Self { spec, requirement, name, steps: Vec::new() };
        let mut err = std::io::stderr().lock();
        let _ = writeln!(err, "\n--- scenario: {spec} / {requirement} / {name}");
        DEPTH.with(|d| d.set(d.get().saturating_add(1)));
        s
    }

    /// record a step label. printing happens here so the label lands in
    /// `--nocapture` output and, via the panic hook, in failure messages.
    pub fn step(&mut self, kind: &'static str, label: &'static str) {
        let depth = DEPTH.with(|d| d.get());
        let indent = "  ".repeat(depth as usize);
        let mut err = std::io::stderr().lock();
        let _ = writeln!(err, "{indent}{kind}: {label}");
        self.steps.push(label);
    }
}

impl Drop for Scenario {
    fn drop(&mut self) {
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        if std::thread::panicking() {
            let mut err = std::io::stderr().lock();
            let _ = writeln!(
                err,
                "\n=== failed scenario: {} / {} / {} ===",
                self.spec, self.requirement, self.name
            );
            if self.steps.is_empty() {
                let _ = writeln!(err, "no recorded steps before failure");
            } else {
                let _ = writeln!(err, "recorded steps:");
                for (i, step) in self.steps.iter().enumerate() {
                    let _ = writeln!(err, "  {i}: {step}");
                }
            }
        }
    }
}

impl fmt::Display for Scenario {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} / {} / {}", self.spec, self.requirement, self.name)
    }
}

fn install_panic_hook() {
    HOOK_INSTALLED.call_once(|| {
        RECORDING.store(true, Ordering::SeqCst);
    });
}

/// build a scenario guard. binds to a `mut` named local so the drop guard
/// lives for the whole test body and step recording can borrow it mutably.
#[macro_export]
macro_rules! scenario {
    ($spec:expr, $requirement:expr, $name:expr) => {{
        let mut __scenario = $crate::Scenario::new($spec, $requirement, $name);
        __scenario
    }};
}

/// record a "given" precondition step. the body is optional; when present it
/// runs inline so setup failures still carry the scenario context.
#[macro_export]
macro_rules! given {
    ($scenario:expr, $label:expr) => {
        $scenario.step("GIVEN", $label);
    };
    ($scenario:expr, $label:expr, $body:block) => {{
        $scenario.step("GIVEN", $label);
        $body
    }};
}

/// record a "when" action step.
#[macro_export]
macro_rules! when {
    ($scenario:expr, $label:expr) => {
        $scenario.step("WHEN", $label);
    };
    ($scenario:expr, $label:expr, $body:block) => {{
        $scenario.step("WHEN", $label);
        $body
    }};
}

/// record a "then" assertion step. assertions inside the body run inline.
#[macro_export]
macro_rules! then {
    ($scenario:expr, $label:expr, $body:block) => {{
        $scenario.step("THEN", $label);
        $body
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_display_includes_all_three_labels() {
        let s = Scenario::new("core", "State machine transitions", "User message starts a turn");
        assert_eq!(s.to_string(), "core / State machine transitions / User message starts a turn");
    }

    #[test]
    fn steps_are_recorded_in_order() {
        let mut s = Scenario::new("core", "r", "n");
        s.step("GIVEN", "g");
        s.step("WHEN", "w");
        s.step("THEN", "t");
        assert_eq!(s.steps, vec!["g", "w", "t"]);
    }
}