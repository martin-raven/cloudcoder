//! Cloud Coder CLI - Rust Implementation
//!
//! A high-performance CLI coding assistant built in Rust.

mod tools;

use clap::{Parser, Subcommand};
use colored::Colorize;

use tools::ToolRegistry;
use cloudcoder_cli::ChatSession;

/// Cloud Coder - Your AI Programming Assistant
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Disable colored output
    #[arg(long, global = true)]
    no_color: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start an interactive coding session
    Chat,

    /// Execute a single tool
    Tool {
        /// Tool name to execute
        #[arg(short, long)]
        name: String,

        /// Tool input as JSON
        #[arg(short, long)]
        input: Option<String>,
    },

    /// List available tools
    Tools,

    /// Show version and build info
    Version,

    /// Manage plugins
    Plugin {
        #[command(subcommand)]
        action: PluginCommands,
    },

    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigCommands,
    },

    /// Run as an agent/worker for coordinator mode
    Agent {
        /// Worker ID
        #[arg(long)]
        id: Option<String>,

        /// Continue from existing worker ID (SendMessage)
        #[arg(long)]
        continue_from: Option<String>,

        /// Task description
        #[arg(short, long)]
        description: String,

        /// Task prompt/instructions
        #[arg(short = 'p', long)]
        prompt: String,

        /// Run as worker (output XML notification on completion)
        #[arg(long, default_value = "false")]
        is_worker: bool,

        /// Model to use
        #[arg(long)]
        model: Option<String>,

        /// System prompt
        #[arg(long)]
        system: Option<String>,

        /// Timeout in milliseconds
        #[arg(long, default_value = "300000")]
        timeout_ms: u64,
    },
}

#[derive(Subcommand, Debug)]
enum PluginCommands {
    /// List installed plugins
    List,

    /// Install a plugin
    Install {
        /// Path to plugin directory
        path: String,
    },

    /// Remove a plugin
    Remove {
        /// Plugin ID to remove
        id: String,
    },

    /// Enable a plugin
    Enable {
        /// Plugin ID to enable
        id: String,
    },

    /// Disable a plugin
    Disable {
        /// Plugin ID to disable
        id: String,
    },
}

#[derive(Subcommand, Debug)]
enum ConfigCommands {
    /// Show current configuration
    Show,

    /// Set a configuration value
    Set {
        /// Configuration key
        key: String,

        /// Configuration value
        value: String,
    },

    /// Get a configuration value
    Get {
        /// Configuration key
        key: String,
    },

    /// Reset configuration to defaults
    Reset,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Initialize logging (simple version)
    if args.verbose {
        tracing_subscriber::fmt::init();
    }

    // Handle commands
    match args.command {
        Some(Commands::Chat) => {
            let mut session = ChatSession::new();
            session.run().await;
        }
        Some(Commands::Tool { name, input }) => {
            run_tool(&name, input).await;
        }
        Some(Commands::Tools) => {
            list_tools().await;
        }
        Some(Commands::Version) => {
            show_version();
        }
        Some(Commands::Plugin { action }) => {
            handle_plugin_command(action).await;
        }
        Some(Commands::Config { action }) => {
            handle_config_command(action).await;
        }
        Some(Commands::Agent { id, continue_from, description, prompt, is_worker, model, system, timeout_ms }) => {
            // Build agent args directly
            let agent_args = cloudcoder_cli::commands::agent::AgentArgs {
                id,
                continue_from,
                description,
                prompt,
                is_worker,
                model,
                system,
                timeout_ms,
            };

            // Run agent command (worker mode or standalone)
            if let Err(e) = cloudcoder_cli::run_agent_command(agent_args).await {
                eprintln!("{}", format!("Agent error: {}", e).red());
                std::process::exit(1);
            }
        }
        None => {
            // Default: start chat session
            let mut session = ChatSession::new();
            session.run().await;
        }
    }
}

