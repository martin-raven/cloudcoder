//! Message types for chat conversations

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Chat message role
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Content block for multimodal messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Text content
    Text { text: String },
    /// Image content (base64)
    Image { source: ImageSource },
    /// Tool use request
    ToolUse { id: String, name: String, input: serde_json::Value },
    /// Tool result
    ToolResult { tool_use_id: String, content: String, is_error: bool },
}

/// Image source for multimodal messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    /// Image encoding type
    #[serde(rename = "type")]
    pub encoding_type: String,
    /// Media type (e.g., "image/png")
    pub media_type: String,
    /// Base64 encoded image data
    pub data: String,
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Message role
    pub role: MessageRole,
    /// Message content (can be string or structured)
    pub content: MessageContent,
    /// Message ID (for tool responses)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Name (for tool messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Additional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
}

/// Message content type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Simple text content
    Text(String),
    /// Structured content blocks
    Blocks(Vec<ContentBlock>),
}

impl MessageContent {
    /// Create text content
    pub fn text(content: impl Into<String>) -> Self {
        MessageContent::Text(content.into())
    }

    /// Get content as text (for simple messages)
    pub fn as_text(&self) -> Option<&str> {
        match self {
            MessageContent::Text(t) => Some(t),
            MessageContent::Blocks(blocks) => {
                // Find first text block
                for block in blocks {
                    if let ContentBlock::Text { text } = block {
                        return Some(text);
                    }
                }
                None
            }
        }
    }

    /// Get all text content combined
    pub fn to_text(&self) -> String {
        match self {
            MessageContent::Text(t) => t.clone(),
            MessageContent::Blocks(blocks) => {
                blocks.iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("")
            }
        }
    }
}

/// Chat request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    /// Model to use
    pub model: String,
    /// Messages in the conversation
    pub messages: Vec<ChatMessage>,
    /// Generation options
    #[serde(flatten)]
    pub options: Option<super::types::GenerationOptions>,
    /// System prompt (if not in messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Stream response
    #[serde(default)]
    pub stream: bool,
    /// Tools available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
}

/// Tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Input schema (JSON Schema)
    pub input_schema: serde_json::Value,
}

/// Chat response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    /// Response ID
    pub id: String,
    /// Model used
    pub model: String,
    /// Generated message
    pub message: ChatMessage,
    /// Token usage
    pub usage: super::types::TokenUsage,
    /// Finish reason
    pub finish_reason: super::types::FinishReason,
    /// Response time in milliseconds
    pub duration_ms: u64,
}

impl ChatMessage {
    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: MessageContent::text(content),
            id: None,
            name: None,
            metadata: None,
        }
    }

    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: MessageContent::text(content),
            id: None,
            name: None,
            metadata: None,
        }
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: MessageContent::text(content),
            id: None,
            name: None,
            metadata: None,
        }
    }

    /// Create a tool result message
    pub fn tool_result(tool_use_id: impl Into<String>, content: impl Into<String>, is_error: bool) -> Self {
        Self {
            role: MessageRole::Tool,
            content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                tool_use_id: tool_use_id.into(),
                content: content.into(),
                is_error,
            }]),
            id: None,
            name: None,
            metadata: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = ChatMessage::user("Hello, world!");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content.as_text(), Some("Hello, world!"));
    }

    #[test]
    fn test_message_serialization() {
        let msg = ChatMessage::system("You are a helpful assistant.");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("system"));
        assert!(json.contains("You are a helpful assistant"));
    }
}