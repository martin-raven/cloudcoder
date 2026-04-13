//! Git tool for Git operations

use serde::{Deserialize, Serialize};
use std::process::Command;
use std::path::PathBuf;

use cloudcoder_core::CloudCoderError;

/// Git tool operation types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitOperation {
    /// Get current status
    Status,
    /// Get diff
    Diff { staged: bool },
    /// Get log
    Log { max_count: Option<usize> },
    /// Get current branch
    CurrentBranch,
    /// List branches
    ListBranches,
    /// Stage files
    Stage { files: Vec<String> },
    /// Unstage files
    Unstage { files: Vec<String> },
    /// Create commit
    Commit { message: String },
    /// Create branch
    CreateBranch { name: String },
    /// Switch branch
    SwitchBranch { name: String },
    /// Pull from remote
    Pull { remote: Option<String>, branch: Option<String> },
    /// Push to remote
    Push { remote: Option<String>, branch: Option<String>, set_upstream: bool },
    /// Fetch from remote
    Fetch { remote: Option<String> },
    /// Get list of remotes
    Remotes,
    /// Get file blame
    Blame { file: String },
    /// Show specific commit
    Show { reference: String },
}

/// Git tool input schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitToolInput {
    /// Working directory (defaults to current directory)
    pub cwd: Option<String>,
    /// Operation to perform
    pub operation: GitOperation,
}

/// Git tool output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitToolOutput {
    /// Whether the operation succeeded
    pub success: bool,
    /// Output content
    pub output: String,
    /// Error output
    pub error: Option<String>,
    /// Structured data (parsed when applicable)
    pub data: Option<serde_json::Value>,
}

/// Git tool for Git operations
pub struct GitTool;

impl GitTool {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        "GitTool"
    }

    pub fn description(&self) -> &str {
        "Perform Git operations: status, diff, commit, branch, push, pull"
    }

    pub async fn execute(&self, input: GitToolInput) -> Result<GitToolOutput, CloudCoderError> {
        let cwd = input.cwd.as_ref()
            .map(|p| PathBuf::from(p))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        match &input.operation {
            GitOperation::Status => self.run_git(&cwd, &["status", "--porcelain"]).await,
            GitOperation::Diff { staged } => {
                let args = if *staged {
                    vec!["diff", "--cached"]
                } else {
                    vec!["diff"]
                };
                self.run_git(&cwd, &args).await
            }
            GitOperation::Log { max_count } => {
                let limit = max_count.map(|c| format!("-{}", c));
                let mut args = vec!["log", "--oneline"];
                if let Some(ref limit_arg) = limit {
                    args.push(limit_arg.as_str());
                }
                self.run_git(&cwd, &args).await
            }
            GitOperation::CurrentBranch => self.run_git(&cwd, &["branch", "--show-current"]).await,
            GitOperation::ListBranches => self.run_git(&cwd, &["branch", "-a"]).await,
            GitOperation::Stage { files } => {
                let mut args = vec!["add"];
                for f in files {
                    args.push(f.as_str());
                }
                self.run_git(&cwd, &args).await
            }
            GitOperation::Unstage { files } => {
                let mut args = vec!["reset", "HEAD", "--"];
                for f in files {
                    args.push(f.as_str());
                }
                self.run_git(&cwd, &args).await
            }
            GitOperation::Commit { message } => {
                self.run_git(&cwd, &["commit", "-m", message]).await
            }
            GitOperation::CreateBranch { name } => {
                self.run_git(&cwd, &["branch", name]).await
            }
            GitOperation::SwitchBranch { name } => {
                self.run_git(&cwd, &["checkout", name]).await
            }
            GitOperation::Pull { remote, branch } => {
                let mut args = vec!["pull"];
                if let Some(r) = remote {
                    args.push(r.as_str());
                }
                if let Some(b) = branch {
                    args.push(b.as_str());
                }
                self.run_git(&cwd, &args).await
            }
            GitOperation::Push { remote, branch, set_upstream } => {
                let mut args = vec!["push"];
                if *set_upstream {
                    args.push("-u");
                }
                if let Some(r) = remote {
                    args.push(r.as_str());
                }
                if let Some(b) = branch {
                    args.push(b.as_str());
                }
                self.run_git(&cwd, &args).await
            }
            GitOperation::Fetch { remote } => {
                let mut args = vec!["fetch"];
                if let Some(r) = remote {
                    args.push(r.as_str());
                }
                self.run_git(&cwd, &args).await
            }
            GitOperation::Remotes => self.run_git(&cwd, &["remote", "-v"]).await,
            GitOperation::Blame { file } => {
                self.run_git(&cwd, &["blame", file]).await
            }
            GitOperation::Show { reference } => {
                self.run_git(&cwd, &["show", reference]).await
            }
        }
    }

    async fn run_git(&self, cwd: &PathBuf, args: &[&str]) -> Result<GitToolOutput, CloudCoderError> {
        let output = tokio::task::spawn_blocking({
            let cwd = cwd.clone();
            let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            move || {
                let mut cmd = Command::new("git");
                for arg in &args {
                    cmd.arg(arg);
                }
                cmd.current_dir(&cwd).output()
            }
        })
        .await
        .map_err(|e| CloudCoderError::ToolExecution {
            message: format!("Failed to spawn git command: {}", e),
            tool_name: "GitTool".to_string(),
            tool_input: None,
        })?
        .map_err(|e| CloudCoderError::ToolExecution {
            message: format!("Failed to execute git: {}", e),
            tool_name: "GitTool".to_string(),
            tool_input: args.first().map(|s| s.to_string()),
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(GitToolOutput {
            success: output.status.success(),
            output: stdout,
            error: if stderr.is_empty() { None } else { Some(stderr) },
            data: None,
        })
    }
}

impl Default for GitTool {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::tools::Tool for GitTool {
    fn name(&self) -> &str {
        "GitTool"
    }

    fn description(&self) -> &str {
        "Perform Git operations: status, diff, commit, branch, push, pull"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_available() {
        // Just test that git is available
        let output = Command::new("git").arg("--version").output();
        assert!(output.is_ok());
    }
}