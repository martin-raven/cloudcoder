//! Tool registry for managing all tools

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use cloudcoder_core::CloudCoderError;
use cloudcoder_provider::ToolDefinition;

use super::{BashTool, FileTool, GitTool, HttpTool};

/// Trait for tools in the registry
#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
}

/// Tool registry for managing all available tools
pub struct ToolRegistry {
    tools: RwLock<HashMap<String, Arc<dyn Tool>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut tools: HashMap<String, Arc<dyn Tool>> = HashMap::new();

        // Register built-in tools
        let bash = Arc::new(BashTool::new());
        tools.insert(bash.name().to_string(), bash);

        let file = Arc::new(FileTool::new());
        tools.insert(file.name().to_string(), file);

        let git = Arc::new(GitTool::new());
        tools.insert(git.name().to_string(), git);

        let http = Arc::new(HttpTool::new());
        tools.insert(http.name().to_string(), http);

        Self {
            tools: RwLock::new(tools),
        }
    }

    /// Get a tool by name
    pub async fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        let tools = self.tools.read().await;
        tools.get(name).cloned()
    }

    /// List all available tools
    pub async fn list(&self) -> Vec<&'static str> {
        vec!["BashTool", "FileTool", "GitTool", "HttpTool"]
    }

    /// Register a new tool
    pub async fn register(&self, tool: Arc<dyn Tool>) -> Result<(), CloudCoderError> {
        let mut tools = self.tools.write().await;

        if tools.contains_key(tool.name()) {
            return Err(CloudCoderError::Config(format!(
                "Tool '{}' already registered",
                tool.name()
            )));
        }

        tools.insert(tool.name().to_string(), tool);
        Ok(())
    }

    /// Unregister a tool
    pub async fn unregister(&self, name: &str) -> Result<(), CloudCoderError> {
        let mut tools = self.tools.write().await;

        if tools.remove(name).is_none() {
            return Err(CloudCoderError::Config(format!(
                "Tool '{}' not found",
                name
            )));
        }

        Ok(())
    }

    /// Get tool info as JSON
    pub async fn get_tool_info(&self) -> Vec<serde_json::Value> {
        let tools = self.tools.read().await;
        tools
            .values()
            .map(|t| {
                serde_json::json!({
                    "name": t.name(),
                    "description": t.description(),
                })
            })
            .collect()
    }

    /// Get tool definitions for LLM function calling
    pub fn get_tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "BashTool".to_string(),
                description: "Execute bash shell commands with optional timeout and environment variables".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The command to execute"
                        },
                        "cwd": {
                            "type": "string",
                            "description": "Working directory (optional)"
                        },
                        "timeout_ms": {
                            "type": "integer",
                            "description": "Timeout in milliseconds (default: 120000)"
                        }
                    },
                    "required": ["command"]
                }),
            },
            ToolDefinition {
                name: "FileTool".to_string(),
                description: "Perform file system operations: read, write, delete, list directories".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file or directory"
                        },
                        "operation": {
                            "type": "string",
                            "enum": ["read", "write", "append", "delete", "create_dir", "list_dir", "exists", "copy", "move", "metadata"],
                            "description": "Operation to perform"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content for write/append operations"
                        }
                    },
                    "required": ["path", "operation"]
                }),
            },
            ToolDefinition {
                name: "GitTool".to_string(),
                description: "Perform Git operations: status, diff, commit, branch, push, pull".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "operation": {
                            "type": "string",
                            "enum": ["status", "diff", "log", "current_branch", "list_branches", "stage", "unstage", "commit", "create_branch", "switch_branch", "pull", "push", "fetch", "remotes", "blame", "show"],
                            "description": "Git operation to perform"
                        },
                        "cwd": {
                            "type": "string",
                            "description": "Working directory (defaults to current directory)"
                        }
                    },
                    "required": ["operation"]
                }),
            },
            ToolDefinition {
                name: "HttpTool".to_string(),
                description: "Make HTTP requests with customizable headers, body, and timeout".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "URL to request"
                        },
                        "method": {
                            "type": "string",
                            "enum": ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"],
                            "description": "HTTP method (default: GET)"
                        },
                        "headers": {
                            "type": "object",
                            "description": "Request headers"
                        },
                        "body": {
                            "type": "string",
                            "description": "Request body"
                        },
                        "timeout_ms": {
                            "type": "integer",
                            "description": "Timeout in milliseconds (default: 30000)"
                        }
                    },
                    "required": ["url"]
                }),
            },
        ]
    }

    /// Execute a tool by name with the given input
    pub async fn execute(&self, name: &str, input: serde_json::Value) -> Result<String, String> {
        match name {
            "BashTool" => {
                let command = input.get("command")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'command' field")?;

                let cwd = input.get("cwd")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let timeout_ms = input.get("timeout_ms")
                    .and_then(|v| v.as_u64());

                let tool = BashTool::new();
                let tool_input = super::BashToolInput {
                    command: command.to_string(),
                    cwd,
                    timeout_ms,
                    env: None,
                };

                let result = tool.execute(tool_input).await
                    .map_err(|e| format!("Tool execution error: {}", e))?;

                let output = if result.timed_out {
                    format!("Command timed out after {}ms\nStderr: {}", result.duration_ms, result.stderr)
                } else {
                    format!(
                        "Exit code: {}\nDuration: {}ms\nStdout: {}\nStderr: {}",
                        result.exit_code,
                        result.duration_ms,
                        result.stdout.trim(),
                        result.stderr.trim()
                    )
                };

                Ok(output)
            }
            "FileTool" => {
                let path = input.get("path")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'path' field")?;

                let operation_str = input.get("operation")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'operation' field")?;

                let operation = match operation_str {
                    "read" => super::FileOperation::Read,
                    "write" => {
                        let content = input.get("content")
                            .and_then(|v| v.as_str())
                            .ok_or("Missing 'content' field for write operation")?;
                        super::FileOperation::Write { content: content.to_string() }
                    }
                    "append" => {
                        let content = input.get("content")
                            .and_then(|v| v.as_str())
                            .ok_or("Missing 'content' field for append operation")?;
                        super::FileOperation::Append { content: content.to_string() }
                    }
                    "delete" => super::FileOperation::Delete,
                    "create_dir" => super::FileOperation::CreateDir,
                    "list_dir" => super::FileOperation::ListDir,
                    "exists" => super::FileOperation::Exists,
                    "copy" => {
                        let destination = input.get("destination")
                            .and_then(|v| v.as_str())
                            .ok_or("Missing 'destination' field for copy operation")?;
                        super::FileOperation::Copy { destination: destination.to_string() }
                    }
                    "move" => {
                        let destination = input.get("destination")
                            .and_then(|v| v.as_str())
                            .ok_or("Missing 'destination' field for move operation")?;
                        super::FileOperation::Move { destination: destination.to_string() }
                    }
                    "metadata" => super::FileOperation::Metadata,
                    _ => return Err(format!("Unknown operation: {}", operation_str)),
                };

                let tool = FileTool::new();
                let tool_input = super::FileToolInput {
                    path: path.to_string(),
                    operation,
                    encoding: "utf-8".to_string(),
                    max_size: None,
                };

                let result = tool.execute(tool_input).await
                    .map_err(|e| format!("Tool execution error: {}", e))?;

                let output = if result.success {
                    if let Some(content) = result.content {
                        format!("Success: {}\nContent:\n{}", result.message, content)
                    } else if let Some(entries) = result.entries {
                        let entries_str = entries.iter()
                            .map(|e| format!("{} {} ({})", if e.is_dir { "DIR" } else { "FILE" }, e.name, e.path))
                            .collect::<Vec<_>>()
                            .join("\n");
                        format!("Success: {}\nEntries:\n{}", result.message, entries_str)
                    } else {
                        result.message
                    }
                } else {
                    format!("Error: {}", result.message)
                };

                Ok(output)
            }
            "GitTool" => {
                let operation_str = input.get("operation")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'operation' field")?;

                let cwd = input.get("cwd")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let operation = match operation_str {
                    "status" => super::GitOperation::Status,
                    "diff" => {
                        let staged = input.get("staged")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        super::GitOperation::Diff { staged }
                    }
                    "log" => {
                        let max_count = input.get("max_count")
                            .and_then(|v| v.as_u64())
                            .map(|n| n as usize);
                        super::GitOperation::Log { max_count }
                    }
                    "current_branch" => super::GitOperation::CurrentBranch,
                    "list_branches" => super::GitOperation::ListBranches,
                    "stage" => {
                        let files = input.get("files")
                            .and_then(|v| v.as_array())
                            .ok_or("Missing 'files' field for stage operation")?
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect();
                        super::GitOperation::Stage { files }
                    }
                    "unstage" => {
                        let files = input.get("files")
                            .and_then(|v| v.as_array())
                            .ok_or("Missing 'files' field for unstage operation")?
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect();
                        super::GitOperation::Unstage { files }
                    }
                    "commit" => {
                        let message = input.get("message")
                            .and_then(|v| v.as_str())
                            .ok_or("Missing 'message' field for commit operation")?;
                        super::GitOperation::Commit { message: message.to_string() }
                    }
                    "create_branch" => {
                        let name = input.get("name")
                            .and_then(|v| v.as_str())
                            .ok_or("Missing 'name' field for create_branch operation")?;
                        super::GitOperation::CreateBranch { name: name.to_string() }
                    }
                    "switch_branch" => {
                        let name = input.get("name")
                            .and_then(|v| v.as_str())
                            .ok_or("Missing 'name' field for switch_branch operation")?;
                        super::GitOperation::SwitchBranch { name: name.to_string() }
                    }
                    "pull" => {
                        let remote = input.get("remote").and_then(|v| v.as_str()).map(|s| s.to_string());
                        let branch = input.get("branch").and_then(|v| v.as_str()).map(|s| s.to_string());
                        super::GitOperation::Pull { remote, branch }
                    }
                    "push" => {
                        let remote = input.get("remote").and_then(|v| v.as_str()).map(|s| s.to_string());
                        let branch = input.get("branch").and_then(|v| v.as_str()).map(|s| s.to_string());
                        let set_upstream = input.get("set_upstream")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        super::GitOperation::Push { remote, branch, set_upstream }
                    }
                    "fetch" => {
                        let remote = input.get("remote").and_then(|v| v.as_str()).map(|s| s.to_string());
                        super::GitOperation::Fetch { remote }
                    }
                    "remotes" => super::GitOperation::Remotes,
                    "blame" => {
                        let file = input.get("file")
                            .and_then(|v| v.as_str())
                            .ok_or("Missing 'file' field for blame operation")?;
                        super::GitOperation::Blame { file: file.to_string() }
                    }
                    "show" => {
                        let reference = input.get("reference")
                            .and_then(|v| v.as_str())
                            .ok_or("Missing 'reference' field for show operation")?;
                        super::GitOperation::Show { reference: reference.to_string() }
                    }
                    _ => return Err(format!("Unknown operation: {}", operation_str)),
                };

                let tool = GitTool::new();
                let tool_input = super::GitToolInput {
                    cwd,
                    operation,
                };

                let result = tool.execute(tool_input).await
                    .map_err(|e| format!("Tool execution error: {}", e))?;

                let output = if result.success {
                    if let Some(data) = result.data {
                        format!("Success: {}\nData: {}", result.output, serde_json::to_string(&data).unwrap_or_default())
                    } else {
                        result.output
                    }
                } else {
                    format!("Error: {}", result.error.unwrap_or_else(|| result.output))
                };

                Ok(output)
            }
            "HttpTool" => {
                let url = input.get("url")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'url' field")?;

                let method = match input.get("method").and_then(|v| v.as_str()) {
                    Some("GET") => super::HttpMethod::Get,
                    Some("POST") => super::HttpMethod::Post,
                    Some("PUT") => super::HttpMethod::Put,
                    Some("PATCH") => super::HttpMethod::Patch,
                    Some("DELETE") => super::HttpMethod::Delete,
                    Some("HEAD") => super::HttpMethod::Head,
                    Some("OPTIONS") => super::HttpMethod::Options,
                    _ => super::HttpMethod::Get,
                };

                let headers = input.get("headers")
                    .and_then(|v| v.as_object())
                    .map(|obj| {
                        obj.iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    });

                let body = input.get("body")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let timeout_ms = input.get("timeout_ms")
                    .and_then(|v| v.as_u64());

                let follow_redirects = input.get("follow_redirects")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);

                let verify_tls = input.get("verify_tls")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);

                let tool = HttpTool::new();
                let tool_input = super::HttpToolInput {
                    url: url.to_string(),
                    method,
                    headers,
                    body,
                    query: None,
                    timeout_ms,
                    follow_redirects,
                    verify_tls,
                };

                let result = tool.execute(tool_input).await
                    .map_err(|e| format!("Tool execution error: {}", e))?;

                let output = if result.success {
                    format!(
                        "Status: {}\nDuration: {}ms\nHeaders: {:?}\nBody: {}",
                        result.status_code,
                        result.duration_ms,
                        result.headers,
                        result.body.unwrap_or_default()
                    )
                } else {
                    format!(
                        "Status: {}\nError: {}",
                        result.status_code,
                        result.error.unwrap_or_else(|| "Unknown error".to_string())
                    )
                };

                Ok(output)
            }
            _ => Err(format!("Unknown tool: {}", name)),
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_creation() {
        let registry = ToolRegistry::new();
        let tools = registry.list().await;
        assert!(!tools.is_empty());
    }

    #[tokio::test]
    async fn test_get_tool() {
        let registry = ToolRegistry::new();
        let tool = registry.get("BashTool").await;
        assert!(tool.is_some());
    }
}