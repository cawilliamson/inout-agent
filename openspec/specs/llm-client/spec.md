# LLM Client Specification

## Purpose

The provider trait, streaming, prompt caching, retry, and clients (replay + http). This extends v1.0 `LlmClient` trait with observability hooks + raw-stream capture + retry/caching.

## Requirements

### Requirement: Core builds without HTTP provider

`inout-core` SHALL compile and pass clippy without depending on `inout-ext-http-provider`. All HTTP-provider-specific types, including `AnthropicClient` and `OpenAiClient`, SHALL live in the extension crate.

#### Scenario: Core-only build compiles

- GIVEN a build of `inout-core` with `inout-ext-http-provider` excluded
- WHEN `cargo clippy --all-targets --no-default-features -- -D warnings` runs
- THEN the build succeeds with no warnings

#### Scenario: Replay client remains in core

- GIVEN `ReplayLlmClient` is defined in `inout-core`
- WHEN `inout-ext-http-provider` is not built
- THEN `ReplayLlmClient` still implements `LlmProvider`

### Requirement: LlmProvider trait

The system SHALL provide a streaming-first `LlmProvider` trait that is `Send + Sync`. It SHALL accept a system prompt, a message history, a list of tool definitions, an optional `BeforeOutput` callback, and an optional thinking budget.

#### Scenario: Trait compiles for multiple clients

- GIVEN the `LlmProvider` trait definition
- WHEN `ReplayLlmClient`, `AnthropicClient`, and `OpenAiClient` implement it
- THEN all three implementations compile

#### Scenario: Trait is Send + Sync

- GIVEN a value of type `Box<dyn LlmProvider>`
- WHEN it is moved into an async task
- THEN the compiler accepts it as `Send + Sync`

### Requirement: ContentBlock enum

The system SHALL provide a `ContentBlock` enum with variants for text, tool use, tool result, and image content.

#### Scenario: Serialize text block

- GIVEN a `ContentBlock::Text { text: "hello" }`
- WHEN it is serialized to JSON
- THEN the output is `{ "type": "text", "text": "hello" }`

#### Scenario: Serialize tool use block

- GIVEN a `ContentBlock::ToolUse { id: "1", name: "bash", input: {} }`
- WHEN it is serialized to JSON
- THEN the output contains `"type": "tool_use"`, `"id": "1"`, `"name": "bash"`, and `"input": {}`

#### Scenario: Serialize tool result block

- GIVEN a `ContentBlock::ToolResult { tool_use_id: "1", content: "ok" }`
- WHEN it is serialized to JSON
- THEN the output contains `"type": "tool_result"` and `"tool_use_id": "1"`

#### Scenario: Serialize image block

- GIVEN a `ContentBlock::Image { media_type: "image/png", data: [0u8, 1, 2] }`
- WHEN it is serialized to JSON
- THEN the output contains `"type": "image"` and `"media_type": "image/png"`

### Requirement: Message struct

The system SHALL provide a `Message` struct with a role string and a vector of `ContentBlock` values.

#### Scenario: Construct user message

- GIVEN role `"user"` and content `[ContentBlock::Text { text: "hi" }]`
- WHEN a `Message` is constructed
- THEN the role is `"user"` and the content contains one text block

#### Scenario: Construct assistant message with tool use

- GIVEN role `"assistant"` and content `[ContentBlock::ToolUse { id: "1", name: "bash", input: {} }]`
- WHEN a `Message` is constructed
- THEN the role is `"assistant"` and the content contains one tool-use block

### Requirement: Usage metrics

The system SHALL provide a `Usage` struct with input tokens, output tokens, cache-read tokens, and cache-write tokens. All fields default to zero.

#### Scenario: Default usage is zero

- GIVEN a default `Usage` value
- WHEN its fields are read
- THEN `input_tokens`, `output_tokens`, `cache_read_tokens`, and `cache_write_tokens` are all `0`

#### Scenario: Aggregate usage across turns

- GIVEN two `Usage` values with non-zero fields
- WHEN they are added
- THEN each field in the result equals the sum of the corresponding input fields

### Requirement: ApiResponse

The system SHALL provide an `ApiResponse` struct containing a vector of `ContentBlock` values, optional `Usage`, and an optional stop reason.

#### Scenario: Response with text and usage

- GIVEN an `ApiResponse` with content `[ContentBlock::Text { text: "ok" }]`, usage `Usage { input_tokens: 10, output_tokens: 5, ..Default::default() }`, and stop reason `"end_turn"`
- WHEN the response is inspected
- THEN the content, usage, and stop reason match the provided values

#### Scenario: Response without usage

