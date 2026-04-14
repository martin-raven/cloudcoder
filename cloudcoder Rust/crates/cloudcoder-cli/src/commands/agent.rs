//! Agent subcommand for CloudCoder coordinator mode.
//!
//! This module implements the `cloudcoder agent` subcommand that can operate in two modes:
//! - Worker mode (`--is-worker`): Run as a worker subprocess, output XML notification on completion
//! - Standalone mode: Run as a single agent session with normal chat output

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::signal;
use tracing::{debug, error, info, warn};

use cloudcoder_provider::{ChatMessage, ChatRequest, ContentBlock, OllamaProvider, Provider};

use crate::coordinator::notifications::{TaskNotification, TaskStatus, TaskUsage, to_xml};
use crate::tools::ToolRegistry;

/// Agent subcommand arguments
#[derive(Parser, Debug, Clone)]
pub struct AgentArgs {
    /// Worker ID
    #[arg(long)]
    pub id: Option<String>,

    /// Continue from existing worker ID (SendMessage)
    #[arg(long)]
    pub continue_from: Option<String>,

    /// Task description
    #[arg(short, long)]
    pub description: String,

    /// Task prompt/instructions
    #[arg(short = 'p', long)]
    pub prompt: String,

    /// Run as worker (output XML notification on completion)
    #[arg(long, default_value = "false")]
    pub is_worker: bool,

    /// Model to use
    #[arg(long)]
    pub model: Option<String>,

    /// System prompt
    #[arg(long)]
    pub system: Option<String>,

    /// Timeout in milliseconds
    #[arg(long, default_value = "300000")]
    pub timeout_ms: u64,
}

/// Result of running an agent
#[derive(Debug, Clone)]
pub struct WorkerResult {
    /// Human-readable summary of what the worker did
    pub summary: String,
    /// Optional detailed result
    pub result: Option<String>,
    /// Total tokens consumed
    pub total_tokens: u64,
    /// Number of tool invocations
    pub tool_uses: u64,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Whether the task succeeded
    pub success: bool,
}

impl WorkerResult {
    /// Create a new worker result
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            result: None,
            total_tokens: 0,
            tool_uses: 0,
            duration_ms: 0,
            success: true,
        }
    }

    /// Add a detailed result
    pub fn with_result(mut self, result: impl Into<String>) -> Self {
        self.result = Some(result.into());
        self
    }

    /// Add usage statistics
    pub fn with_usage(mut self, total_tokens: u64, tool_uses: u64, duration_ms: u64) -> Self {
        self.total_tokens = total_tokens;
        self.tool_uses = tool_uses;
        self.duration_ms = duration_ms;
        self
    }

    /// Mark as failed
    pub fn failed(mut self) -> Self {
        self.success = false;
        self
    }
}

/// Run the agent command
pub async fn run_agent_command(args: AgentArgs) -> Result<(), Box<dyn std::error::Error>> {
    debug!("Running agent command with args: {:?}", args);

    if args.is_worker {
        run_worker_mode(&args).await
    } else {
        run_standalone_mode(&args).await
    }
}

/// Worker mode: Run chat session and output XML notification on completion
async fn run_worker_mode(args: &AgentArgs) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();
    let worker_id = args.id.clone().unwrap_or_else(|| {
        // Generate a unique ID if not provided
        format!("agent-{}", uuid::Uuid::new_v4())
    });

    info!("Starting worker {} with description: {}", worker_id, args.description);

    // Setup graceful shutdown
    let shutdown_flag = setup_shutdown_handler();

    // Setup progress indicator
    let progress = create_progress_bar(&args.description);
    progress.set_message("Initializing...");

    // Create the chat provider
    let provider = if let Some(ref model) = args.model {
        OllamaProvider::with_model(model.clone())
    } else {
        OllamaProvider::cloud()
    };

    let model = args.model.clone().unwrap_or_else(|| provider.default_model().to_string());

    debug!("Using model: {}", model);

    // Build messages
    let mut messages = Vec::new();

    // Add system prompt if provided
    if let Some(ref system) = args.system {
        messages.push(ChatMessage::system(system));
    }

    // Handle continuation
    if let Some(ref conversation_id) = args.continue_from {
        debug!("Continuing from conversation: {}", conversation_id);
        // In a real implementation, we'd load the conversation history here
        // For now, we just log it
    }

    // Add the user prompt
    messages.push(ChatMessage::user(&args.prompt));

    // Create tool registry
    let tool_registry = ToolRegistry::new();

    progress.set_message("Processing...");

    // Track metrics
    let mut total_tokens = 0u64;
    let mut tool_uses = 0u64;

    // Run the chat completion with tool support
    let result = run_chat_completion(
        &provider,
        &model,
        messages.clone(),
        &args.system,
        &tool_registry,
        args.timeout_ms,
        shutdown_flag.clone(),
        &progress,
        &mut total_tokens,
        &mut tool_uses,
    ).await;

    let duration_ms = start_time.elapsed().as_millis() as u64;
    progress.finish_and_clear();

    // Create worker result
    let worker_result = match result {
        Ok(response) => {
            let summary = extract_summary(&response);
            WorkerResult::new(&summary)
                .with_result(response)
                .with_usage(total_tokens, tool_uses, duration_ms)
        }
        Err(e) => {
            error!("Worker failed: {}", e);
            WorkerResult::new(format!("Task failed: {}", e))
                .with_result(e.to_string())
                .with_usage(total_tokens, tool_uses, duration_ms)
                .failed()
        }
    };

    // Output XML notification to stdout
    output_xml_notification(&worker_id, &worker_result);

    // Exit with appropriate code
    if worker_result.success {
        std::process::exit(0);
    } else {
        std::process::exit(1);
    }
}

