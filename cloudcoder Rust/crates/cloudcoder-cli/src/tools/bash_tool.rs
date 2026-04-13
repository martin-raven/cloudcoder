//! Bash tool for shell command execution

use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::{Duration, Instant};

use cloudcoder_core::CloudCoderError;

/// Bash tool input schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashToolInput {
    /// Command to execute
    pub command: String,
    /// Working directory (optional)
    pub cwd: Option<String>,
    /// Timeout in milliseconds (default: 120000)
    pub timeout_ms: Option<u64>,
    /// Environment variables
    pub env: Option<std::collections::HashMap<String, String>>,
}

/// Bash tool output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashToolOutput {
    /// Exit code
    pub exit_code: i32,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Execution time in milliseconds
    pub duration_ms: u64,
    /// Whether the command was killed due to timeout
    pub timed_out: bool,
}

/// Bash tool for executing shell commands
pub struct BashTool {
    default_timeout_ms: u64,
}

impl BashTool {
    pub fn new() -> Self {
        Self {
            default_timeout_ms: 120_000, // 2 minutes
        }
    }

    pub fn name(&self) -> &str {
        "BashTool"
    }

    pub fn description(&self) -> &str {
        "Execute bash shell commands with optional timeout and environment variables"
    }

    pub async fn execute(&self, input: BashToolInput) -> Result<BashToolOutput, CloudCoderError> {
        let timeout_ms = input.timeout_ms.unwrap_or(self.default_timeout_ms);
        let start = Instant::now();

        // Build the command
        let mut cmd = if cfg!(target_os = "windows") {
            Command::new("cmd")
        } else {
            Command::new("bash")
        };

        if cfg!(target_os = "windows") {
            cmd.arg("/C").arg(&input.command);
        } else {
            cmd.arg("-c").arg(&input.command.clone());
        }

        // Set working directory
        if let Some(cwd) = &input.cwd {
            cmd.current_dir(cwd);
        }

        // Set environment variables
        if let Some(env) = &input.env {
            for (key, value) in env {
                cmd.env(key, value);
            }
        }

        // Execute with timeout using tokio::process
        let output = tokio::time::timeout(
            Duration::from_millis(timeout_ms),
            async {
                cmd.output()
            }
        )
        .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match output {
            Ok(Ok(output)) => {
                Ok(BashToolOutput {
                    exit_code: output.status.code().unwrap_or(-1),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    duration_ms,
                    timed_out: false,
                })
            }
            Ok(Err(e)) => {
                Err(CloudCoderError::ToolExecution {
                    message: format!("Failed to execute command: {}", e),
                    tool_name: "BashTool".to_string(),
                    tool_input: Some(input.command),
                })
            }
            Err(_) => {
                Ok(BashToolOutput {
                    exit_code: -1,
                    stdout: String::new(),
                    stderr: format!("Command timed out after {}ms", timeout_ms),
                    duration_ms,
                    timed_out: true,
                })
            }
        }
    }
}

impl Default for BashTool {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::tools::Tool for BashTool {
    fn name(&self) -> &str {
        "BashTool"
    }

    fn description(&self) -> &str {
        "Execute bash shell commands with optional timeout and environment variables"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_echo_command() {
        let tool = BashTool::new();
        let result = tool.execute(BashToolInput {
            command: "echo hello".to_string(),
            cwd: None,
            timeout_ms: Some(5000),
            env: None,
        }).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.trim() == "hello");
    }
}