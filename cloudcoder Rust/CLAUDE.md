# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo build                    # Build entire workspace
cargo build -p cloudcoder-cli  # Build a specific crate
cargo test                     # Run all tests across workspace
cargo test -p cloudcoder-core  # Run tests for a specific crate
cargo test test_rate_limiting  # Run a single test by name
cargo test -p cloudcoder-services -- test_parallel  # Run tests matching pattern in specific crate
cargo run                      # Run the CLI (defaults to interactive chat)
cargo run -- tool --name BashTool  # Run with CLI subcommand
```

The `network` feature flag on `cloudcoder-services` enables `reqwest`/`sha2` dependencies — build with `cargo build -p cloudcoder-services --features network` if needed.

## Architecture

Workspace with 4 crates following a layered dependency graph:

```
cloudcoder-core (no dependencies)
    ↑
cloudcoder-services → cloudcoder-core
cloudcoder-provider → cloudcoder-core
    ↑
cloudcoder-cli → cloudcoder-services, cloudcoder-core
```

**cloudcoder-core** — Zero-dependency foundation with shared types and traits:
- `CloudCoderError` / `ServiceError` — error types with `Clone` support (source is dropped on clone)
- `EventType` + `IEventBus` trait — pub/sub event system (payload is `String` for JSON serialization)
- `Service` trait — lifecycle interface (`initialize`/`dispose`/`health_check`) returning `Pin<Box<dyn Future>>`
- `CacheOptions`, `CacheStats`, `HealthStatus`, `ToolPermissionBehavior`, `LazyLoader<T>` type alias

**cloudcoder-services** — Infrastructure services depending only on core:
- `CacheService` — tiered memory+disk cache using bincode serialization, implements `Service` trait
- `EventBus` — concrete `IEventBus` impl with `RwLock`-based subscriber management and panic-isolated handlers
- `LazyRegistry<T>` — deferred-loading registry with concurrent load deduplication via `tokio::sync::Notify`
- `RateLimiter` — synchronous sliding-window rate limiter (not async, uses `Instant` directly)
- `ParallelExecutor` — task scheduler with dependency graph resolution, priority ordering, semaphore-bounded concurrency, and cascade-on-fail semantics
- `VectorStore` — in-memory vector store with cosine/euclidean/dot similarity, optional JSON persistence
- `PluginRegistry` — plugin lifecycle (discover→load→activate) with sandbox, manifest validation, and dependency cycle detection
- `Telemetry` / `MetricsRegistry` — atomic counters, gauges, histograms (all `AtomicU64`-based, lock-free reads)

**cloudcoder-provider** — LLM provider abstraction:
- `Provider` trait — async interface: `complete`, `chat`, `chat_stream`, `count_tokens`
- `OllamaProvider` — implements `Provider` with cloud (`ollama.com`) and local (`localhost:11434`) modes
- `ChatMessage` / `MessageContent` — multimodal messages supporting text, images, tool_use/tool_result content blocks
- Cloud auth handled by `ollama login` (no API key in config); models with `:cloud`/`-cloud` suffix use cloud mode

**cloudcoder-cli** — Binary crate (`cloudcoder`) with clap-based CLI:
- Subcommands: `chat`, `tool`, `tools`, `version`, `plugin`, `config`
- `ToolRegistry` — async registry for `Tool` trait implementations
- Built-in tools: `BashTool`, `FileTool`, `GitTool`, `HttpTool`

## Key Patterns

- All shared state uses `tokio::sync::RwLock` (not `std::sync`) — services are async-native
- Services implement the `Service` trait from core with `Pin<Box<dyn Future>>` return types for lifecycle methods
- `CloudCoderError` is `Clone` by dropping the error source chain — useful for reporting but loses cause info on clone
- Plugin manifests are `cloudcoder-plugin.json` files in plugin directories, validated before loading
- The CLI is a placeholder REPL — LLM integration via the provider layer is not yet wired into the chat loop

## CLI Usage

```bash
cargo run                    # Start interactive chat session
cargo run -- tools           # List available tools
cargo run -- version         # Show version info
```

Chat commands:
- `/model <name>` - Switch to a different model
- `/models` - List all available models (cloud models marked with ☁️)
- `/clear` - Clear conversation history
- `/system <prompt>` - Set system prompt
- `/help` - Show available commands
- `/exit` - Quit the session