- GIVEN an `ApiResponse` with content and no usage
- WHEN the response is inspected
- THEN `usage` is `None`

### Requirement: BeforeOutput callback

The system SHALL provide a `BeforeOutput` callback type. The provider SHALL invoke it exactly once on the first stream chunk, before any output content is delivered.

#### Scenario: Callback fires on first chunk

- GIVEN a provider configured with a `BeforeOutput` callback that records invocation
- WHEN the first stream chunk arrives
- THEN the callback is invoked exactly once

#### Scenario: Callback does not fire without chunks

- GIVEN a provider configured with a `BeforeOutput` callback
- WHEN the stream completes with no chunks
- THEN the callback is never invoked

### Requirement: ReplayLlmClient

`ReplayLlmClient` SHALL store a queue of pre-recorded `ApiResponse` values and return them in order, ignoring all inputs. It SHALL be deterministic and require no network access.

#### Scenario: Return pre-recorded responses

- GIVEN a `ReplayLlmClient` seeded with two `ApiResponse` values
- WHEN `send` is called twice with different inputs
- THEN the first call returns the first response and the second call returns the second response

#### Scenario: Empty queue returns error

- GIVEN a `ReplayLlmClient` with no responses
- WHEN `send` is called
- THEN the result is an error indicating no recorded response is available

### Requirement: HttpLlmClient

`HttpLlmClient` SHALL support Anthropic and OpenAI-compatible providers. OpenAI-compatible mode SHALL support Azure, Groq, DeepSeek, Together, OpenRouter, Mistral, xAI, Perplexity, and local servers such as LM Studio or Ollama via a configurable `base_url`.

#### Scenario: Anthropic client sends request

- GIVEN an `AnthropicClient` with a valid configuration
- WHEN `send` is called
- THEN an HTTP request is sent to the configured Anthropic base URL
- AND the response is parsed into an `ApiResponse`

#### Scenario: OpenAI-compatible client sends request

- GIVEN an `OpenAiClient` configured with a non-Anthropic `base_url`
- WHEN `send` is called
- THEN an HTTP request is sent to that `base_url`
- AND the response is parsed using OpenAI-compatible message format

#### Scenario: Local server via base_url

- GIVEN an `OpenAiClient` configured with `base_url` pointing to `http://localhost:1234/v1`
- WHEN `send` is called
- THEN the request targets the local server
- AND the response is parsed as OpenAI-compatible

### Requirement: SSE event parsing
The system SHALL parse OpenAI-compatible Server-Sent Events into a stream of delta events: content deltas, reasoning deltas, tool-call deltas, and a terminal `Done` marker. A single delta chunk carrying both `reasoning` and `content` SHALL produce both events in order.

#### Scenario: Done marker terminates the stream
- GIVEN a `data: [DONE]` SSE line
- WHEN `parse_sse_event` is called
- THEN a single `Done` event is produced

#### Scenario: Content delta is parsed
- GIVEN a delta chunk whose `delta.content` is `hi`
- WHEN `parse_sse_event` is called
- THEN a single delta-content event carries the text `hi`

#### Scenario: Reasoning and content on the same chunk
- GIVEN a delta chunk whose `delta` carries both `reasoning` and `content`
- WHEN `parse_sse_event` is called
- THEN a reasoning event and a content event are produced in order

#### Scenario: Tool-call delta is parsed
- GIVEN a delta chunk whose `delta.tool_calls` carries index, id, function name, and arguments
- WHEN `parse_sse_event` is called
- THEN a single delta-tool event carries the index, id, name, and arguments

### Requirement: send_with_retry

The system SHALL provide a `send_with_retry` helper that retries HTTP stream drops with exponential backoff, up to a maximum of four retries, with delays of 3 seconds, 6 seconds, 12 seconds, and 24 seconds.

#### Scenario: Retry on stream drop

- GIVEN a request that fails with an `sse stream error` on the first attempt and succeeds on the second
- WHEN `send_with_retry` is called
- THEN the request is retried once
- AND the call succeeds

#### Scenario: Backoff delays follow sequence

- GIVEN a request that fails on every attempt
- WHEN `send_with_retry` is called
- THEN retries occur after 3s, 6s, 12s, and 24s
- AND the final attempt returns an error

#### Scenario: Retry on recoverable errors

- GIVEN a request that fails with `connection reset`, `connection closed`, `broken pipe`, or `incomplete message`
- WHEN `send_with_retry` is called
- THEN the request is retried up to four times

### Requirement: Anthropic prompt caching

The Anthropic client SHALL mark the system prompt, tool definitions, and the final user message with `cache_control` breakpoints so that Anthropic can cache and reuse prompt context.

#### Scenario: System prompt breakpoint

