# Cloud Coder Rust - Architecture Breakdown

This directory contains the planned Rust rewrite of Cloud Coder, based on the architecture improvements plan.

## Project Structure

```
cloudcoder-rust/
├── Cargo.toml                    # Root workspace configuration
├── Cargo.lock
├── rust-toolchain.toml           # Rust version pinning
├── .cargo/config.toml            # Cargo configuration
├── README.md
├── LICENSE
├── deny.toml                     # Cargo-deny configuration
│
├── crates/
│   ├── cloudcoder/               # Main CLI binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs           # Entry point
│   │       ├── cli.rs            # CLI argument parsing
│   │       └── repl.rs           # REPL loop
│   │
│   ├── cloudcoder-core/          # Core types and traits (NO external deps)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types.rs          # Core types (Tool, Event, Service traits)
│   │       ├── error.rs          # Error types
│   │       ├── result.rs         # Result type
│   │       └── event.rs          # Event bus types
│   │
│   ├── cloudcoder-tools/         # Tool implementations
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── registry.rs       # Lazy tool registry
│   │       ├── bash.rs           # BashTool
│   │       ├── file_read.rs      # FileReadTool
│   │       ├── file_edit.rs      # FileEditTool
│   │       ├── file_write.rs     # FileWriteTool
│   │       ├── glob.rs           # GlobTool
│   │       └── grep.rs           # GrepTool
│   │
│   ├── cloudcoder-services/      # Service implementations
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── cache/            # Caching service
│   │       │   ├── mod.rs
│   │       │   ├── memory.rs     # LRU memory cache
│   │       │   ├── disk.rs       # SQLite disk cache
│   │       │   └── service.rs    # Unified cache service
│   │       ├── event_bus.rs      # Event bus implementation
│   │       ├── telemetry.rs      # Telemetry service (opt-in)
│   │       └── health.rs         # Health monitoring
│   │
│   ├── cloudcoder-providers/     # LLM provider implementations
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── trait.rs          # Provider trait
│   │       ├── anthropic.rs      # Anthropic provider
│   │       ├── openai.rs         # OpenAI provider
│   │       ├── gemini.rs         # Gemini provider
│   │       └── ollama.rs         # Ollama provider
│   │
│   ├── cloudcoder-api/           # API client layer
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── client.rs         # HTTP client
│   │       ├── streaming.rs      # Streaming response handling
│   │       └── retry.rs          # Retry logic
│   │
│   ├── cloudcoder-permissions/   # Permission system
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── rules.rs          # Permission rules
│   │       ├── checker.rs        # Permission checker
│   │       └── classifier.rs     # Auto-mode classifier
│   │
│   ├── cloudcoder-ui/            # Terminal UI (using Ratatui)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── app.rs            # App state
│   │       ├── components/       # UI components
│   │       │   ├── mod.rs
│   │       │   ├── transcript.rs
│   │       │   ├── input.rs
│   │       │   └── status.rs
│   │       └── theme.rs          # Theme support
│   │
│   ├── cloudcoder-mcp/           # MCP protocol implementation
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── client.rs         # MCP client
│   │       ├── transport/        # Transports
│   │       │   ├── mod.rs
│   │       │   ├── stdio.rs
│   │       │   └── sse.rs
│   │       └── oauth.rs          # OAuth handling
│   │
│   └── cloudcoder-ffi/           # FFI bindings for TypeScript interop
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           └── bindings.rs
│
├── bindings/                     # Generated bindings
│   ├── typescript/               # TypeScript bindings
│   └── python/                   # Python bindings (future)
│
├── scripts/
│   ├── build.rs                  # Build script
│   ├── generate-bindings.sh
│   └── release.sh
│
└── tests/
    ├── integration/              # Integration tests
    └── fixtures/                 # Test fixtures
```

## Phase 1: Foundation (Rust Implementation)

### Week 1-2: Core Types and Event Bus

