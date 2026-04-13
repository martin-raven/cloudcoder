//! Ollama provider implementation
//!
//! Uses localhost:11434 proxy which handles authentication to ollama.com transparently.
//! Both cloud and local models are accessed through the same local proxy endpoint.

use std::pin::Pin;
use std::time::Instant;

use async_trait::async_trait;
use futures_util::stream::{BoxStream, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use cloudcoder_core::CloudCoderError;

use crate::types::{CompletionResult, FinishReason, GenerationOptions, ModelCapabilities, ModelInfo, ProviderConfig, ProviderStatus, StreamChunk, TokenUsage};
use crate::message::{ChatMessage, ChatRequest, ChatResponse, ContentBlock, MessageContent, MessageRole};
use crate::provider::{Provider, ProviderMetrics};

/// Default Ollama URL (localhost proxy for both cloud and local)
pub const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

/// Ollama model information from API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModelInfo {
    pub name: String,
    pub modified_at: String,
    pub size: u64,
    pub digest: String,
    pub details: Option<OllamaModelDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaModelDetails {
    pub format: String,
    pub family: String,
    pub parameter_size: String,
    pub quantization_level: Option<String>,
}

/// Ollama API request
#[derive(Debug, Clone, Serialize)]
struct OllamaRequest {
    model: String,
    prompt: Option<String>,
    messages: Option<Vec<OllamaMessage>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw: Option<bool>,
}

/// Ollama message format
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
}

/// Ollama generation options
#[derive(Debug, Clone, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_ctx: Option<u32>,
}

impl From<GenerationOptions> for OllamaOptions {
    fn from(opts: GenerationOptions) -> Self {
        Self {
            temperature: opts.temperature,
            top_p: opts.top_p,
            top_k: opts.top_k,
            num_predict: opts.max_tokens.map(|m| m as i32),
            stop: opts.stop,
            seed: opts.seed,
            num_ctx: None,
        }
    }
}

/// Ollama API response
#[derive(Debug, Clone, Deserialize)]
struct OllamaResponse {
    model: String,
    created_at: String,
    message: Option<OllamaMessageResponse>,
    response: Option<String>,
    done: bool,
    total_duration: Option<u64>,
    prompt_eval_count: Option<u64>,
    eval_count: Option<u64>,
    context: Option<Vec<i32>>,
}

/// Ollama message response
#[derive(Debug, Clone, Deserialize)]
struct OllamaMessageResponse {
    role: String,
    content: String,
    #[serde(default)]
    thinking: String,
    #[serde(default, rename = "tool_calls")]
    tool_calls: Vec<OllamaToolCall>,
}

/// Ollama tool call
#[derive(Debug, Clone, Deserialize)]
struct OllamaToolCall {
    id: String,
    function: OllamaFunction,
}

/// Ollama function
#[derive(Debug, Clone, Deserialize)]
struct OllamaFunction {
    #[serde(default)]
    index: Option<u32>,
    name: String,
    #[serde(rename = "arguments")]
    input: serde_json::Value,
}

/// Ollama streaming response chunk
#[derive(Debug, Clone, Deserialize)]
struct OllamaStreamChunk {
    model: String,
    created_at: String,
    message: Option<OllamaMessageResponse>,
    response: Option<String>,
    done: bool,
    total_duration: Option<u64>,
    prompt_eval_count: Option<u64>,
    eval_count: Option<u64>,
    #[serde(default)]
    thinking: String,
}

/// Ollama list models response
#[derive(Debug, Clone, Deserialize)]
struct OllamaListResponse {
    models: Vec<OllamaModelInfo>,
}

/// Ollama provider
pub struct OllamaProvider {
    /// Base URL for API
    base_url: String,
    /// HTTP client
    client: reqwest::Client,
    /// Default model to use
    default_model: String,
    /// Default generation options
    default_options: GenerationOptions,
    /// Metrics
    metrics: RwLock<ProviderMetrics>,
}

impl OllamaProvider {
    /// Check if a model is a cloud model
    pub fn is_cloud_model(model: &str) -> bool {
        model.contains(":cloud") || model.contains("-cloud")
    }

    /// Create a new Ollama provider for cloud (default)
    pub fn new() -> Self {
        Self::cloud()
    }

