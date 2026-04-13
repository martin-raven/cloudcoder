//! Chat session implementation
//!
//! Provides interactive chat session with Ollama models.

use std::io::{self, Write};

use colored::Colorize;
use futures_util::StreamExt;

use cloudcoder_provider::{OllamaProvider, Provider, ChatRequest, ChatMessage};

use crate::tools::ToolRegistry;

/// Interactive chat session with Ollama models
pub struct ChatSession {
    /// The Ollama provider
    provider: OllamaProvider,
    /// Conversation history
    messages: Vec<ChatMessage>,
    /// Current model
    model: String,
    /// Tool registry for tool execution
    tool_registry: ToolRegistry,
    /// Optional system prompt
    system_prompt: Option<String>,
}

impl ChatSession {
    /// Create a new chat session with default settings
    pub fn new() -> Self {
        let provider = OllamaProvider::cloud();
        let model = provider.default_model().to_string();
        Self {
            provider,
            messages: Vec::new(),
            model,
            tool_registry: ToolRegistry::new(),
            system_prompt: None,
        }
    }

    /// Create a chat session with a specific model
    pub fn with_model(model: String) -> Self {
        let mut session = Self::new();
        session.model = model;
        session
    }

    /// Run the interactive chat session
    pub async fn run(&mut self) {
        println!("{}", "  Cloud Coder - Rust Edition".bright_blue().bold());
        println!("{}", "-".repeat(40));
        println!("Model: {}", self.model.bright_blue());
        println!();
        println!("Commands: /help");
        println!();

        loop {
            print!("{}", "> ".green().bold());
            io::stdout().flush().unwrap();

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap();
            let input = input.trim();

            if input.is_empty() {
                continue;
            }

            if input.starts_with('/') {
                match input {
                    "/exit" | "/quit" | "/q" => {
                        println!("{}", "Goodbye!".bright_blue());
                        break;
                    }
                    "/help" => {
                        self.print_help();
                        continue;
                    }
                    _ => {
                        println!("Unknown command. Type /help for available commands.");
                        continue;
                    }
                }
            }

            self.messages.push(ChatMessage::user(input));
            match self.stream_response().await {
                Ok(()) => {}
                Err(e) => eprintln!("{}", format!("Error: {}", e).red()),
            }
        }
    }

    /// Stream response from the model
    async fn stream_response(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let request = self.build_request();
        let stream_future = self.provider.chat_stream(request);
        let mut stream = stream_future.await?;

        let mut full_content = String::new();

        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    // Print thinking dimly
                    if !chunk.thinking.is_empty() {
                        print!("{}", chunk.thinking.bright_black());
                        io::stdout().flush().unwrap();
                    }

                    // Print content normally
                    if !chunk.content.is_empty() {
                        print!("{}", chunk.content);
                        io::stdout().flush().unwrap();
                        full_content.push_str(&chunk.content);
                    }

                    if chunk.is_final {
                        println!();
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("{}", format!("\nStream error: {}", e).red());
                    break;
                }
            }
        }

        self.messages.push(ChatMessage::assistant(full_content));
        Ok(())
    }

    /// Build a chat request
    fn build_request(&self) -> ChatRequest {
        ChatRequest {
            model: self.model.clone(),
            messages: self.messages.clone(),
            options: None,
            system: self.system_prompt.clone(),
            stream: false,
            tools: None,
        }
    }

    /// Print available commands
    fn print_help(&self) {
        println!();
        println!("Available commands:");
        println!("  {}  - Exit the session", "/exit".yellow());
        println!("  {} - Clear the screen", "/clear".yellow());
        println!("  {}  - List available models", "/models".yellow());
        println!("  {} <model> - Switch to a model", "/model".yellow());
        println!("  {}  - Show this help", "/help".yellow());
        println!();
    }
}

impl Default for ChatSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation() {
        let session = ChatSession::new();
        assert!(!session.model.is_empty());
    }

    #[test]
    fn test_session_with_model() {
        let session = ChatSession::with_model("test-model".to_string());
        assert_eq!(session.model, "test-model");
    }

    #[test]
    fn test_build_request() {
        let session = ChatSession::new();
        let request = session.build_request();
        assert!(!request.model.is_empty());
        assert!(request.messages.is_empty());
    }
}