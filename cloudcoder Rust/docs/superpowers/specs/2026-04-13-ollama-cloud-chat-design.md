# Ollama Cloud Chat Integration

## Problem

The CLI has a placeholder REPL loop that doesn't connect to any LLM. The `OllamaProvider` cloud mode incorrectly points to `ollama.com` directly instead of routing through the local Ollama server. We need a working chat session that uses Ollama cloud models with tool execution support.

## Decision: Local Proxy Architecture

All API calls go through the local Ollama server at `localhost:11434`. The server handles:
- Auth to ollama.com (via `ollama signin`)
- Routing cloud models (identified by `:cloud` suffix) to the cloud
- Local model inference for non-cloud models

This means CloudCoder does NOT need to implement Ollama auth — we just use the standard Ollama API.

## Component Changes

### 1. OllamaProvider fixes (cloudcoder-provider)

**Current state:** `OllamaProvider::cloud()` points to `https://ollama.com`, which returns "unauthorized" without auth headers. The `OllamaMode` enum distinguishes Cloud vs Local.

**Changes:**
- Default URL: `http://localhost:11434` for all modes
- Remove `OllamaMode` (Cloud/Local) — the server routes based on model name, not URL
- `OllamaProvider::new()` → creates provider pointing at `localhost:11434`
- `OllamaProvider::cloud()` and `OllamaProvider::local()` → both point to `localhost:11434` (kept for API compat but deprecated)
- Default model: read from `~/.ollama/config.json` `last_selection` integration, or `glm-5.1:cloud`
- Add `thinking` field to `OllamaMessageResponse` and `OllamaStreamChunk` — reasoning models (glm-5.1, deepseek, etc.) return thinking tokens in a separate field
- Add `tool_calls` to `OllamaMessageResponse` — Ollama returns tool calls as `tool_calls[].{id, function.{name, arguments}}` (not ContentBlock::ToolUse like Claude)
- Fix test: `base_url().contains("api.ollama.cloud")` → assert localhost:11434
- When converting Ollama response to `ChatMessage`: map `tool_calls` → `ContentBlock::ToolUse { id, name, input }`
- `is_cloud_model()` helper still valid — used to identify cloud models by `:cloud`/`-cloud` suffix for display purposes

### 2. ChatSession (cloudcoder-cli, new file: `src/chat.rs`)

**New struct** that replaces the placeholder REPL:

```rust
struct ChatSession {
    provider: OllamaProvider,
    messages: Vec<ChatMessage>,
    model: String,
    tool_registry: ToolRegistry,
}
```

**Message loop:**
1. Read user input
2. Construct `ChatMessage::user(input)`, append to `messages`
3. Build `ChatRequest { model, messages, stream: true }`
4. Call `provider.chat_stream(request)`
5. Print tokens incrementally:
   - `thinking` content → dim gray
   - `content` content → normal
6. Collect the full response message
7. If response contains `ToolUse` content blocks:
   - For each tool call: parse name+input, execute via ToolRegistry, construct `ChatMessage::tool_result`
   - Append assistant message + tool results to conversation
   - Go to step 3 (auto-continue, no user input needed)
8. If no tool calls: append assistant message, display to user, go to step 1

**Model switching via /model:**
- `/model` with no args: list available models from `/api/tags`, highlight current
- `/model <name>`: switch current model, confirm switch

**Other commands:**
- `/models` — list all available models (cloud and local)
- `/clear` — clear conversation history
- `/system <prompt>` — set/replace system prompt
- `/exit` — quit

**Startup behavior:**
1. Check Ollama server availability (`GET /api/tags`)
2. If unreachable: print "Ollama server not found at localhost:11434. Run `ollama serve` first." and exit
3. If reachable: load default model from `~/.ollama/config.json` or use `glm-5.1:cloud`
4. Print welcome banner with current model info
5. Enter message loop

### 3. Tool integration in chat

The tool call flow enables agentic behavior:

```
User: "Create a file called hello.txt with 'hello world'"

→ Provider returns ToolUse { id: "call_1", name: "FileTool", input: { action: "write", path: "hello.txt", content: "hello world" } }
→ ChatSession executes FileTool.write()
→ Constructs ChatMessage::tool_result("call_1", "File written successfully", false)
→ Appends to messages, re-submits to provider
→ Provider returns text: "I've created hello.txt with the content 'hello world'."
→ Displayed to user
```

**Ollama tool call format:**
Ollama returns tool calls as `tool_calls[].{id, function.{name, arguments}}` in the message object, NOT as ContentBlock::ToolUse. ChatSession maps these to our internal types:
- `tool_calls[i].id` → `ToolUse.id`
- `tool_calls[i].function.name` → `ToolUse.name`
- `tool_calls[i].function.arguments` → `ToolUse.input` (JSON value)

Tool results are sent back as messages with `role: "tool"` and the tool call ID.

**Tool execution details:**
- Parse `tool_calls` from the Ollama response message
- Each tool gets its own result message with `role: "tool"`
- Error results: include error message in content
- Multiple tool calls in one response: execute sequentially (simpler, avoids race conditions)
- Maximum tool call rounds: 10 (prevent infinite loops)

### 4. CLI argument changes (cloudcoder-cli/src/main.rs)

**Remove:**
- `Chat { system: Option<String> }` → `system` flag removed (use `/system` command instead)
- `Chat { --model }` flag removed (use `/model` command)

**Add:**
- `Chat` subcommand with no required flags

**Keep:**
- `Tool`, `Tools`, `Version`, `Plugin`, `Config` subcommands unchanged
- `--verbose`, `--no-color` global flags unchanged

### 5. Config integration

Read `~/.ollama/config.json` at startup to get:
- `last_selection` — which integration was last used (e.g., "claude")
- `integrations[last_selection].models[0]` — default model for that integration

This provides a sensible default without requiring user configuration.

## Error Handling

- Ollama server unreachable → clear error + exit
- Model not found → suggest `/models` to list available, keep current model
- Tool execution failure → return error as tool result, let model decide how to handle
- Stream disconnect → display partial response, let user retry
- Auth failure (cloud model) → suggest running `ollama signin`

## Testing

- Unit tests: `ChatSession` message construction, tool result parsing, model switching logic
- Integration tests (requires Ollama running): streaming chat, tool execution round-trip
- Manual testing: full chat session with cloud model, verify thinking tokens render, verify /model switching