    /// Create Ollama Cloud provider (uses localhost proxy)
    pub fn cloud() -> Self {
        Self::with_config(ProviderConfig {
            base_url: Some(DEFAULT_OLLAMA_URL.to_string()),
            api_key: None, // ollama proxy handles auth transparently
            timeout_ms: Some(120_000),
            max_retries: Some(3),
            default_model: Some("qwen3.5:397b-cloud".to_string()),
            default_options: Some(GenerationOptions::default()),
        })
    }

    /// Create local Ollama provider (same as cloud - uses localhost proxy)
    pub fn local() -> Self {
        Self::cloud()
    }

    /// Create provider with custom configuration
    pub fn with_config(config: ProviderConfig) -> Self {
        let base_url = config.base_url.unwrap_or_else(|| DEFAULT_OLLAMA_URL.to_string());
        let timeout = config.timeout_ms.unwrap_or(120_000);
        let default_model = config.default_model.unwrap_or_else(|| "qwen3.5:397b-cloud".to_string());

        let mut client_builder = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout));

        // The localhost proxy handles auth transparently
        // No explicit API key needed

        let client = client_builder
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            base_url,
            client,
            default_model,
            default_options: config.default_options.unwrap_or_default(),
            metrics: RwLock::new(ProviderMetrics::default()),
        }
    }

    /// Get the base URL
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the default model
    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    /// Build request options
    fn build_options(&self, options: Option<&GenerationOptions>) -> Option<OllamaOptions> {
        let opts = options.cloned().unwrap_or_else(|| self.default_options.clone());
        Some(opts.into())
    }

    /// Convert chat messages to Ollama format
    fn convert_messages(&self, messages: &[ChatMessage]) -> Vec<OllamaMessage> {
        messages.iter().map(|m| {
            OllamaMessage {
                role: match m.role {
                    MessageRole::System => "system",
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::Tool => "tool",
                }.to_string(),
                content: m.content.to_text(),
                images: None,
            }
        }).collect()
    }

    /// Parse finish reason from response
    fn parse_finish_reason(&self, _response: &OllamaResponse) -> FinishReason {
        FinishReason::Stop // Ollama always uses Stop for now
    }

    /// Update metrics
    async fn update_metrics(&self, success: bool, duration_ms: u64, tokens: Option<TokenUsage>) {
        let mut metrics = self.metrics.write().await;
        metrics.total_requests += 1;
        if success {
            metrics.successful_requests += 1;
        } else {
            metrics.failed_requests += 1;
        }
        metrics.total_duration_ms += duration_ms;
        metrics.average_latency_ms = metrics.total_duration_ms as f64 / metrics.total_requests as f64;

        if let Some(usage) = tokens {
            metrics.total_tokens += usage.total_tokens;
            metrics.total_input_tokens += usage.prompt_tokens;
            metrics.total_output_tokens += usage.completion_tokens;
        }
    }

    /// Make API request with retry
    async fn request_with_retry<T: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<T, CloudCoderError> {
        let url = format!("{}/api/{}", self.base_url, endpoint);

        let response = if let Some(body) = body {
            self.client.post(&url)
                .json(body)
                .send()
                .await
        } else {
            self.client.get(&url)
                .send()
                .await
        }.map_err(|e| CloudCoderError::Api(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(CloudCoderError::Api(format!(
                "API error {}: {}",
                status, text
            )));
        }

        response.json::<T>()
            .await
            .map_err(|e| CloudCoderError::Api(format!("Failed to parse response: {}", e)))
    }
}

impl Default for OllamaProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        self.client.get(&url).send().await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn status(&self) -> ProviderStatus {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let start = Instant::now();
        let available = self.is_available().await;
        let latency = start.elapsed().as_millis() as u64;

        let model_count = if available {
            self.list_models().await
                .map(|m| m.len())
                .unwrap_or(0)
        } else {
            0
        };

        ProviderStatus {
            name: "ollama".to_string(),
            available,
            response_time_ms: Some(latency),
            model_count,
            last_check: now,
            error: if available { None } else { Some("Ollama server not responding".to_string()) },
        }
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, CloudCoderError> {
        let response: OllamaListResponse = self.request_with_retry("tags", None).await?;

        let models = response.models.into_iter().map(|m| {
            let capabilities = ModelCapabilities {
                streaming: true,
                tools: m.details.as_ref().map(|d| d.family == "llama").unwrap_or(false),
                vision: m.details.as_ref().map(|d| d.family == "llava").unwrap_or(false),
                max_context: 8192,
                prompt_caching: false,
            };

            ModelInfo {
                id: m.name.clone(),
                name: m.name,
                provider: "ollama".to_string(),
                capabilities,
            }
        }).collect();

        Ok(models)
    }

