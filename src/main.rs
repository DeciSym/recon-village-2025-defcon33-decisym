use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use decisym_defcon33::{EnrichConfig, OpenAIClient, TorDownloader};
use std::path::PathBuf;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// A privacy-focused tool for collecting content through Tor and enriching it with local LLMs
#[derive(Parser, Debug)]
#[command(name = "decisym_defcon33")]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Subcommand to execute
    #[command(subcommand)]
    command: Commands,

    /// Enable quiet mode (suppress non-error messages)
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Enable verbose output
    #[arg(short, long, global = true, conflicts_with = "quiet")]
    verbose: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Download content from URLs through Tor for privacy
    Collect {
        /// URL to download
        url: String,

        /// Write output to FILE instead of using the server-provided name
        #[arg(short = 'o', long = "output", value_name = "FILE")]
        output: Option<PathBuf>,

        /// Output file name (alternative to --output)
        #[arg(short = 'O', value_name = "FILE", conflicts_with = "output")]
        output_alt: Option<PathBuf>,

        /// Set User-Agent header (default: Chrome)
        #[arg(short = 'A', long = "user-agent", value_name = "STRING")]
        user_agent: Option<String>,

        /// Wait SECONDS between requests (rate limiting)
        #[arg(
            short = 'w',
            long = "wait",
            value_name = "SECONDS",
            default_value = "1"
        )]
        wait: u64,

        /// Maximum number of redirects to follow
        #[arg(long = "max-redirect", value_name = "NUM", default_value = "5")]
        max_redirects: u32,

        /// Accept invalid TLS certificates (insecure)
        #[arg(short = 'k', long = "insecure")]
        insecure: bool,

        /// Download buffer size in bytes
        #[arg(long = "buffer-size", value_name = "BYTES", default_value = "8192")]
        buffer_size: usize,

        /// Default filename for URLs without a filename
        #[arg(
            long = "default-filename",
            value_name = "NAME",
            default_value = "index.html"
        )]
        default_filename: String,

        /// HTTP method to use (GET or POST)
        #[arg(
            short = 'X',
            long = "method",
            value_name = "METHOD",
            default_value = "GET"
        )]
        method: Option<String>,

        /// Add custom HTTP header (can be used multiple times)
        #[arg(short = 'H', long = "header", value_name = "HEADER")]
        headers: Vec<String>,

        /// HTTP request body data (for POST requests)
        #[arg(short = 'd', long = "data", value_name = "DATA")]
        data: Option<String>,

        /// HTTP request body data from file (for POST requests)
        #[arg(long = "data-file", value_name = "FILE", conflicts_with = "data")]
        data_file: Option<PathBuf>,
    },

    /// Enrich content using an OpenAI-compatible API
    Enrich {
        /// Path to the configuration file (YAML or JSON)
        #[arg(short = 'c', long = "config", value_name = "PATH")]
        config_file: PathBuf,

        /// Optional input file to process (overrides any file path in config)
        #[arg(short = 'i', long = "input")]
        input_file: Option<PathBuf>,

        /// Output file (if not specified, prints to stdout)
        #[arg(short = 'o', long = "output")]
        output: Option<PathBuf>,
    },
}