- GIVEN an `AnthropicClient` configured with a system prompt
- WHEN a request is built
- THEN the system prompt includes `cache_control: { "type": "ephemeral" }`

#### Scenario: Tool definitions breakpoint

- GIVEN an `AnthropicClient` configured with tool definitions
- WHEN a request is built
- THEN each tool definition includes `cache_control: { "type": "ephemeral" }`

#### Scenario: Final message breakpoint

- GIVEN an `AnthropicClient` configured with a message history
- WHEN a request is built
- THEN the last message includes `cache_control: { "type": "ephemeral" }`

### Requirement: before_provider_payload hook

The system SHALL invoke the `before_provider_payload` observability hook from v2.1 with the raw provider payload before sending the request over HTTP.

#### Scenario: Hook receives raw payload

- GIVEN an observer registered on `before_provider_payload`
- WHEN `AnthropicClient::send` is called
- THEN the observer receives the serialized JSON request body
- AND the body contains the system prompt and messages

#### Scenario: Hook does not block send

- GIVEN an observer on `before_provider_payload` that panics
- WHEN `send` is called
- THEN the send still proceeds
- AND the panic is caught and logged

### Requirement: trace_span on provider calls

The system SHALL wrap every provider call in a `trace_span` from v2.1 observability, recording the provider name and model.

#### Scenario: Span is created for send

- GIVEN tracing is enabled
- WHEN `LlmProvider::send` is called on any client
- THEN a span named `llm_provider_send` is entered
- AND the span contains `provider` and `model` fields

#### Scenario: Span follows async boundaries

- GIVEN a traced provider call that awaits an HTTP response
- WHEN the future yields and resumes
- THEN the span remains active across await points

### Requirement: Per-task model routing

The system SHALL support per-task model routing via a `model_routes` configuration. It SHALL classify a prompt as one of `coding`, `review`, `explain`, or `search`, swap the model for that single turn, and restore the original model afterward.

#### Scenario: Classify coding prompt

- GIVEN the prompt `"refactor this rust function"`
- WHEN `route_for_turn` is called
- THEN the classification is `coding`
- AND the model from `model_routes.coding` is used for the turn

#### Scenario: Classify review prompt

- GIVEN the prompt `"review this pull request"`
- WHEN `route_for_turn` is called
- THEN the classification is `review`
- AND the model from `model_routes.review` is used for the turn

#### Scenario: Model is restored after turn

- GIVEN a `RoutingSwap` that replaced the active model
- WHEN `restore_routing` is called after the turn completes
- THEN the original model is restored
- AND subsequent turns use the original model

#### Scenario: Unknown prompt uses default model

- GIVEN a prompt that does not match any classification
- WHEN `route_for_turn` is called
- THEN no routing swap occurs
- AND the default model is used

### Requirement: Token redaction

The system SHALL redact API tokens in all logs. Full API keys SHALL never appear in log output or error messages.

#### Scenario: Redact token in log

- GIVEN an API key `"sk-abcdef1234567890"`
- WHEN `redact_token` is applied before logging
- THEN the logged value is not `"sk-abcdef1234567890"`
- AND the logged value is a recognizable redacted form

#### Scenario: Config with key is safe to debug

- GIVEN a configuration struct containing an API key
- WHEN it is formatted with `Debug`
- THEN the API key field shows the redacted form

### Requirement: build_curl_block

The system SHALL provide a `build_curl_block` helper that constructs a `curl` command from a provider request for raw-stream logging and manual replay.

#### Scenario: Generate curl from request

- GIVEN a request builder with a URL, headers including an API key, and a JSON body
- WHEN `build_curl_block` is called
- THEN the output starts with `curl`
- AND the output contains the URL
- AND the output contains `-H` entries for the headers
- AND the output contains `--data` or `--data-binary` with the body

#### Scenario: Curl block redacts token

- GIVEN a request builder with an API key header
- WHEN `build_curl_block` is called
- THEN the output does not contain the full API key

### Requirement: Clippy green with all features

The full workspace SHALL pass clippy with all features enabled.

#### Scenario: Run all-features clippy

- GIVEN the workspace with all default and optional features enabled
- WHEN `cargo clippy --all-targets --all-features -- -D warnings` runs
- THEN the command exits with status `0`

### Requirement: Clippy green with no default features

The workspace SHALL pass clippy with no default features, exercising the replay-only path without the HTTP provider.

#### Scenario: Run no-default-features clippy

- GIVEN the workspace with default features disabled
- WHEN `cargo clippy --all-targets --no-default-features -- -D warnings` runs
- THEN the command exits with status `0`
- AND `inout-ext-http-provider` is not built
