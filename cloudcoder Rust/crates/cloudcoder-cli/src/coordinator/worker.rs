//! Worker process launcher for CloudCoder coordinator mode.
//!
//! This module handles spawning, monitoring, and managing worker subprocesses.
//! Workers run as child processes executing `cloudcoder agent` commands and
//! communicate their results via XML notifications parsed from stdout.

use std::process::Stdio;
use std::time::{Duration, Instant};
use std::collections::VecDeque;

use thiserror::Error;
use tokio::process::{Child, Command};
use tokio::io::{AsyncBufReadExt, BufReader as AsyncBufReader};
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

use super::notifications::{parse, TaskNotification, TaskStatus, TaskUsage};

/// Error type for worker operations
#[derive(Debug, Error)]
pub enum WorkerError {
    /// Failed to spawn the worker process
    #[error("Failed to spawn worker: {0}")]
    SpawnFailed(String),

    /// Worker process exited with non-zero status
    #[error("Worker process failed with exit code: {0}")]
    ProcessFailed(i32),

    /// Worker timed out
    #[error("Worker timed out after {0}ms")]
    Timeout(u64),

    /// Failed to kill worker process
    #[error("Failed to kill worker: {0}")]
    KillFailed(String),

    /// Failed to parse worker output
    #[error("Failed to parse worker notification: {0}")]
    ParseError(#[from] super::notifications::ParseError),

    /// Failed to validate worker notification
    #[error("Worker notification validation failed: {0}")]
    ValidationError(#[from] super::notifications::ValidationError),

    /// Worker ID not found
    #[error("Worker not found: {0}")]
    NotFound(String),

    /// I/O error during worker operation
    #[error("I/O error: {0}")]
    IoError(String),

    /// Worker produced no valid notification
    #[error("Worker produced no valid task notification")]
    NoNotification,
}

/// Configuration for spawning a worker process
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// Unique identifier for this worker
    pub id: String,

    /// Human-readable description of the task
    pub description: String,

    /// The prompt to send to the worker
    pub prompt: String,

    /// Optional conversation ID to continue from (for SendMessage continuation)
    pub continue_from: Option<String>,

    /// Optional model to use (overrides default)
    pub model: Option<String>,

    /// Optional system prompt for the worker
    pub system_prompt: Option<String>,

    /// Optional timeout in milliseconds (default: 5 minutes)
    pub timeout_ms: Option<u64>,

    /// Working directory for the worker (default: current directory)
    pub working_dir: Option<String>,
}

impl WorkerConfig {
    /// Create a new worker config with required fields
    pub fn new(id: impl Into<String>, description: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            prompt: prompt.into(),
            continue_from: None,
            model: None,
            system_prompt: None,
            timeout_ms: None,
            working_dir: None,
        }
    }

    /// Set the conversation ID to continue from
    pub fn with_continue_from(mut self, conversation_id: impl Into<String>) -> Self {
        self.continue_from = Some(conversation_id.into());
        self
    }

    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the system prompt
    pub fn with_system_prompt(mut self, system_prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(system_prompt.into());
        self
    }

    /// Set the timeout in milliseconds
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    /// Set the working directory
    pub fn with_working_dir(mut self, working_dir: impl Into<String>) -> Self {
        self.working_dir = Some(working_dir.into());
        self
    }

    /// Get the timeout in milliseconds (default: 5 minutes)
    pub fn get_timeout_ms(&self) -> u64 {
        self.timeout_ms.unwrap_or(300_000) // 5 minutes default
    }
}

/// Status of a worker process
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerStatus {
    /// Worker is currently running
    Running,

    /// Worker completed successfully with result
    Completed(WorkerResult),

    /// Worker failed with error message
    Failed(String),

    /// Worker was killed (either by timeout or coordinator)
    Killed,
}

impl WorkerStatus {
    /// Check if the worker is still running
    pub fn is_running(&self) -> bool {
        matches!(self, WorkerStatus::Running)
    }

    /// Check if the worker has finished (any terminal state)
    pub fn is_finished(&self) -> bool {
        !self.is_running()
    }

    /// Get the result if completed successfully
    pub fn get_result(&self) -> Option<&WorkerResult> {
        match self {
            WorkerStatus::Completed(result) => Some(result),
            _ => None,
        }
    }

    /// Get the error message if failed
    pub fn get_error(&self) -> Option<&str> {
        match self {
            WorkerStatus::Failed(msg) => Some(msg),
            _ => None,
        }
    }
}