async fn handle_collect_command(cli: &Cli, cmd: &Commands) -> Result<()> {
    let Commands::Collect {
        url,
        output,
        output_alt,
        user_agent,
        wait,
        max_redirects,
        insecure,
        buffer_size,
        default_filename,
        method,
        headers,
        data,
        data_file,
    } = cmd
    else {
        unreachable!("handle_collect_command called with non-Collect command");
    };
    if !cli.quiet {
        println!("Tor File Downloader");
        println!("==================");
        println!();
    }

    // Create downloader
    let mut downloader = TorDownloader::new().await?;
    downloader.set_rate_limit_delay(*wait);
    downloader.set_max_redirects(*max_redirects);
    downloader.set_insecure(*insecure);
    downloader.set_buffer_size(*buffer_size);
    downloader.set_default_filename(default_filename);

    // Set custom user agent if provided
    if let Some(user_agent) = user_agent {
        downloader.set_user_agent(user_agent);
    }

    // Check if this is a web service request (POST or has data)
    let is_web_service = method
        .as_ref()
        .map_or(false, |m| m.to_uppercase() == "POST")
        || data.is_some()
        || data_file.is_some()
        || !headers.is_empty();

    // Download the file
    info!("Downloading: {}", url);

    let filename = if is_web_service {
        // Web service mode - use the new download_web_service method
        info!("Using web service mode");

        // Read data from file if specified
        let body_data = if let Some(file) = data_file {
            Some(std::fs::read_to_string(file).context("Failed to read data file")?)
        } else {
            data.clone()
        };

        let method_str = method.as_ref().map(|s| s.as_str()).unwrap_or("GET");
        let (response_body, suggested_filename) = downloader
            .download_web_service(url, method_str, headers, body_data.as_deref())
            .await?;

        // For web service responses, save directly as the response body
        let output_filename = output
            .as_ref()
            .or(output_alt.as_ref())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or(suggested_filename);

        std::fs::write(&output_filename, &response_body)
            .context("Failed to write response to file")?;

        output_filename
    } else {
        downloader.download_file(url).await?
    };

    // Handle output filename
    let final_path = match output.as_ref().or(output_alt.as_ref()) {
        Some(output) if output != &PathBuf::from(&filename) => {
            // User specified output file that differs from downloaded name
            std::fs::rename(&filename, output)?;
            info!("Saved as: {}", output.display());
            output.clone()
        }
        Some(_) => {
            // Output matches the downloaded filename
            PathBuf::from(&filename)
        }
        None => {
            // Use server-provided name
            info!("Saved as: {}", &filename);
            PathBuf::from(&filename)
        }
    };

    if !cli.quiet {
        println!();
        println!("Download complete: {}", final_path.display());
    }

    Ok(())
}

async fn handle_enrich_command(cli: &Cli, cmd: &Commands) -> Result<()> {
    let Commands::Enrich {
        config_file,
        input_file,
        output,
    } = cmd
    else {
        unreachable!("handle_enrich_command called with non-Enrich command");
    };

    if !cli.quiet {
        println!("OpenAI-Compatible API Client");
        println!("===========================");
        println!();
    }

    // Load configuration
    let mut config = match config_file.extension().and_then(|s| s.to_str()) {
        Some("yaml") | Some("yml") => EnrichConfig::from_yaml_file(config_file)?,
        Some("json") => EnrichConfig::from_json_file(config_file)?,
        _ => anyhow::bail!("Configuration file must have .yaml, .yml, or .json extension"),
    };

    info!("Loaded configuration from: {}", config_file.display());

    // If input file is specified, read it and update the prompt
    if let Some(input_path) = input_file {
        let content = std::fs::read_to_string(input_path).context("Failed to read input file")?;

        // Update the prompt in the config to include the file content
        match &mut config.prompt {
            decisym_defcon33::PromptConfig::Completion { prompt } => {
                *prompt = format!("{}\n\nContent:\n{}", prompt, content);
            }
            decisym_defcon33::PromptConfig::Chat { messages } => {
                // Append content to the last user message
                if let Some(last_msg) = messages.iter_mut().rev().find(|m| m.role == "user") {
                    last_msg.content = format!("{}\n\nContent:\n{}", last_msg.content, content);
                } else {
                    // If no user message, add one
                    messages.push(decisym_defcon33::ChatMessage {
                        role: "user".to_string(),
                        content: format!("Content:\n{}", content),
                    });
                }
            }
        }
    }

    // Create client and send request
    let client = OpenAIClient::new()?;

    info!("Sending request to: {}", config.api_url);
    let response = client.enrich(&config).await?;

    // Output response
    if let Some(output_path) = output {
        std::fs::write(output_path, &response)?;
        if !cli.quiet {
            println!("Response saved to: {}", output_path.display());
        }
    } else {
        println!("{}", response);
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set up logging based on verbosity
    let filter = if cli.quiet {
        "error"
    } else if cli.verbose {
        "debug"
    } else {
        // Default to info level, but reduce tor_dirmgr warnings to error level only
        "info,tor_dirmgr=error"
    };

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_new(filter).unwrap_or_default())
        .init();

    match &cli.command {
        Commands::Collect { .. } => {
            handle_collect_command(&cli, &cli.command).await?;
        }
        Commands::Enrich { .. } => {
            handle_enrich_command(&cli, &cli.command).await?;
        }
    }

    Ok(())
}
