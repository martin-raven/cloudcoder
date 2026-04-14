# Automatic Coordinator Detection Design

## Problem

The current CloudCoder chat session only supports single-agent mode. The coordinator architecture exists (worker spawning, notification parsing, registry) but is not integrated into the chat loop. We need the LLM to automatically detect when tasks warrant parallel worker coordination and orchestrate them seamlessly.

## Decision: LLM-Driven Automatic Coordinator Mode

CloudCoder will expose coordinator tools (`agent`, `send_message`, `task_stop`) in every chat session. A detailed system prompt explains the LLM's coordinator role. The LLM decides when to spawn workers based on task complexity. A background listener injects worker notifications into the chat loop, and the LLM decides per-notification how to proceed.

## Architecture

### Components

**Coordinator Tools** (new file: `crates/cloudcoder-cli/src/tools/coordinator.rs`):
- `AgentTool` — Spawns a new worker subprocess, returns worker ID
- `SendMessageTool` — Sends follow-up instructions to existing worker
- `TaskStopTool` — Kills a running worker

All tools implement the existing `Tool` trait and register in `ToolRegistry`.

**Worker Listener** (new file: `crates/cloudcoder-cli/src/coordinator/listener.rs`):
- Background task monitoring all running workers
- Parses XML notifications from worker stdout as they arrive
- Broadcasts `TaskNotification` through channel to chat loop

**Worker Registry** (modify: `crates/cloudcoder-cli/src/coordinator/registry.rs`):
- Add `notification_tx: mpsc::Sender<TaskNotification>`
- Track notification channel for broadcasting worker results

**ChatSession** (modify: `crates/cloudcoder-cli/src/chat.rs`):
- Add `worker_registry: Arc<WorkerRegistry>`
- Add `notification_rx: mpsc::Receiver<TaskNotification>`
- Spawn worker listener at startup
- Inject notifications as synthetic user messages between turns

### Message Flow

```
User: "Research auth bugs in src/ and src/auth/ in parallel"
    ↓
ChatSession builds request with coordinator system prompt
    ↓
LLM calls AgentTool twice with different research tasks
    ↓
AgentTool spawns workers:
    worker-1: "Research auth bugs in src/"
    worker-2: "Research auth bugs in src/auth/"
    ↓
Workers execute in parallel subprocesses
    ↓
Worker listener catches XML notifications as workers complete
    ↓
Notifications injected as synthetic user messages:
    <task-notification>
      <task-id>worker-1</task-id>
      <status>completed</status>
      ...
    </task-notification>
    ↓
LLM sees notifications, decides to synthesize for user
    ↓
LLM responds to user with combined findings
```

## Component Design

### 1. Coordinator Tools (`tools/coordinator.rs`)

```rust
use crate::coordinator::{WorkerConfig, WorkerRegistry, spawn_worker, kill_worker};
use crate::tools::{Tool, ToolDefinition, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Tool for spawning new worker subprocesses
pub struct AgentTool {
    registry: Arc<WorkerRegistry>,
}

/// Tool for sending follow-up messages to existing workers
pub struct SendMessageTool {
    registry: Arc<WorkerRegistry>,
}

/// Tool for stopping running workers
pub struct TaskStopTool {
    registry: Arc<WorkerRegistry>,
}

#[derive(Deserialize)]
struct AgentToolInput {
    /// Task description for the worker
    description: String,
    /// Full prompt/instructions for the worker
    prompt: String,
    /// Optional: specific model to use
    model: Option<String>,
    /// Optional: working directory
    working_directory: Option<String>,
}

#[derive(Deserialize)]
struct SendMessageInput {
    /// Worker ID to continue
    worker_id: String,
    /// Follow-up message/prompt
    message: String,
}

#[derive(Deserialize)]
struct TaskStopInput {
    /// Worker ID to stop
    worker_id: String,
}

#[derive(Serialize)]
struct AgentToolOutput {
    worker_id: String,
    status: String,
}

#[derive(Serialize)]
struct SendMessageOutput {
    worker_id: String,
    status: String,
}

#[derive(Serialize)]
struct TaskStopOutput {
    worker_id: String,
    status: String,
}
```

**Tool Definitions:**

