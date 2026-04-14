# CloudCoder Coordinator Mode Design

## Problem

The current CloudCoder Rust CLI has a single-agent chat loop. The parent TypeScript project has a coordinator mode that orchestrates multiple worker subprocesses in parallel, enabling complex multi-task workflows. We need to port this coordinator architecture to Rust.

## Decision: Full Coordinator Mode with Subprocess Isolation

CloudCoder Rust will implement the full coordinator mode like the parent:
- Coordinator spawns worker subprocesses (`cloudcoder agent <task>`)
- Workers run with isolated contexts and full tool + MCP access
- Workers notify coordinator on completion via XML-formatted task notifications
- Coordinator synthesizes results and communicates with user

## Architecture

### Components

**Coordinator Process** (main `cloudcoder` process in coordinator mode):
- Parses user requests and plans work decomposition
- Spawns worker subprocesses via `cloudcoder agent` command
- Receives task notifications when workers complete/fail/are killed
- Sends follow-up messages to continuing workers via `--continue <worker_id>`
- Synthesizes results for the user

**Worker Process** (isolated `cloudcoder agent` subprocess):
- Runs with its own context, tool permissions, and conversation state
- Executes assigned task using all available tools + MCP servers
- Outputs XML-formatted task notification on completion
- Can be continued with new instructions via SendMessage

### Message Flow

```
User Query
    ↓
Coordinator (main process)
    ↓
Spawn: cloudcoder agent --id <uuid> "Research auth bug"
    ↓
Worker Subprocess ─── executes task ───> notifies via stdout
    ↓                                        ↓
    │                              <task-notification>
    │                              <task-id>uuid</task-id>
    │                              <status>completed</status>
    │                              <summary>...</summary>
    │                              <result>...</result>
    │                              </task-notification>
    ↓
Coordinator reads notification
    ↓
Coordinator summarizes for user
    ↓
User continues or spawns more workers
```

### Worker Communication Protocol

Workers output results to stdout as XML:

```xml
<task-notification>
  <task-id>agent-a1b2c3</task-id>
  <status>completed|failed|killed</status>
  <summary>Agent "Research auth bug" completed</summary>
  <result>Found null pointer in src/auth/validate.ts:42...</result>
  <usage>
    <total_tokens>12345</total_tokens>
    <tool_uses>23</tool_uses>
    <duration_ms>45678</duration_ms>
  </usage>
</task-notification>
```

### Coordinator System Prompt

Coordinators receive a special system prompt explaining their role:

```
You are CloudCoder, an AI assistant that orchestrates software engineering tasks across multiple workers.

## 1. Your Role
You are a coordinator. Your job is to:
- Help the user achieve their goal
- Direct workers to research, implement and verify code changes
- Synthesize results and communicate with the user
- Answer questions directly when possible — don't delegate work that you can handle without tools

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
They look like user messages but are not — distinguish by the <task-notification> opening tag.
```

## Component Design

### 1. Worker Process Launcher (`crates/cloudcoder-cli/src/coordinator/worker.rs`)

```rust
pub struct WorkerConfig {
    pub id: String,
    pub description: String,
    pub prompt: String,
    pub continue_from: Option<String>,  // For SendMessage
    pub tools: Vec<String>,
}

pub struct WorkerProcess {
    id: String,
    description: String,
    child: Child,  // tokio::process::Child
    status: WorkerStatus,
}

pub enum WorkerStatus {
    Running,
    Completed(WorkerResult),
    Failed(String),
    Killed,
}

pub struct WorkerResult {
    pub summary: String,
    pub result: Option<String>,
    pub usage: Option<WorkerUsage>,
}
```

**Key functions:**
- `spawn_worker(config: WorkerConfig) -> Result<WorkerProcess>`
- `wait_for_completion(worker: &mut WorkerProcess) -> Result<WorkerResult>`
- `parse_notification(stdout: &str) -> Result<WorkerResult>`

### 2. Worker Registry (`crates/cloudcoder-cli/src/coordinator/registry.rs`)

