//! WeChat Public Account Markdown Uploader
//!
//! A command-line tool for uploading markdown files to WeChat public accounts.

use anyhow::{Context, Result};
use clap::Parser;
use wx_uploader::{Config, WxUploader, cli};

#[tokio::main]
async fn main() -> Result<()> {
    // Check if help is requested before clap processes args
    if std::env::args().any(|arg| arg == "--help" || arg == "-h") {
        cli::print_colored_help();
        std::process::exit(0);
    }

    let args = cli::Args::parse();

    // Validate arguments
    if let Err(error_msg) = cli::validate_args(&args) {
        eprintln!("Error: {}", error_msg);
        std::process::exit(1);
    }

    // Initialize logging
    cli::init_logging(args.verbose);

    // Display banner if verbose
    cli::display_banner(&args);

    // Create configuration from environment variables
    let config = Config::from_env()
        .context("Failed to load configuration from environment variables")?
        .with_verbose(args.verbose);

    // Create the uploader
    let uploader = WxUploader::new(config)
        .await
        .context("Failed to initialize WeChat uploader")?;

    // Process the input path
    if args.path.is_file() {
        // Force upload single file
        uploader
            .upload_file(&args.path, true)
            .await
            .context("Failed to upload file")?;
    } else if args.path.is_dir() {
        // Process directory
        uploader
            .process_directory(&args.path)
            .await
            .context("Failed to process directory")?;
    } else {
        anyhow::bail!("Path must be a file or directory");
    }

    Ok(())
}