/// Standalone mode: Run as a single agent session with normal chat output
async fn run_standalone_mode(args: &AgentArgs) -> Result<(), Box<dyn std::error::Error>> {
    info!("Running standalone agent session");

    println!("{}", format!("Cloud Coder Agent - {}", args.description).bright_blue().bold());
    println!("{}", "=".repeat(50));
    println!();

    // Create the chat provider
    let provider = if let Some(ref model) = args.model {
        OllamaProvider::with_model(model.clone())
    } else {
        OllamaProvider::cloud()
    };

    let model = args.model.clone().unwrap_or_else(|| provider.default_model().to_string());
    println!("Model: {}", model.bright_cyan());
    println!("Prompt: {}", args.prompt);
    println!();

    // Build messages
    let mut messages = Vec::new();

    // Add system prompt if provided
    if let Some(ref system) = args.system {
        messages.push(ChatMessage::system(system));
    }

    // Handle continuation
    if let Some(ref conversation_id) = args.continue_from {
        println!("{}", format!("Continuing from: {}", conversation_id).bright_black());
    }

    // Add the user prompt
    messages.push(ChatMessage::user(&args.prompt));

    // Create tool registry
    let tool_registry = ToolRegistry::new();

    // Setup shutdown handler
    let shutdown_flag = setup_shutdown_handler();

    println!("{}", "Processing your request...".yellow());
    println!();

    // Run the chat completion
    let mut total_tokens = 0u64;
    let mut tool_uses = 0u64;
    let progress = create_progress_bar(&args.description);

    let result = run_chat_completion(
        &provider,
        &model,
        messages,
        &args.system,
        &tool_registry,
        args.timeout_ms,
        shutdown_flag,
        &progress,
        &mut total_tokens,
        &mut tool_uses,
    ).await;

    progress.finish_and_clear();

    match result {
        Ok(response) => {
            println!("{}", response);
            println!();
            println!("{}", format!("Completed: {} tokens, {} tool uses", total_tokens, tool_uses).bright_black());
        }
        Err(e) => {
            eprintln!("{}", format!("Error: {}", e).red());
            return Err(e);
        }
    }

    Ok(())
}