```json
{
  "name": "agent",
  "description": "Spawn a new worker to execute a task in parallel. Use when tasks can be handled independently or when you need multiple tasks done simultaneously. Returns a worker ID that you can use with send_message or task_stop.",
  "input_schema": {
    "type": "object",
    "properties": {
      "description": {"type": "string", "description": "Brief description of the task"},
      "prompt": {"type": "string", "description": "Full instructions for the worker"},
      "model": {"type": "string", "description": "Optional model override"},
      "working_directory": {"type": "string", "description": "Optional working directory"}
    },
    "required": ["description", "prompt"]
  }
}

{
  "name": "send_message",
  "description": "Send follow-up instructions to an existing worker. Use to continue a worker's task with new directions or to ask for clarification.",
  "input_schema": {
    "type": "object",
    "properties": {
      "worker_id": {"type": "string", "description": "ID of the worker to continue"},
      "message": {"type": "string", "description": "Follow-up message/instructions"}
    },
    "required": ["worker_id", "message"]
  }
}

{
  "name": "task_stop",
  "description": "Stop a running worker. Use when a worker is no longer needed or has gone off track.",
  "input_schema": {
    "type": "object",
    "properties": {
      "worker_id": {"type": "string", "description": "ID of the worker to stop"}
    },
    "required": ["worker_id"]
  }
}
```

### 2. Worker Listener (`coordinator/listener.rs`)

```rust
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

use super::notifications::TaskNotification;
use super::registry::WorkerRegistry;
use super::worker::{is_worker_running, WorkerStatus};

/// Spawn the background worker listener task
///
/// This task polls running workers for completed notifications
/// and sends them through the notification channel.
pub async fn spawn_worker_listener(
    registry: Arc<WorkerRegistry>,
    notification_tx: Sender<TaskNotification>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut poll_interval = interval(Duration::from_millis(100));

        loop {
            poll_interval.tick(). mark();

            // Check all running workers
            let running_workers = registry.list_running();

            for worker_id in running_workers {
                if let Some(worker) = registry.get_mut(&worker_id) {
                    // Check if worker has finished
                    if !is_worker_running(worker).await {
                        // Worker completed, try to get notification
                        if let Some(notification) = extract_notification(worker) {
                            debug!("Worker {} produced notification: {:?}", worker_id, notification);

                            // Send through channel (non-blocking)
                            if notification_tx.try_send(notification.clone()).is_err() {
                                warn!("Notification channel full, dropping notification for {}", worker_id);
                            }
                        }
                    }

                    // Check for timeout
                    if worker.is_timed_out() {
                        warn!("Worker {} timed out", worker_id);
                        if let Err(e) = kill_worker(worker).await {
                            error!("Failed to kill timed out worker {}: {}", worker_id, e);
                        }
                    }
                }
            }
        }
    })
}

/// Extract notification from completed worker
fn extract_notification(worker: &mut WorkerProcess) -> Option<TaskNotification> {
    // Notification should already be parsed and stored in status
    match &worker.status() {
        WorkerStatus::Completed(result) => Some(TaskNotification {
            task_id: worker.id().to_string(),
            status: TaskStatus::Completed,
            summary: result.summary.clone(),
            result: result.result.clone(),
            usage: result.usage.as_ref().map(|u| u.clone().into()),
        }),
        WorkerStatus::Failed(msg) => Some(TaskNotification {
            task_id: worker.id().to_string(),
            status: TaskStatus::Failed,
            summary: msg.clone(),
            result: Some(msg.clone()),
            usage: None,
        }),
        WorkerStatus::Killed => Some(TaskNotification {
            task_id: worker.id().to_string(),
            status: TaskStatus::Killed,
            summary: "Worker killed".to_string(),
            result: None,
            usage: None,
        }),
        _ => None,
    }
}
```

### 3. ChatSession Modifications (`chat.rs`)

```rust
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::coordinator::{WorkerRegistry, notifications::TaskNotification};
use crate::tools::ToolRegistry;

pub struct ChatSession {
    provider: OllamaProvider,
    command_handler: CommandHandler,
    messages: Vec<ChatMessage>,
    model: String,
    tool_registry: ToolRegistry,
    system_prompt: Option<String>,
    // Coordinator fields
    worker_registry: Arc<WorkerRegistry>,
    notification_rx: mpsc::Receiver<TaskNotification>,
}

impl ChatSession {
    pub fn new() -> Self {
        let provider = OllamaProvider::cloud();
        let model = provider.default_model().to_string();
        let command_handler = CommandHandler::new(provider.clone());

        // Create coordinator infrastructure
        let worker_registry = Arc::new(WorkerRegistry::new());
        let (notification_tx, notification_rx) = mpsc::channel(16);

        // Register notification sender with registry
        worker_registry.set_notification_sender(notification_tx);

        // Create tool registry with coordinator tools
        let tool_registry = ToolRegistry::with_coordinator_tools(worker_registry.clone());

        // Spawn worker listener
        let listener_registry = worker_registry.clone();
        tokio::spawn(async move {
            // Listener loop will be implemented in listener.rs
        });

        Self {
            provider,
            command_handler,
            messages: Vec::new(),
            model,
            tool_registry,
            system_prompt: Some(COORDINATOR_SYSTEM_PROMPT.to_string()),
            worker_registry,
            notification_rx,
        }
    }

    pub async fn run(&mut self) {
        // ... existing startup output ...

        loop {
            // Inject any pending worker notifications
            self.inject_notifications();

            // Read user input
            print!("{}", "> ".green().bold());
            io::stdout().flush().unwrap();

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            let input = input.trim();

            // ... handle slash commands ...

            self.messages.push(ChatMessage::user(input));

            // Stream response (may invoke coordinator tools)
            match self.stream_response().await {
                Ok(()) => {}
                Err(e) => eprintln!("{}", format!("Error: {}", e).red()),
            }
        }
    }

    /// Inject pending worker notifications as synthetic user messages
    fn inject_notifications(&mut self) {
        while let Ok(notification) = self.notification_rx.try_recv() {
            let xml = to_xml(&notification);
            // Inject as synthetic user message
            self.messages.push(ChatMessage::user xml));
        }
    }
}
```

