# Proposal: TUI visual redesign

## Intent

Iterate on the TUI appearance — iMessage-style chat bubbles, thinking indicators, per-message boxes with role labels, model name display, explicit widget colours, and border fixes.

## Scope

### In scope

- Chat bubble rendering
- Role-coloured message boxes
- Thinking / reasoning indicator with spinner
- Model label in footer
- Input area styling
- Border width consistency
- Lib / bin split for testability

### Out of scope

- New features beyond visual / structural changes

## Approach

Per-message boxed rendering with left-border accent colour per role. Spinner for thinking state. Footer bar with model name, context percentage, cost. Split `lib.rs` from `main.rs` for test access.
