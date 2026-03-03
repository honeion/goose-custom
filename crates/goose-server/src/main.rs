// Import modules for different functionalities
mod commands;
mod configuration;
mod error;
mod logging;
mod openapi;
mod routes;
mod state;
mod tunnel;

// Import necessary dependencies
use std::path::PathBuf;
use clap::{Parser, Subcommand};
use goose::agents::validate_extensions;
use goose::config::paths::Paths;
use goose_mcp::{
    mcp_server_runner::{serve, McpCommand},
    AutoVisualiserRouter, ComputerControllerServer, DeveloperServer, MemoryServer, TutorialServer,
};

// Define CLI options and subcommands
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

// Define available subcommands
#[derive(Subcommand)]
enum Commands {
    /// Run the agent server
    Agent,
    /// Run the MCP server
    Mcp {
        #[arg(value_parser = clap::value_parser!(McpCommand))]
        server: McpCommand,
    },
    /// Validate a bundled-extensions JSON file
    #[command(name = "validate-extensions")]
    ValidateExtensions {
        /// Path to the bundled-extensions JSON file
        path: PathBuf,
    },
}

// Main async function to parse CLI input and execute respective commands
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Agent => {
            // Run the agent server
            commands::agent::run().await?;
        }
        Commands::Mcp { server } => {
            // Setup logging for MCP server
            logging::setup_logging(Some(&format!("mcp-{}", server.name())))?;
            match server {
                McpCommand::AutoVisualiser => serve(AutoVisualiserRouter::new()).await?,
                McpCommand::ComputerController => serve(ComputerControllerServer::new()).await?,
                McpCommand::Memory => serve(MemoryServer::new()).await?,
                McpCommand::Tutorial => serve(TutorialServer::new()).await?,
                McpCommand::Developer => {
                    // Load .bash_env configuration for Developer server
                    let bash_env = Paths::config_dir().join(".bash_env");
                    serve(
                        DeveloperServer::new()
                            .extend_path_with_shell(true)
                            .bash_env_file(Some(bash_env)),
                    )
                    .await?
                }
            }
        }
        Commands::ValidateExtensions { path } => {
            // Validate the bundled-extensions JSON file
            match validate_extensions::validate_bundled_extensions(&path) {
                Ok(msg) => println!("{msg}"),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}