/// Result from a completed worker
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerResult {
    /// Human-readable summary of what the worker did
    pub summary: String,

    /// Optional detailed result
    pub result: Option<String>,

    /// Optional usage statistics
    pub usage: Option<WorkerUsage>,
}

impl WorkerResult {
    /// Create a new worker result with the summary
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            result: None,
            usage: None,
        }
    }

    /// Add a detailed result
    pub fn with_result(mut self, result: impl Into<String>) -> Self {
        self.result = Some(result.into());
        self
    }

    /// Add usage statistics
    pub fn with_usage(mut self, usage: WorkerUsage) -> Self {
        self.usage = Some(usage);
        self
    }
}

/// Usage statistics for a worker
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerUsage {
    /// Total tokens consumed
    pub total_tokens: u64,

    /// Number of tool invocations
    pub tool_uses: u64,

    /// Duration in milliseconds
    pub duration_ms: u64,
}

impl WorkerUsage {
    /// Create new usage statistics
    pub fn new(total_tokens: u64, tool_uses: u64, duration_ms: u64) -> Self {
        Self {
            total_tokens,
            tool_uses,
            duration_ms,
        }
    }
}

impl From<TaskUsage> for WorkerUsage {
    fn from(usage: TaskUsage) -> Self {
        Self {
            total_tokens: usage.total_tokens,
            tool_uses: usage.tool_uses,
            duration_ms: usage.duration_ms,
        }
    }
}

impl From<WorkerUsage> for TaskUsage {
    fn from(usage: WorkerUsage) -> Self {
        Self {
            total_tokens: usage.total_tokens,
            tool_uses: usage.tool_uses,
            duration_ms: usage.duration_ms,
        }
    }
}

/// A running worker process
pub struct WorkerProcess {
    /// Unique identifier for this worker
    id: String,

    /// Human-readable description
    description: String,

    /// The child process
    child: Option<Child>,

    /// Current status
    status: WorkerStatus,

    /// Timestamp when the worker started (milliseconds since epoch)
    started_at: u64,

    /// Instant when the worker started (for duration calculation)
    started_instant: Instant,

    /// Timeout duration
    timeout_duration: Duration,

    /// Buffer for collecting stdout lines
    stdout_buffer: VecDeque<String>,
}

impl std::fmt::Debug for WorkerProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerProcess")
            .field("id", &self.id)
            .field("description", &self.description)
            .field("child", &self.child.as_ref().map(|_| "Child"))
            .field("status", &self.status)
            .field("started_at", &self.started_at)
            .field("started_instant", &"<instant>")
            .field("timeout_duration", &self.timeout_duration)
            .field("stdout_buffer_len", &self.stdout_buffer.len())
            .finish()
    }
}

impl WorkerProcess {
    /// Get the worker ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the worker description
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Get the current status
    pub fn status(&self) -> &WorkerStatus {
        &self.status
    }

    /// Check if the worker is still running
    pub fn is_running(&self) -> bool {
        self.status.is_running() && self.child.is_some()
    }

    /// Get the runtime in milliseconds
    pub fn get_runtime_ms(&self) -> u64 {
        self.started_instant.elapsed().as_millis() as u64
    }

    /// Get the remaining time before timeout in milliseconds
    pub fn get_remaining_ms(&self) -> u64 {
        let elapsed = self.started_instant.elapsed();
        let remaining = self.timeout_duration.saturating_sub(elapsed);
        remaining.as_millis() as u64
    }

    /// Check if the worker has timed out
    pub fn is_timed_out(&self) -> bool {
        self.started_instant.elapsed() > self.timeout_duration
    }

    /// Get the timestamp when the worker started
    pub fn started_at(&self) -> u64 {
        self.started_at
    }

    /// Set the worker status (for use by registry)
    pub fn set_status(&mut self, status: WorkerStatus) {
        self.status = status;
    }

    /// Create a WorkerProcess for testing purposes only
    ///
    /// This creates a worker without spawning a process, useful for unit tests.
    #[cfg(test)]
    pub fn test_new(id: &str, description: &str) -> Self {
        use std::time::SystemTime;
        let started_at = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Self {
            id: id.to_string(),
            description: description.to_string(),
            child: None,
            status: WorkerStatus::Running,
            started_at,
            started_instant: Instant::now(),
            timeout_duration: Duration::from_secs(60),
            stdout_buffer: VecDeque::new(),
        }
    }
}