/// Run chat completion with tool support
#[allow(clippy::too_many_arguments)]
async fn run_chat_completion(
    provider: &OllamaProvider,
    model: &str,
    mut messages: Vec<ChatMessage>,
    system_prompt: &Option<String>,
    tool_registry: &ToolRegistry,
    timeout_ms: u64,
    shutdown_flag: Arc<AtomicBool>,
    progress: &ProgressBar,
    total_tokens: &mut u64,
    tool_uses: &mut u64,
) -> Result<String, Box<dyn std::error::Error>> {
    const MAX_TOOL_ROUNDS: usize = 20;
    let mut tool_rounds = 0;
    let start_time = Instant::now();
    let timeout_duration = Duration::from_millis(timeout_ms);

    let mut accumulated_response = String::new();

    loop {
        // Check for shutdown signal
        if shutdown_flag.load(Ordering::Relaxed) {
            warn!("Shutdown signal received, stopping execution");
            return Err("Interrupted by signal".into());
        }

        // Check timeout
        if start_time.elapsed() > timeout_duration {
            warn!("Timeout reached after {}ms", timeout_ms);
            return Err(format!("Timeout after {}ms", timeout_ms).into());
        }

        // Build request
        let tools = tool_registry.get_tool_definitions();
        let request = ChatRequest {
            model: model.to_string(),
            messages: messages.clone(),
            options: None,
            system: system_prompt.clone(),
            stream: false,
            tools: Some(tools),
        };

        progress.set_message("Calling model...");

        // Call the provider
        let response = provider.chat(request).await?;
        let message = response.message;

        // Extract content and tool calls
        let (content, tool_calls) = extract_content_and_tool_calls(&message);

        // Update metrics
        *total_tokens += response.usage.total_tokens;

        // Accumulate response
        if !content.is_empty() {
            accumulated_response.push_str(&content);
            accumulated_response.push('\n');
        }

        // If no tool calls, we're done
        if tool_calls.is_empty() {
            messages.push(message);
            break;
        }

        // Handle tool calls
        tool_rounds += 1;
        if tool_rounds > MAX_TOOL_ROUNDS {
            warn!("Maximum tool call rounds reached");
            return Err("Maximum tool call rounds reached".into());
        }

        // Add assistant message to history
        messages.push(message);

        // Execute each tool call
        for (tool_id, tool_name, tool_input) in tool_calls {
            *tool_uses += 1;
            progress.set_message(format!("Executing: {}", tool_name));

            debug!("Executing tool: {} with input: {}", tool_name, tool_input);

            let result = tool_registry.execute(&tool_name, tool_input.clone()).await;

            let (result_content, is_error) = match result {
                Ok(output) => (output, false),
                Err(e) => (format!("Error: {}", e), true),
            };

            debug!("Tool result: {}", result_content);

            // Add tool result to messages
            messages.push(ChatMessage::tool_result(tool_id, result_content, is_error));
        }
    }

    Ok(accumulated_response.trim().to_string())
}

/// Extract content and tool calls from a message
fn extract_content_and_tool_calls(
    message: &ChatMessage,
) -> (String, Vec<(String, String, serde_json::Value)>) {
    match &message.content {
        cloudcoder_provider::MessageContent::Text(text) => {
            (text.clone(), Vec::new())
        }
        cloudcoder_provider::MessageContent::Blocks(blocks) => {
            let mut text = String::new();
            let mut calls = Vec::new();

            for block in blocks {
                match block {
                    ContentBlock::Text { text: t } => {
                        text.push_str(t);
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        calls.push((id.clone(), name.clone(), input.clone()));
                    }
                    _ => {}
                }
            }
            (text, calls)
        }
    }
}

/// Extract a summary from the response
fn extract_summary(response: &str) -> String {
    // Try to get the first line or first 100 chars as summary
    let first_line = response.lines().next().unwrap_or(response);
    if first_line.len() > 100 {
        format!("{}...", &first_line[..100])
    } else {
        first_line.to_string()
    }
}

/// Output XML notification to stdout
fn output_xml_notification(worker_id: &str, result: &WorkerResult) {
    let status = if result.success {
        TaskStatus::Completed
    } else {
        TaskStatus::Failed
    };

    let notification = TaskNotification {
        task_id: worker_id.to_string(),
        status,
        summary: result.summary.clone(),
        result: result.result.clone(),
        usage: Some(TaskUsage {
            total_tokens: result.total_tokens,
            tool_uses: result.tool_uses,
            duration_ms: result.duration_ms,
        }),
    };

    let xml = to_xml(&notification);

    // Output to stdout (this is what the coordinator will parse)
    println!("{}", xml);
    io::stdout().flush().ok();
}

/// Setup graceful shutdown handler
fn setup_shutdown_handler() -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let flag_clone = flag.clone();

    // Spawn a task to handle signals
    tokio::spawn(async move {
        let ctrl_c = signal::ctrl_c();

        #[cfg(unix)]
        let mut sigterm = {
            use tokio::signal::unix::{signal, SignalKind};
            signal(SignalKind::terminate()).expect("Failed to install SIGTERM handler")
        };

        #[cfg(unix)]
        tokio::select! {
            _ = ctrl_c => {
                warn!("Received SIGINT (Ctrl+C)");
                flag_clone.store(true, Ordering::Relaxed);
            }
            _ = sigterm.recv() => {
                warn!("Received SIGTERM");
                flag_clone.store(true, Ordering::Relaxed);
            }
        }

        #[cfg(not(unix))]
        {
            ctrl_c.await.expect("Failed to listen for ctrl+c");
            warn!("Received Ctrl+C");
            flag_clone.store(true, Ordering::Relaxed);
        }
    });

    flag
}