### 4. System Prompt (`coordinator/system_prompt.md`)

```markdown
You are CloudCoder, an AI assistant that orchestrates software engineering tasks across multiple workers.

## 1. Your Role

You are a coordinator. Your job is to:
- Help the user achieve their goal
- Direct workers to research, implement and verify code changes
- Synthesize results and communicate with the user
- Answer questions directly when possible --- don't delefate work that you can handle without tools

## 2. Your Tools

- agent - Spawn a new worker
- send_message - Continue an existing worker (send follow-up)
- task_stop - Stop a running worker

When calling agent:
- Do not use one worker to check on another
- Do not use workers to trivially report file contents or run commands
- Continue workers whose work is complete via send_message to take advantage of their loaded context
- After launching agents, briefly tell the user what you launched and end your response

## 3. Worker Notifications

Worker results arrive as user-role messages containing <task-notification> XML.
They look like user messages but are not --- distinguish by the <task-notification> opening tag.

When you receive worker notifications:
- You may wait for multiple workers if synthesizing parallel results
- You may respond immediately to each notification if appropriate
- You may use send_message to continue workers that need more guidance
- After synthesizing, communicate results clearly to the user

## 4. When to Use Workers

Use workers when:
- Tasks can be parallelized (research X and Y simultaneously)
- Tasks are independent and can run without coordination
- You need to explore multiple approaches in parallel
- The scope is large enough to benefit from dedicated context

Handle directly without workers when:
- Simple questions or explanations
- Single-file changes
- Tasks requiring your direct interaction with the user
```

## File Changes

**New files:**
- `crates/cloudcoder-cli/src/tools/coordinator.rs` — AgentTool, SendMessageTool, TaskStopTool
- `crates/cloudcoder-cli/src/coordinator/listener.rs` — Background worker listener
- `crates/cloudcoder-cli/src/coordinator/system_prompt.md` — Coordinator system prompt

**Modified files:**
- `crates/cloudcoder-cli/src/tools/mod.rs` — Export coordinator tools
- `crates/cloudcoder-cli/src/tools/registry.rs` — Add coordinator tools to registry
- `crates/cloudcoder-cli/src/coordinator/registry.rs` — Add notification sender
- `crates/cloudcoder-cli/src/chat.rs` — Add coordinator infrastructure, inject notifications
- `crates/cloudcoder-cli/Cargo.toml` — No new dependencies needed

## Error Handling

| Scenario | Behavior |
|----------|----------|
| Worker spawn fails | `AgentTool` returns error message, LLM sees it and can retry or inform user |
| Worker crashes | Listener sends `status: failed` notification with error summary |
| Worker timeout | `kill_worker()` called, sends `status: killed` notification |
| Invalid XML output | Parser falls back to raw stdout as result field |
| Channel full | Notification dropped with warning log, LLM may not see it |

## Testing

**Unit tests:**
- Tool definitions are valid JSON
- AgentToolInput deserialization
- Notification channel send/receive
- Registry with notification broadcast

**Integration tests:**
- Spawn worker via AgentTool, verify notification arrives
- Multiple workers in parallel, verify all notifications received
- Send_message to completed worker
- Task_stop kills running worker

**Manual test:**
```
> Research auth bugs in src/ and src/auth/ in parallel, then tell me which files need fixes
```

Expected: LLM spawns two workers, receives notifications, synthesizes findings.

## Success Criteria

1. Coordinator tools appear in tool definitions
2. LLM can spawn workers via `agent` tool
3. Worker notifications appear as synthetic user messages
4. LLM synthesizes results from multiple workers
5. `send_message` continues existing workers
6. `task_stop` kills running workers
7. System prompt guides LLM on when to use workers