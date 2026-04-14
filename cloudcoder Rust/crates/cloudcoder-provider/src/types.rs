//! Common types for the provider layer

use serde::{Deserialize, Serialize};

/// Model capabilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// Supports streaming responses
    pub streaming: bool,
    /// Supports function/tool calling
    pub tools: bool,
    /// Supports vision/image input
    pub vision: bool,
    /// Maximum context length
    pub max_context: usize,
    /// Supports prompt caching
    pub prompt_caching: bool,
}

/// Model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Provider name
    pub provider: String,
    /// Capabilities
    pub capabilities: ModelCapabilities,
}

/// Token usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens used
    pub prompt_tokens: u64,
    /// Output tokens generated
    pub completion_tokens: u64,
    /// Total tokens
    pub total_tokens: u64,
}

/// Generation options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationOptions {
    /// Temperature (0.0 - 2.0)
    pub temperature: Option<f32>,
    /// Top-p sampling
    pub top_p: Option<f32>,
    /// Top-k sampling
    pub top_k: Option<u32>,
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// Stop sequences
    pub stop: Option<Vec<String>>,
    /// Seed for reproducibility
    pub seed: Option<i64>,
    /// Frequency penalty
    pub frequency_penalty: Option<f32>,
    /// Presence penalty
    pub presence_penalty: Option<f32>,
}

impl Default for GenerationOptions {
    fn default() -> Self {
        Self {
            temperature: Some(0.7),
            top_p: Some(0.9),
            top_k: None,
            max_tokens: Some(4096),
            stop: None,
            seed: None,
            frequency_penalty: None,
            presence_penalty: None,
        }
    }
}

/// Completion result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResult {
    /// Generated text content
    pub content: String,
    /// Model used
    pub model: String,
    /// Token usage
    pub usage: TokenUsage,
    /// Finish reason
    pub finish_reason: FinishReason,
    /// Total generation time in milliseconds
    pub duration_ms: u64,
    /// Whether response was from cache
    pub from_cache: bool,
}

/// Why generation stopped
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// Normal completion
    Stop,
    /// Max tokens reached
    Length,
    /// Stop sequence encountered
    StopSequence,
    /// Content filtered
    ContentFilter,
    /// Tool call requested
    ToolCall,
    /// Error occurred
    Error,
    /// Cancelled
    Cancelled,
}

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    /// Base URL for API
    pub base_url: Option<String>,
    /// API key (if required)
    pub api_key: Option<String>,
    /// Request timeout in milliseconds
    pub timeout_ms: Option<u64>,
    /// Maximum retries
    pub max_retries: Option<u32>,
    /// Default model to use
    pub default_model: Option<String>,
    /// Default generation options
    pub default_options: Option<GenerationOptions>,
}

/// Streaming chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    /// Chunk content
    pub content: String,
    /// Thinking content (model's internal reasoning)
    #[serde(default)]
    pub thinking: String,
    /// Whether this is the final chunk
    pub is_final: bool,
    /// Token usage (on final chunk)
    pub usage: Option<TokenUsage>,
    /// Finish reason (on final chunk)
    pub finish_reason: Option<FinishReason>,
}

/// Provider status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    /// Provider name
    pub name: String,
    /// Whether provider is available
    pub available: bool,
    /// Response time in milliseconds (if checked)
    pub response_time_ms: Option<u64>,
    /// Number of available models
    pub model_count: usize,
    /// Last check timestamp
    pub last_check: u64,
    /// Error message if unavailable
    pub error: Option<String>,
}