/// Create a progress bar for long-running operations
fn create_progress_bar(description: &str) -> ProgressBar {
    let progress = ProgressBar::new_spinner();
    progress.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .expect("Invalid progress template")
            .tick_chars("/-\\|"),
    );
    progress.set_message(description.to_string());
    progress.enable_steady_tick(Duration::from_millis(100));
    progress
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_args_parsing() {
        let args = AgentArgs::try_parse_from([
            "agent",
            "--description", "Test task",
            "--prompt", "Do something",
        ]);

        assert!(args.is_ok());
        let args = args.unwrap();
        assert_eq!(args.description, "Test task");
        assert_eq!(args.prompt, "Do something");
        assert!(!args.is_worker);
        assert_eq!(args.timeout_ms, 300000);
    }

    #[test]
    fn test_agent_args_worker_mode() {
        let args = AgentArgs::try_parse_from([
            "agent",
            "--id", "agent-123",
            "--description", "Worker task",
            "--prompt", "Do work",
            "--is-worker",
        ]);

        assert!(args.is_ok());
        let args = args.unwrap();
        assert!(args.is_worker);
        assert_eq!(args.id, Some("agent-123".to_string()));
    }

    #[test]
    fn test_agent_args_with_model() {
        let args = AgentArgs::try_parse_from([
            "agent",
            "--description", "Test",
            "--prompt", "Do something",
            "--model", "claude-3-opus",
        ]);

        assert!(args.is_ok());
        let args = args.unwrap();
        assert_eq!(args.model, Some("claude-3-opus".to_string()));
    }

    #[test]
    fn test_agent_args_with_system_prompt() {
        let args = AgentArgs::try_parse_from([
            "agent",
            "--description", "Test",
            "--prompt", "Do something",
            "--system", "You are a helpful assistant",
        ]);

        assert!(args.is_ok());
        let args = args.unwrap();
        assert_eq!(args.system, Some("You are a helpful assistant".to_string()));
    }

    #[test]
    fn test_agent_args_with_timeout() {
        let args = AgentArgs::try_parse_from([
            "agent",
            "--description", "Test",
            "--prompt", "Do something",
            "--timeout-ms", "60000",
        ]);

        assert!(args.is_ok());
        let args = args.unwrap();
        assert_eq!(args.timeout_ms, 60000);
    }

    #[test]
    fn test_agent_args_continue_from() {
        let args = AgentArgs::try_parse_from([
            "agent",
            "--description", "Continued task",
            "--prompt", "Continue",
            "--continue-from", "conv-123",
        ]);

        assert!(args.is_ok());
        let args = args.unwrap();
        assert_eq!(args.continue_from, Some("conv-123".to_string()));
    }

    #[test]
    fn test_worker_result_creation() {
        let result = WorkerResult::new("Task completed")
            .with_result("Detailed result")
            .with_usage(1000, 10, 5000);

        assert_eq!(result.summary, "Task completed");
        assert_eq!(result.result, Some("Detailed result".to_string()));
        assert_eq!(result.total_tokens, 1000);
        assert_eq!(result.tool_uses, 10);
        assert_eq!(result.duration_ms, 5000);
        assert!(result.success);
    }

    #[test]
    fn test_worker_result_failed() {
        let result = WorkerResult::new("Task failed").failed();

        assert!(!result.success);
    }

    #[test]
    fn test_extract_summary_short() {
        let response = "This is a short response.";
        let summary = extract_summary(response);
        assert_eq!(summary, "This is a short response.");
    }

    #[test]
    fn test_extract_summary_long() {
        let response = "This is a very long response that exceeds one hundred characters and should be truncated with an ellipsis at the end.";
        let summary = extract_summary(response);
        assert!(summary.len() <= 103); // 100 chars + "..."
        assert!(summary.ends_with("..."));
    }

    #[test]
    fn test_extract_summary_multiline() {
        let response = "First line\nSecond line\nThird line";
        let summary = extract_summary(response);
        assert_eq!(summary, "First line");
    }

    #[test]
    fn test_output_xml_notification() {
        let result = WorkerResult::new("Test summary")
            .with_result("Test result")
            .with_usage(100, 5, 1000);

        // We can't easily capture stdout in tests, but we can verify the function doesn't panic
        output_xml_notification("agent-test", &result);
    }

    #[test]
    fn test_create_progress_bar() {
        let progress = create_progress_bar("Test description");
        assert!(!progress.is_finished());
        progress.finish();
    }
}