**crates/cloudcoder-core/src/types.rs**
```rust
/// Core types shared across all modules.
/// This module has NO external dependencies to prevent coupling.

use uuid::Uuid;
use std::collections::HashMap;
use std::time::SystemTime;

// ============================================================================
// Tool Types
// ============================================================================

pub type ToolName = String;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPermissionBehavior {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone)]
pub struct ToolPermissionResult {
    pub behavior: ToolPermissionBehavior,
    pub updated_input: Option<serde_json::Value>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ToolProgressData {
    pub r#type: String,
    pub data: serde_json::Value,
}

// ============================================================================
// Event Types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    ToolCallStart,
    ToolCallComplete,
    ToolCallError,
    PermissionCheck,
    ApiRequestStart,
    ApiRequestComplete,
    ContextCompactStart,
    ContextCompactComplete,
    SessionStart,
    SessionEnd,
    SettingsChange,
}

#[derive(Debug, Clone)]
pub struct CloudCoderEvent<T = serde_json::Value> {
    pub r#type: EventType,
    pub payload: T,
    pub timestamp: SystemTime,
    pub source: String,
}

pub trait EventHandler<T>: Fn(&CloudCoderEvent<T>) + Send + Sync {}

// ============================================================================
// Service Types
// ============================================================================

#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub healthy: bool,
    pub checks: HashMap<String, HealthCheck>,
    pub last_check: SystemTime,
}

#[derive(Debug, Clone)]
pub struct HealthCheck {
    pub ok: bool,
    pub message: Option<String>,
}

#[async_trait::async_trait]
pub trait Service: Send + Sync {
    fn name(&self) -> &'static str;
    async fn initialize(&mut self) -> Result<(), ServiceError>;
    async fn dispose(&mut self) -> Result<(), ServiceError>;
    async fn health_check(&self) -> Result<HealthStatus, ServiceError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("Service initialization failed: {0}")]
    Initialization(String),
    #[error("Service health check failed: {0}")]
    HealthCheck(String),
}

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum CloudCoderError {
    #[error("Tool execution failed: {tool_name} - {message}")]
    ToolExecution {
        message: String,
        tool_name: String,
        tool_input: Option<serde_json::Value>,
    },

    #[error("Permission denied: {tool_name} - {reason}")]
    PermissionDenied {
        tool_name: String,
        reason: Option<String>,
    },

    #[error("API error: {0}")]
    Api(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ============================================================================
// Result Types
// ============================================================================

pub type Result<T, E = CloudCoderError> = std::result::Result<T, E>;

pub fn ok<T>(value: T) -> Result<T, Never> {
    Ok(value)
}

pub fn err<E, T>(error: E) -> Result<T, E> {
    Err(error)
}

/// Never type for impossible errors
#[derive(Debug, Clone, Copy)]
pub enum Never {}

impl std::fmt::Display for Never {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unreachable!()
    }
}

impl std::error::Error for Never {}
```

### Week 3-4: Lazy Loading and Tool Registry