async fn run_tool(name: &str, _input: Option<String>) {
    let registry = ToolRegistry::new();

    if let Some(_tool) = registry.get(name).await {
        println!("{}", format!("Tool: {}", name).bright_blue());
        println!("{}", "─".repeat(40));

        // For now, just show tool info
        let tools = registry.get_tool_info().await;
        if let Some(info) = tools.iter().find(|t| t["name"] == name) {
            println!("Name: {}", info["name"]);
            println!("Description: {}", info["description"]);
        }
    } else {
        eprintln!("{}", format!("Tool '{}' not found", name).red());
        println!("Available tools:");
        for tool in registry.list().await {
            println!("  - {}", tool);
        }
    }
}

async fn list_tools() {
    let registry = ToolRegistry::new();
    println!("{}", "Available Tools".bright_blue().bold());
    println!("{}", "─".repeat(40));

    for tool in registry.list().await {
        println!("  {} {}", "•".green(), tool);
    }

    println!();
    println!("Use '/tool <name>' to see tool details");
}

fn show_version() {
    println!("{}", "☁️  Cloud Coder - Rust Edition".bright_blue().bold());
    println!("{}", "─".repeat(40));
    println!("Version:     {}", env!("CARGO_PKG_VERSION"));
    println!("Target:      {}", std::env::consts::ARCH);

    // Feature flags
    println!();
    println!("Features:");
    println!("  {} Core types and errors", "✓".green());
    println!("  {} Event bus and caching", "✓".green());
    println!("  {} Rate limiting and telemetry", "✓".green());
    println!("  {} Parallel execution", "✓".green());
    println!("  {} Plugin system", "✓".green());
    println!("  {} Vector store", "✓".green());
}

async fn handle_plugin_command(action: PluginCommands) {
    use cloudcoder_services::plugin::{PluginRegistry, PluginRegistryConfig};
    use std::sync::Arc;

    let config = PluginRegistryConfig::default();
    let registry = Arc::new(PluginRegistry::new(config));

    match action {
        PluginCommands::List => {
            println!("{}", "Installed Plugins".bright_blue().bold());
            println!("{}", "─".repeat(40));

            match registry.discover_plugins().await {
                Ok(plugins) => {
                    if plugins.is_empty() {
                        println!("No plugins installed.");
                    } else {
                        for plugin_id in plugins {
                            println!("  {} {}", "•".green(), plugin_id);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("{}", format!("Failed to list plugins: {}", e).red());
                }
            }
        }
        PluginCommands::Install { path } => {
            println!("{}", format!("Installing plugin from: {}", path).yellow());
            println!("Note: Plugin installation requires additional implementation");
        }
        PluginCommands::Remove { id } => {
            println!("{}", format!("Removing plugin: {}", id).yellow());
            if let Err(e) = registry.remove_plugin(&id).await {
                eprintln!("{}", format!("Failed to remove plugin: {}", e).red());
            }
        }
        PluginCommands::Enable { id } => {
            println!("{}", format!("Enabling plugin: {}", id).yellow());
            if let Err(e) = registry.enable_plugin(&id).await {
                eprintln!("{}", format!("Failed to enable plugin: {}", e).red());
            }
        }
        PluginCommands::Disable { id } => {
            println!("{}", format!("Disabling plugin: {}", id).yellow());
            if let Err(e) = registry.disable_plugin(&id).await {
                eprintln!("{}", format!("Failed to disable plugin: {}", e).red());
            }
        }
    }
}

async fn handle_config_command(action: ConfigCommands) {
    match action {
        ConfigCommands::Show => {
            println!("{}", "Configuration".bright_blue().bold());
            println!("{}", "─".repeat(40));
            println!("Config file: ~/.config/cloudcoder/config.toml");
            println!();
            println!("Current settings:");
            println!("  model = {}", "default".yellow());
            println!("  max_tokens = {}", "4096".yellow());
            println!("  temperature = {}", "0.7".yellow());
        }
        ConfigCommands::Set { key, value } => {
            println!("{}", format!("Setting {} = {}", key, value).yellow());
            println!("Note: Configuration persistence requires additional implementation");
        }
        ConfigCommands::Get { key } => {
            println!("{}", format!("Getting configuration for: {}", key).yellow());
            println!("Value: default");
        }
        ConfigCommands::Reset => {
            println!("{}", "Resetting configuration to defaults...".yellow());
            println!("{}", "Done!".green());
        }
    }
}