/// Spawn a new worker process
///
/// # Arguments
///
/// * `config` - The worker configuration
///
/// # Returns
///
/// A `Result` containing the spawned `WorkerProcess` or a `WorkerError`.
///
/// # Example
///
/// ```ignore
/// let config = WorkerConfig::new("agent-123", "Research auth bug", "Find the auth issue");
/// let worker = spawn_worker(config).await?;
/// ```
pub async fn spawn_worker(config: WorkerConfig) -> Result<WorkerProcess, WorkerError> {
    let timeout_ms = config.get_timeout_ms();
    let timeout_duration = Duration::from_millis(timeout_ms);
    let started_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    // Build the command arguments
    let mut args = vec![
        "agent".to_string(),
        "--id".to_string(),
        config.id.clone(),
        "--description".to_string(),
        config.description.clone(),
        "--is-worker".to_string(),
    ];

    // Add optional model
    if let Some(ref model) = config.model {
        args.push("--model".to_string());
        args.push(model.clone());
    }

    // Add optional system prompt
    if let Some(ref system_prompt) = config.system_prompt {
        args.push("--system-prompt".to_string());
        args.push(system_prompt.clone());
    }

    // Add optional continuation
    if let Some(ref conversation_id) = config.continue_from {
        args.push("--continue".to_string());
        args.push(conversation_id.clone());
    }

    // Add the prompt as the last argument
    args.push("--prompt".to_string());
    args.push(config.prompt.clone());

    info!("Spawning worker {} with args: {:?}", config.id, args);

    // Build the command
    let mut cmd = Command::new("cloudcoder");
    cmd.args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Set working directory if specified
    if let Some(ref working_dir) = config.working_dir {
        cmd.current_dir(working_dir);
    }

    // Spawn the process
    let child = cmd.spawn().map_err(|e| {
        WorkerError::SpawnFailed(format!("Failed to spawn cloudcoder process: {}", e))
    })?;

    debug!("Worker {} spawned successfully", config.id);

    Ok(WorkerProcess {
        id: config.id,
        description: config.description,
        child: Some(child),
        status: WorkerStatus::Running,
        started_at,
        started_instant: Instant::now(),
        timeout_duration,
        stdout_buffer: VecDeque::new(),
    })
}

/// Wait for a worker to complete and parse its result
///
/// This function reads the worker's stdout looking for XML task notifications.
/// If the worker times out, it will be killed.
///
/// # Arguments
///
/// * `worker` - The worker process to wait for
/// * `timeout_ms` - Optional timeout in milliseconds (uses worker's default if not specified)
///
/// # Returns
///
/// A `Result` containing the `WorkerResult` or a `WorkerError`.
pub async fn wait_for_completion(
    worker: &mut WorkerProcess,
    timeout_ms: Option<u64>,
) -> Result<WorkerResult, WorkerError> {
    let effective_timeout = timeout_ms.unwrap_or_else(|| worker.timeout_duration.as_millis() as u64);

    // Try to wait with timeout
    let wait_result = timeout(
        Duration::from_millis(effective_timeout),
        async {
            // We need to poll the child process status
            if let Some(ref mut child) = worker.child {
                // Read stdout lines
                if let Some(stdout) = child.stdout.take() {
                    let reader = AsyncBufReader::new(stdout);
                    let mut lines = reader.lines();

                    while let Ok(Some(line)) = lines.next_line().await {
                        debug!("Worker {} stdout: {}", worker.id, line);
                        worker.stdout_buffer.push_back(line);
                    }
                }

                // Wait for the process to complete
                child.wait().await
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "No child process",
                ))
            }
        },
    )
    .await;

    match wait_result {
        Ok(Ok(exit_status)) => {
            // Process completed
            let exit_code = exit_status.code().unwrap_or(-1);
            debug!("Worker {} exited with code {}", worker.id, exit_code);

            if exit_code != 0 {
                worker.status = WorkerStatus::Failed(format!("Exit code: {}", exit_code));
                return Err(WorkerError::ProcessFailed(exit_code));
            }

            // Parse the output for task notifications
            parse_worker_result(worker)
        }
        Ok(Err(e)) => {
            error!("Worker {} I/O error: {}", worker.id, e);
            worker.status = WorkerStatus::Failed(e.to_string());
            Err(WorkerError::IoError(e.to_string()))
        }
        Err(_) => {
            // Timeout occurred
            warn!("Worker {} timed out after {}ms", worker.id, effective_timeout);
            worker.status = WorkerStatus::Failed(format!("Timed out after {}ms", effective_timeout));

            // Kill the worker
            kill_worker(worker).await?;

            Err(WorkerError::Timeout(effective_timeout))
        }
    }
}