**crates/cloudcoder-tools/src/registry.rs**
```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::Tool;

type ToolLoader = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Result<Arc<dyn Tool>>>> + Send>;

struct LazyEntry {
    loader: Option<ToolLoader>,
    loaded: bool,
    value: Option<Arc<dyn Tool>>,
    error: Option<anyhow::Error>,
    pending: Option<Arc<tokio::sync::Notify>>,
}

pub struct LazyToolRegistry {
    entries: RwLock<HashMap<String, LazyEntry>>,
}

impl LazyToolRegistry {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    pub fn register<F, Fut>(&mut self, name: String, loader: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<Arc<dyn Tool>>> + Send + 'static,
    {
        let mut entries = self.entries.blocking_write();
        entries.insert(name, LazyEntry {
            loader: Some(Box::new(move || Box::pin(loader()))),
            loaded: false,
            value: None,
            error: None,
            pending: Some(Arc::new(tokio::sync::Notify::new())),
        });
    }

    pub async fn get(&self, name: &str) -> Result<Option<Arc<dyn Tool>>> {
        // Check if already loaded
        {
            let entries = self.entries.read().await;
            if let Some(entry) = entries.get(name) {
                if entry.loaded {
                    return Ok(entry.value.clone());
                }
                if entry.pending.is_some() {
                    // Wait for pending load
                    drop(entries);
                    self.wait_for_load(name).await?;
                    let entries = self.entries.read().await;
                    return Ok(entries.get(name).and_then(|e| e.value.clone()));
                }
            }
        }

        // Start loading
        self.load(name).await
    }

    async fn load(&self, name: &str) -> Result<Option<Arc<dyn Tool>>> {
        let mut entries = self.entries.write().await;
        let entry = entries.get_mut(name).ok_or_else(|| {
            anyhow::anyhow!("Tool not registered: {}", name)
        })?;

        let loader = entry.loader.take()?;
        let notify = entry.pending.clone().unwrap();

        // Load the tool
        match loader().await {
            Ok(tool) => {
                entry.loaded = true;
                entry.value = Some(tool.clone());
                notify.notify_all();
                Ok(Some(tool))
            }
            Err(e) => {
                entry.error = Some(e.clone());
                notify.notify_all();
                Err(e)
            }
        }
    }

    async fn wait_for_load(&self, name: &str) -> Result<()> {
        let notify = {
            let entries = self.entries.read().await;
            entries.get(name)
                .and_then(|e| e.pending.clone())
                .ok_or_else(|| anyhow::anyhow!("Tool not registered: {}", name))?
        };

        notify.notified().await;
        Ok(())
    }
}
```

## Cargo.toml (Root Workspace)

```toml
[workspace]
resolver = "2"
members = [
    "crates/cloudcoder",
    "crates/cloudcoder-core",
    "crates/cloudcoder-tools",
    "crates/cloudcoder-services",
    "crates/cloudcoder-providers",
    "crates/cloudcoder-api",
    "crates/cloudcoder-permissions",
    "crates/cloudcoder-ui",
    "crates/cloudcoder-mcp",
    "crates/cloudcoder-ffi",
]

[workspace.package]
version = "0.2.0"
edition = "2021"
license = "MIT"
repository = "https://github.com/martin-raven/cloudcoder"
rust-version = "1.75"

[workspace.dependencies]
# Async
tokio = { version = "1.35", features = ["full"] }
async-trait = "0.1"
futures = "0.3"

# Error handling
anyhow = "1.0"
thiserror = "1.0"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# HTTP client
reqwest = { version = "0.11", features = ["json", "stream"] }

# SQLite
rusqlite = { version = "0.30", features = ["bundled"] }

# Terminal UI
ratatui = "0.25"
crossterm = "0.27"

# Utilities
uuid = { version = "1.6", features = ["v4", "serde"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# FFI
cxx = "1.0"
neon = "0.10"

[profile.release]
lto = true
codegen-units = 1
strip = true
```

## Key Differences from TypeScript Version

| Aspect | TypeScript | Rust |
|--------|-----------|------|
| Startup | ~500ms | <100ms (expected) |
| Memory | ~200MB | <50MB (expected) |
| Type Safety | Runtime + TS checks | Compile-time guarantees |
| Concurrency | Event loop | Async + threads |
| Error Handling | Try/catch | Result<T, E> |
| Tool Registry | Dynamic imports | Lazy static + async |
| Cache | bun:sqlite | rusqlite (bundled) |
| UI | Ink/React | Ratatui |

## Next Steps

1. **Initialize Rust project:**
   ```bash
   cd "cloudcoder Rust"
   cargo init --name cloudcoder-rust
   ```

2. **Create workspace structure:**
   ```bash
   mkdir -p crates/{cloudcoder-core,cloudcoder-tools,cloudcoder-services}
   ```

3. **Start with cloudcoder-core:**
   - Implement types.rs
   - Implement error.rs
   - Implement event.rs (event bus)

4. **Build incrementally:**
   - Each crate compiles independently
   - Integration tests verify cross-crate behavior
   - Benchmark against TypeScript version
