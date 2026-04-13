//! Provider trait definition

use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;
use futures_util::stream::BoxStream;

use cloudcoder_core::CloudCoderError;

use crate::types::{CompletionResult, GenerationOptions, ModelInfo, ProviderConfig, ProviderStatus, StreamChunk};
use crate::message::{ChatMessage, ChatRequest, ChatResponse};

/// Provider trait for LLM implementations
#[async_trait]
pub trait Provider: Send + Sync {
    /// Get provider name
    fn name(&self) -> &str;

    /// Check if provider is available
    async fn is_available(&self) -> bool;

    /// Get provider status
    async fn status(&self) -> ProviderStatus;

    /// List available models
    async fn list_models(&self) -> Result<Vec<ModelInfo>, CloudCoderError>;

    /// Get model info
    async fn get_model(&self, model_id: &str) -> Result<Option<ModelInfo>, CloudCoderError>;

    /// Complete a prompt (simple completion)
    async fn complete(
        &self,
        prompt: &str,
        model: Option<&str>,
        options: Option<GenerationOptions>,
    ) -> Result<CompletionResult, CloudCoderError>;

    /// Chat completion
    async fn chat(
        &self,
        request: ChatRequest,
    ) -> Result<ChatResponse, CloudCoderError>;

    /// Stream chat completion
    fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Pin<Box<dyn Future<Output = Result<futures_util::stream::BoxStream<'static, Result<StreamChunk, CloudCoderError>>, CloudCoderError>> + Send + '_>>;

    /// Count tokens in messages
    async fn count_tokens(&self, messages: &[ChatMessage]) -> Result<u64, CloudCoderError>;

    /// Count tokens in text
    async fn count_text_tokens(&self, text: &str) -> Result<u64, CloudCoderError>;
}

/// Provider factory trait
pub trait ProviderFactory: Send + Sync {
    /// Create a new provider instance
    fn create(&self, config: ProviderConfig) -> Result<Box<dyn Provider>, CloudCoderError>;

    /// Get provider name
    fn name(&self) -> &str;
}

/// Provider health check
#[derive(Debug, Clone)]
pub struct ProviderHealth {
    pub name: String,
    pub healthy: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
}

/// Provider metrics
#[derive(Debug, Clone, Default)]
pub struct ProviderMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub total_tokens: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_duration_ms: u64,
    pub average_latency_ms: f64,
}