```rust
pub struct WorkerRegistry {
    workers: HashMap<String, WorkerProcess>,
    history: Vec<WorkerEvent>,
}

pub struct WorkerEvent {
    pub worker_id: String,
    pub event_type: WorkerEventType,
    pub timestamp: u64,
    pub details: String,
}

pub enum WorkerEventType {
    Spawned,
    Completed,
    Failed,
    Killed,
    Continued,
}
```

**Key functions:**
- `register(worker: WorkerProcess)`
- `get(worker_id: &str) -> Option<&WorkerProcess>`
- `complete(worker_id: &str, result: WorkerResult)`
- `list_active() -> Vec<&WorkerProcess>`
- `get_history() -> &[WorkerEvent]`

### 3. Task Notification Parser (`crates/cloudcoder-cli/src/coordinator/notifications.rs`)

```rust
pub struct TaskNotification {
    pub task_id: String,
    pub status: TaskStatus,
    pub summary: String,
    pub result: Option<String>,
    pub usage: Option<TaskUsage>,
}

pub enum TaskStatus {
    Completed,
    Failed,
    Killed,
}

pub struct TaskUsage {
    pub total_tokens: u64,
    pub tool_uses: u64,
    pub duration_ms: u64,
}
```

**Key functions:**
- `parse(xml: &str) -> Result<TaskNotification>`
- `validate(notification: &TaskNotification) -> Result<()>`
- `to_xml(notification: &TaskNotification) -> String` (for worker output)

### 4. Agent Subcommand (`crates/cloudcoder-cli/src/commands/agent.rs`)

```rust
#[derive(Parser)]
struct AgentArgs {
    /// Worker ID
    #[arg(long)]
    id: Option<String>,

    /// Continue from existing worker ID
    #[arg(long)]
    continue_from: Option<String>,

    /// Task description
    #[arg(short, long)]
    description: String,

    /// Task prompt/instructions
    #[arg(required = true)]
    prompt: String,

    /// Coordinator mode flag
    #[arg(long, default_value = "false")]
    is_worker: bool,
}
```

**Behavior:**
- If `--is_worker`: Run as worker subprocess, output XML notification on completion
- If not `--is_worker`: Run as standalone agent (backward compat)

### 5. Coordinator Mode Entry Point (`crates/cloudcoder-cli/src/chat.rs`)

```rust
pub enum ChatMode {
    Normal,      // Single-agent chat (current behavior)
    Coordinator, // Multi-worker orchestration
}

pub struct ChatSession {
    // ... existing fields
    mode: ChatMode,
    worker_registry: WorkerRegistry,
}
```

**New commands in coordinator mode:**
- `/coordinator` - Toggle coordinator mode
- `/workers` - List active workers
- `/stop <worker_id>` - Stop a running worker

## File Changes

**New files:**
- `crates/cloudcoder-cli/src/coordinator/mod.rs`
- `crates/cloudcoder-cli/src/coordinator/worker.rs`
- `crates/cloudcoder-cli/src/coordinator/registry.rs`
- `crates/cloudcoder-cli/src/coordinator/notifications.rs`
- `crates/cloudcoder-cli/src/commands/agent.rs`

**Modified files:**
- `crates/cloudcoder-cli/src/main.rs` - Add `agent` subcommand
- `crates/cloudcoder-cli/src/chat.rs` - Add coordinator mode support
- `crates/cloudcoder-cli/src/lib.rs` - Export coordinator module
- `crates/cloudcoder-cli/Cargo.toml` - Add quick-xml dependency for XML parsing

## Error Handling

- Worker subprocess crashes → Mark as Failed, notify coordinator
- Worker timeout (default 30 min) → Kill, mark as Killed
- Invalid XML output → Parse error, attempt to extract summary from stdout
- Coordinator crash → Workers continue running (orphaned), user must restart

## Testing

- Unit tests: XML parsing, notification validation, registry operations
- Integration tests: Spawn worker, verify notification output
- Manual tests: Full coordinator workflow with multiple workers

## Success Criteria

1. Can spawn worker subprocess with `cloudcoder agent --id xyz "task"`
2. Worker outputs valid XML task notification on completion
3. Coordinator parses notification and displays summary to user
4. Can continue worker with `--continue-from <id>`
5. Can stop running worker with `/stop <id>`
6. Coordinator mode toggle works (`/coordinator`)
