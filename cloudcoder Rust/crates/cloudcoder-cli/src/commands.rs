//! Slash command handlers for ChatSession

use colored::Colorize;
use cloudcoder_provider::{OllamaProvider, Provider};

/// Handles slash commands for the chat session
pub struct CommandHandler {
    provider: OllamaProvider,
}

impl CommandHandler {
    /// Create a new command handler
    pub fn new(provider: OllamaProvider) -> Self {
        Self { provider }
    }

    /// List available models from /api/tags
    pub async fn list_models(&self, current_model: &str) {
        println!("{}", "Available Models".bright_blue().bold());
        println!("{}", "─".repeat(40));

        match self.provider.list_models().await {
            Ok(models) => {
                if models.is_empty() {
                    println!("  No models available.");
                    println!("  Make sure Ollama is running: ollama serve");
                } else {
                    for model in models {
                        let marker = if model.id == current_model {
                            "*"
                        } else {
                            " "
                        };
                        let cloud_marker = if model.id.contains("cloud") {
                            " (cloud)"
                        } else {
                            ""
                        };
                        println!(
                            "  {} {}{}",
                            marker.yellow(),
                            model.id.bright_white(),
                            cloud_marker.bright_black()
                        );
                    }
                }
            }
            Err(e) => {
                eprintln!(
                    "{}",
                    format!("Failed to list models: {}", e).red()
                );
                println!();
                println!("  Make sure Ollama is running: ollama serve");
            }
        }
        println!();
    }

    /// Switch to a different model
    pub async fn switch_model(
        &self,
        _current: &str,
        new_model: &str,
    ) -> Result<String, String> {
        let models = self
            .provider
            .list_models()
            .await
            .map_err(|e| format!("Failed to list models: {}", e))?;

        // Check for exact match first
        let model_exists = models.iter().any(|m| m.id == new_model);

        // Also check for partial match (e.g., "llama3" matches "llama3:latest")
        let matched_model = if model_exists {
            Some(new_model.to_string())
        } else {
            models
                .iter()
                .find(|m| m.id.starts_with(new_model) || m.id == format!("{}:latest", new_model))
                .map(|m| m.id.clone())
        };

        match matched_model {
            Some(model) => {
                println!("Switched to model: {}", model.bright_blue());
                Ok(model)
            }
            None => {
                Err(format!(
                    "Model '{}' not found. Use /models to list available models.",
                    new_model
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_handler_creation() {
        let provider = OllamaProvider::new();
        let handler = CommandHandler::new(provider);
        assert_eq!(handler.provider.name(), "ollama");
    }
}