    async fn get_model(&self, model_id: &str) -> Result<Option<ModelInfo>, CloudCoderError> {
        let models = self.list_models().await?;
        Ok(models.into_iter().find(|m| m.id == model_id || m.name == model_id))
    }

    async fn complete(
        &self,
        prompt: &str,
        model: Option<&str>,
        options: Option<GenerationOptions>,
    ) -> Result<CompletionResult, CloudCoderError> {
        let start = Instant::now();
        let model = model.unwrap_or(&self.default_model);

        let request = OllamaRequest {
            model: model.to_string(),
            prompt: Some(prompt.to_string()),
            messages: None,
            stream: false,
            options: self.build_options(options.as_ref()),
            format: None,
            system: None,
            raw: None,
        };

        let body = serde_json::to_value(&request)
            .map_err(|e| CloudCoderError::Api(format!("Failed to serialize request: {}", e)))?;

        let response: OllamaResponse = self.request_with_retry("generate", Some(&body)).await?;
        let duration_ms = start.elapsed().as_millis() as u64;
        let model = response.model.clone();

        let usage = TokenUsage {
            prompt_tokens: response.prompt_eval_count.unwrap_or(0),
            completion_tokens: response.eval_count.unwrap_or(0),
            total_tokens: response.prompt_eval_count.unwrap_or(0) + response.eval_count.unwrap_or(0),
        };

        let finish_reason = self.parse_finish_reason(&response);

        let result = CompletionResult {
            content: response.response.unwrap_or_default(),
            model,
            usage: usage.clone(),
            finish_reason,
            duration_ms,
            from_cache: false,
        };

        self.update_metrics(true, duration_ms, Some(usage)).await;

        Ok(result)
    }

    async fn chat(
        &self,
        request: ChatRequest,
    ) -> Result<ChatResponse, CloudCoderError> {
        let start = Instant::now();

        let ollama_request = OllamaRequest {
            model: request.model.clone(),
            prompt: None,
            messages: Some(self.convert_messages(&request.messages)),
            stream: false,
            options: self.build_options(request.options.as_ref()),
            format: None,
            system: request.system.clone(),
            raw: None,
        };

        let body = serde_json::to_value(&ollama_request)
            .map_err(|e| CloudCoderError::Api(format!("Failed to serialize request: {}", e)))?;

        let response: OllamaResponse = self.request_with_retry("chat", Some(&body)).await?;
        let duration_ms = start.elapsed().as_millis() as u64;
        let model = response.model.clone();

        let usage = TokenUsage {
            prompt_tokens: response.prompt_eval_count.unwrap_or(0),
            completion_tokens: response.eval_count.unwrap_or(0),
            total_tokens: response.prompt_eval_count.unwrap_or(0) + response.eval_count.unwrap_or(0),
        };

        let finish_reason = self.parse_finish_reason(&response);

        let message = if let Some(msg) = response.message {
            // If there are tool calls, build structured content with tool use blocks
            if !msg.tool_calls.is_empty() {
                let mut blocks: Vec<ContentBlock> = vec![ContentBlock::Text { text: msg.content.clone() }];
                for tc in &msg.tool_calls {
                    blocks.push(ContentBlock::ToolUse {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        input: tc.function.input.clone(),
                    });
                }
                ChatMessage {
                    role: MessageRole::Assistant,
                    content: MessageContent::Blocks(blocks),
                    id: None,
                    name: None,
                    metadata: None,
                }
            } else {
                ChatMessage::assistant(msg.content)
            }
        } else {
            ChatMessage::assistant(response.response.unwrap_or_default())
        };

        let result = ChatResponse {
            id: uuid::Uuid::new_v4().to_string(),
            model,
            message,
            usage: usage.clone(),
            finish_reason,
            duration_ms,
        };

        self.update_metrics(true, duration_ms, Some(usage)).await;

        Ok(result)
    }

    fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<BoxStream<'static, Result<StreamChunk, CloudCoderError>>, CloudCoderError>> + Send + '_>> {
        use futures_util::TryStreamExt;

        Box::pin(async move {
            let url = format!("{}/api/chat", self.base_url);

            let ollama_request = OllamaRequest {
                model: request.model.clone(),
                prompt: None,
                messages: Some(self.convert_messages(&request.messages)),
                stream: true,
                options: self.build_options(request.options.as_ref()),
                format: None,
                system: request.system.clone(),
                raw: None,
            };

            let response = self.client.post(&url)
                .json(&ollama_request)
                .send()
                .await
                .map_err(|e| CloudCoderError::Api(format!("Request failed: {}", e)))?;

            if !response.status().is_success() {
                return Err(CloudCoderError::Api(format!("API error: {}", response.status())));
            }

            let stream = response
                .bytes_stream()
                .map_err(|e| CloudCoderError::Api(format!("Stream error: {}", e)))
                .and_then(|bytes| async move {
                    let text = String::from_utf8_lossy(&bytes);
                    let mut chunks = Vec::new();

                    for line in text.lines() {
                        if line.trim().is_empty() {
                            continue;
                        }
                        if let Ok(chunk) = serde_json::from_str::<OllamaStreamChunk>(line) {
                            let content = chunk.message.as_ref()
                                .map(|m| m.content.clone())
                                .or(chunk.response.clone())
                                .unwrap_or_default();

                            let usage = if chunk.done {
                                Some(TokenUsage {
                                    prompt_tokens: chunk.prompt_eval_count.unwrap_or(0),
                                    completion_tokens: chunk.eval_count.unwrap_or(0),
                                    total_tokens: chunk.prompt_eval_count.unwrap_or(0) + chunk.eval_count.unwrap_or(0),
                                })
                            } else {
                                None
                            };

                            let finish_reason = if chunk.done {
                                Some(FinishReason::Stop)
                            } else {
                                None
                            };

                            chunks.push(StreamChunk {
                                content,
                                thinking: chunk.thinking.clone(),
                                is_final: chunk.done,
                                usage,
                                finish_reason,
                            });
                        }
                    }
                    Ok(futures_util::stream::iter(chunks.into_iter().map(Ok::<_, CloudCoderError>)))
                })
                .try_flatten();

            Ok(stream.boxed())
        })
    }

    async fn count_tokens(&self, messages: &[ChatMessage]) -> Result<u64, CloudCoderError> {
        // Rough approximation: ~4 characters per token
        let total_chars: usize = messages.iter()
            .map(|m| m.content.to_text().len())
            .sum();
        Ok((total_chars / 4) as u64)
    }

    async fn count_text_tokens(&self, text: &str) -> Result<u64, CloudCoderError> {
        Ok((text.len() / 4) as u64)
    }
}

impl OllamaProvider {
    /// Get current metrics
    pub async fn get_metrics(&self) -> ProviderMetrics {
        self.metrics.read().await.clone()
    }

    /// Reset metrics
    pub async fn reset_metrics(&self) {
        let mut metrics = self.metrics.write().await;
        *metrics = ProviderMetrics::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = OllamaProvider::new();
        assert_eq!(provider.name(), "ollama");
        assert!(provider.base_url().contains("localhost:11434"));
    }

    #[test]
    fn test_options_conversion() {
        let opts = GenerationOptions {
            temperature: Some(0.5),
            top_p: Some(0.9),
            top_k: Some(40),
            max_tokens: Some(100),
            stop: Some(vec!["end".to_string()]),
            seed: Some(42),
            frequency_penalty: None,
            presence_penalty: None,
        };

        let ollama_opts: OllamaOptions = opts.into();
        assert_eq!(ollama_opts.temperature, Some(0.5));
        assert_eq!(ollama_opts.num_predict, Some(100));
    }

    #[tokio::test]
    async fn test_message_conversion() {
        let provider = OllamaProvider::new();
        let messages = vec![
            ChatMessage::system("You are helpful"),
            ChatMessage::user("Hello"),
        ];

        let ollama_msgs = provider.convert_messages(&messages);
        assert_eq!(ollama_msgs.len(), 2);
        assert_eq!(ollama_msgs[0].role, "system");
        assert_eq!(ollama_msgs[1].role, "user");
    }
}