/// Kill a running worker process
///
/// First attempts graceful termination (SIGTERM on Unix), then forces
/// termination (SIGKILL) after a short grace period.
///
/// # Arguments
///
/// * `worker` - The worker process to kill
///
/// # Returns
///
/// A `Result` indicating success or a `WorkerError`.
pub async fn kill_worker(worker: &mut WorkerProcess) -> Result<(), WorkerError> {
    if let Some(ref mut child) = worker.child {
        debug!("Killing worker {}", worker.id);

        // First try graceful shutdown with SIGTERM (Unix) or Ctrl+C event (Windows)
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;

            if let Some(pid) = child.id() {
                let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
            }
        }

        #[cfg(windows)]
        {
            // On Windows, we need to use the win32 API for graceful shutdown
            // For simplicity, we'll go directly to kill
        }

        // Wait a short grace period for graceful shutdown
        tokio::select! {
            result = child.wait() => {
                match result {
                    Ok(_) => {
                        debug!("Worker {} terminated gracefully", worker.id);
                    }
                    Err(e) => {
                        warn!("Worker {} grace period error: {}", worker.id, e);
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                // Grace period expired, force kill
                debug!("Worker {} grace period expired, forcing kill", worker.id);

                match child.kill().await {
                    Ok(_) => {
                        debug!("Worker {} killed forcibly", worker.id);
                    }
                    Err(e) => {
                        error!("Failed to kill worker {}: {}", worker.id, e);
                        return Err(WorkerError::KillFailed(e.to_string()));
                    }
                }
            }
        }
    }

    worker.status = WorkerStatus::Killed;
    worker.child = None;

    Ok(())
}

/// Parse worker output lines for task notifications
fn parse_worker_result(worker: &mut WorkerProcess) -> Result<WorkerResult, WorkerError> {
    // Look for XML task notifications in the output
    let mut found_notification: Option<TaskNotification> = None;
    let mut last_error: Option<String> = None;

    // Search through buffered output for XML notifications
    let buffer: Vec<String> = worker.stdout_buffer.drain(..).collect();
    let output = buffer.join("\n");

    // Try to find XML notification blocks
    if let Some(start) = output.find("<task-notification") {
        if let Some(end) = output.find("</task-notification>") {
            let xml = &output[start..=end + "</task-notification>".len() - 1];

            match parse(xml) {
                Ok(notification) => {
                    // Validate the notification
                    if let Err(e) = super::notifications::validate(&notification) {
                        last_error = Some(format!("Validation error: {}", e));
                    } else {
                        found_notification = Some(notification);
                    }
                }
                Err(e) => {
                    last_error = Some(format!("Parse error: {}", e));
                }
            }
        }
    }

    match found_notification {
        Some(notification) => {
            // Verify the notification is for this worker
            if notification.task_id != worker.id {
                warn!(
                    "Worker {} received notification for different task: {}",
                    worker.id, notification.task_id
                );
            }

            // Create worker result from notification
            let result = WorkerResult {
                summary: notification.summary.clone(),
                result: notification.result.clone(),
                usage: notification.usage.map(WorkerUsage::from),
            };

            // Update status based on notification
            worker.status = match notification.status {
                TaskStatus::Completed => WorkerStatus::Completed(result.clone()),
                TaskStatus::Failed => WorkerStatus::Failed(notification.result.unwrap_or_else(|| "Unknown error".to_string())),
                TaskStatus::Killed => WorkerStatus::Killed,
            };

            Ok(result)
        }
        None => {
            let error_msg = last_error
                .map(|e| format!("No valid notification found: {}", e))
                .unwrap_or_else(|| "No task notification found in worker output".to_string());

            worker.status = WorkerStatus::Failed(error_msg.clone());
            Err(WorkerError::NoNotification)
        }
    }
}

/// Check if a worker is still running (non-blocking)
///
/// This function checks the worker's status without blocking.
/// It returns `true` if the worker is still running, `false` otherwise.
///
/// # Arguments
///
/// * `worker` - The worker process to check
///
/// # Returns
///
/// `true` if the worker is running, `false` if completed or failed.
pub async fn is_worker_running(worker: &mut WorkerProcess) -> bool {
    if worker.status.is_finished() {
        return false;
    }

    if let Some(ref mut child) = worker.child {
        // Try to poll the process without blocking
        match child.try_wait() {
            Ok(Some(_status)) => {
                // Process has exited
                false
            }
            Ok(None) => {
                // Process is still running
                true
            }
            Err(_) => {
                // Error checking status - assume not running
                false
            }
        }
    } else {
        false
    }
}

/// Get a snapshot of a worker's current runtime
///
/// # Arguments
///
/// * `worker` - The worker process
///
/// # Returns
///
/// The runtime in milliseconds.
pub fn get_worker_runtime_ms(worker: &WorkerProcess) -> u64 {
    worker.get_runtime_ms()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_config_creation() {
        let config = WorkerConfig::new("agent-123", "Test task", "Test prompt");

        assert_eq!(config.id, "agent-123");
        assert_eq!(config.description, "Test task");
        assert_eq!(config.prompt, "Test prompt");
        assert!(config.continue_from.is_none());
        assert!(config.model.is_none());
        assert!(config.system_prompt.is_none());
        assert_eq!(config.get_timeout_ms(), 300_000); // Default timeout
    }

    #[test]
    fn test_worker_config_with_options() {
        let config = WorkerConfig::new("agent-456", "Test", "Prompt")
            .with_model("claude-3-opus")
            .with_system_prompt("You are a helpful assistant")
            .with_continue_from("conv-123")
            .with_timeout_ms(60_000);

        assert_eq!(config.model, Some("claude-3-opus".to_string()));
        assert_eq!(config.system_prompt, Some("You are a helpful assistant".to_string()));
        assert_eq!(config.continue_from, Some("conv-123".to_string()));
        assert_eq!(config.get_timeout_ms(), 60_000);
    }

    #[test]
    fn test_worker_status_transitions() {
        // Running status
        let running = WorkerStatus::Running;
        assert!(running.is_running());
        assert!(!running.is_finished());
        assert!(running.get_result().is_none());
        assert!(running.get_error().is_none());

        // Completed status
        let result = WorkerResult::new("Task completed");
        let completed = WorkerStatus::Completed(result);
        assert!(!completed.is_running());
        assert!(completed.is_finished());
        assert!(completed.get_result().is_some());
        assert!(completed.get_error().is_none());

        // Failed status
        let failed = WorkerStatus::Failed("Something went wrong".to_string());
        assert!(!failed.is_running());
        assert!(failed.is_finished());
        assert!(failed.get_result().is_none());
        assert!(failed.get_error().is_some());

        // Killed status
        let killed = WorkerStatus::Killed;
        assert!(!killed.is_running());
        assert!(killed.is_finished());
    }

    #[test]
    fn test_worker_result_creation() {
        let result = WorkerResult::new("Summary")
            .with_result("Detailed result")
            .with_usage(WorkerUsage::new(1000, 10, 5000));

        assert_eq!(result.summary, "Summary");
        assert_eq!(result.result, Some("Detailed result".to_string()));
        assert!(result.usage.is_some());

        let usage = result.usage.unwrap();
        assert_eq!(usage.total_tokens, 1000);
        assert_eq!(usage.tool_uses, 10);
        assert_eq!(usage.duration_ms, 5000);
    }

    #[test]
    fn test_worker_usage_conversion() {
        let task_usage = TaskUsage {
            total_tokens: 500,
            tool_uses: 5,
            duration_ms: 3000,
        };

        let worker_usage: WorkerUsage = task_usage.clone().into();
        assert_eq!(worker_usage.total_tokens, 500);
        assert_eq!(worker_usage.tool_uses, 5);
        assert_eq!(worker_usage.duration_ms, 3000);

        let back_to_task: TaskUsage = worker_usage.into();
        assert_eq!(back_to_task.total_tokens, 500);
    }

    #[test]
    fn test_worker_error_display() {
        let error = WorkerError::SpawnFailed("Process not found".to_string());
        assert!(error.to_string().contains("Failed to spawn worker"));
        assert!(error.to_string().contains("Process not found"));

        let error = WorkerError::Timeout(60_000);
        assert!(error.to_string().contains("60"));
        assert!(error.to_string().contains("timed out"));

        let error = WorkerError::ProcessFailed(1);
        assert!(error.to_string().contains("exit code"));
    }

    #[test]
    fn test_worker_process_runtime() {
        let started_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let worker = WorkerProcess {
            id: "test-worker".to_string(),
            description: "Test".to_string(),
            child: None,
            status: WorkerStatus::Running,
            started_at,
            started_instant: Instant::now(),
            timeout_duration: Duration::from_millis(60_000),
            stdout_buffer: VecDeque::new(),
        };

        // Runtime should be very small (just created)
        let runtime = worker.get_runtime_ms();
        assert!(runtime < 1000); // Less than 1 second

        // Check timeout status
        assert!(!worker.is_timed_out());
        assert!(worker.get_remaining_ms() > 50_000); // Should have most of timeout remaining
    }

    #[test]
    fn test_worker_process_accessors() {
        let worker = WorkerProcess {
            id: "test-123".to_string(),
            description: "Test worker".to_string(),
            child: None,
            status: WorkerStatus::Running,
            started_at: 0,
            started_instant: Instant::now(),
            timeout_duration: Duration::from_millis(60_000),
            stdout_buffer: VecDeque::new(),
        };

        assert_eq!(worker.id(), "test-123");
        assert_eq!(worker.description(), "Test worker");
        assert!(matches!(worker.status(), WorkerStatus::Running));
        // is_running() checks both status and child presence, so with no child it returns false
        assert!(!worker.is_running());
        // But the status itself is still Running
        assert!(worker.status().is_running());
    }

    #[test]
    fn test_worker_process_completed_status() {
        let result = WorkerResult::new("Done");
        let worker = WorkerProcess {
            id: "test-complete".to_string(),
            description: "Completed worker".to_string(),
            child: None,
            status: WorkerStatus::Completed(result),
            started_at: 0,
            started_instant: Instant::now(),
            timeout_duration: Duration::from_millis(60_000),
            stdout_buffer: VecDeque::new(),
        };

        assert!(!worker.is_running());
        assert!(matches!(worker.status(), WorkerStatus::Completed(_)));
    }

    #[test]
    fn test_parse_worker_result_valid_xml() {
        let mut worker = WorkerProcess {
            id: "agent-xml-test".to_string(),
            description: "Test".to_string(),
            child: None,
            status: WorkerStatus::Running,
            started_at: 0,
            started_instant: Instant::now(),
            timeout_duration: Duration::from_millis(60_000),
            stdout_buffer: VecDeque::new(),
        };

        // Add XML notification to buffer
        let xml = r#"<task-notification>
            <task-id>agent-xml-test</task-id>
            <status>completed</status>
            <summary>Task completed successfully</summary>
            <result>Found the answer</result>
            <usage>
                <total_tokens>1000</total_tokens>
                <tool_uses>10</tool_uses>
                <duration_ms>5000</duration_ms>
            </usage>
        </task-notification>"#;

        worker.stdout_buffer.push_back(xml.to_string());

        let result = parse_worker_result(&mut worker).expect("Should parse valid XML");

        assert_eq!(result.summary, "Task completed successfully");
        assert_eq!(result.result, Some("Found the answer".to_string()));
        assert!(result.usage.is_some());

        let usage = result.usage.unwrap();
        assert_eq!(usage.total_tokens, 1000);
        assert_eq!(usage.tool_uses, 10);
        assert_eq!(usage.duration_ms, 5000);
    }

    #[test]
    fn test_parse_worker_result_no_notification() {
        let mut worker = WorkerProcess {
            id: "agent-no-xml".to_string(),
            description: "Test".to_string(),
            child: None,
            status: WorkerStatus::Running,
            started_at: 0,
            started_instant: Instant::now(),
            timeout_duration: Duration::from_millis(60_000),
            stdout_buffer: VecDeque::new(),
        };

        // Add non-XML output
        worker.stdout_buffer.push_back("Some random output".to_string());
        worker.stdout_buffer.push_back("No XML here".to_string());

        let result = parse_worker_result(&mut worker);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), WorkerError::NoNotification));
    }

    #[test]
    fn test_parse_worker_result_status_update() {
        let mut worker = WorkerProcess {
            id: "agent-status-test".to_string(),
            description: "Test".to_string(),
            child: None,
            status: WorkerStatus::Running,
            started_at: 0,
            started_instant: Instant::now(),
            timeout_duration: Duration::from_millis(60_000),
            stdout_buffer: VecDeque::new(),
        };

        // Add a failed notification
        let xml = r#"<task-notification>
            <task-id>agent-status-test</task-id>
            <status>failed</status>
            <summary>Task failed</summary>
            <result>Something went wrong</result>
        </task-notification>"#;

        worker.stdout_buffer.push_back(xml.to_string());

        let _ = parse_worker_result(&mut worker);

        // Status should be updated to Failed
        assert!(matches!(worker.status, WorkerStatus::Failed(_)));
    }
}