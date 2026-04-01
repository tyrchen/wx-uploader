//! Command-line interface for the wx-uploader application
//!
//! This module handles argument parsing, colored help display, and CLI-specific
//! functionality for the WeChat uploader tool.

use clap::Parser;
use colored::*;
use std::path::PathBuf;

/// Command-line arguments for the wx-uploader application
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "A tool to upload articles to WeChat Official Account",
    override_help = "Run with --help to see colored help",
    color = clap::ColorChoice::Always,
    styles = clap::builder::Styles::styled()
        .header(clap::builder::styling::AnsiColor::Yellow.on_default())
        .usage(clap::builder::styling::AnsiColor::Green.on_default())
        .literal(clap::builder::styling::AnsiColor::Green.on_default())
        .placeholder(clap::builder::styling::AnsiColor::Green.on_default())
)]
pub struct Args {
    /// Path to markdown file or directory to upload
    #[arg(
        help = "Path to markdown file or directory to upload. Files uploaded regardless of status. Directories skip published files. Set theme and code highlighter in frontmatter - see help for complete lists."
    )]
    pub path: PathBuf,

    /// Enable verbose logging with detailed tracing information
    #[arg(
        short,
        long,
        help = "Enable verbose logging with detailed tracing information"
    )]
    pub verbose: bool,

    /// Force refresh WeChat access token before operation
    #[arg(
        short = 'r',
        long = "refresh",
        help = "Force refresh WeChat access token before operation. This gets a new token from WeChat API."
    )]
    pub clear_cache: bool,
}

/// Print colored help message with detailed information about usage and features
pub fn print_colored_help() {
    // Force colors to be enabled
    colored::control::set_override(true);

    println!(
        "{}",
        "A tool to upload articles to WeChat Official Account"
            .bright_white()
            .bold()
    );
    println!();
    println!(
        "{}: {} [OPTIONS] <PATH>",
        "Usage".bright_green().bold(),
        "wx-uploader".bright_cyan()
    );
    println!();
    println!("{}", "Arguments:".bright_yellow().bold());
    println!(
        "  {}  Path to markdown file or directory to upload. Files uploaded regardless of status.",
        "<PATH>".bright_cyan()
    );
    println!(
        "          Directories skip published files. Set theme and code highlighter in frontmatter - see help"
    );
    println!("          for complete lists.");
    println!();
    println!("{}", "Options:".bright_yellow().bold());
    println!(
        "  {}, {}  Enable verbose logging with detailed tracing information",
        "-v".bright_cyan(),
        "--verbose".bright_cyan()
    );
    println!(
        "  {}, {}  Force refresh WeChat access token",
        "-r".bright_cyan(),
        "--refresh".bright_cyan()
    );
    println!(
        "  {}, {}     Print help",
        "-h".bright_cyan(),
        "--help".bright_cyan()
    );
    println!(
        "  {}, {}  Print version",
        "-V".bright_cyan(),
        "--version".bright_cyan()
    );
    println!();
    println!(
        "{}: Set {} and {} environment variables",
        "REQUIREMENTS".bright_red().bold(),
        "WECHAT_APP_ID".bright_cyan(),
        "WECHAT_APP_SECRET".bright_cyan()
    );
    println!(
        "{}: Set {} for default cover generation or {} for {}",
        "OPTIONAL".bright_blue().bold(),
        "GEMINI_API_KEY".bright_cyan(),
        "OPENAI_API_KEY".bright_cyan(),
        "model: gpt".bright_cyan()
    );
    println!();
    println!(
        "{}: Supports {} themes and {} code highlighters via frontmatter:",
        "THEMING".bright_green().bold(),
        "8".bright_white().bold(),
        "10".bright_white().bold()
    );
    println!("  {}", "---".bright_black());
    println!("  {}: \"My Article\"", "title".bright_cyan());
    println!(
        "  {}: \"lapis\"        {} Themes: {}",
        "theme".bright_cyan(),
        "#".bright_black(),
        "default, lapis, maize, orangeheart, phycat, pie, purple, rainbow".white()
    );
    println!(
        "  {}: \"github\"        {} Highlighters: {}",
        "code".bright_cyan(),
        "#".bright_black(),
        "github, github-dark, vscode, atom-one-light, atom-one-dark,".white()
    );
    println!(
        "                        {}",
        "solarized-light, solarized-dark, monokai, dracula, xcode".white()
    );
    println!("  {}: \"draft\"", "published".bright_cyan());
    println!(
        "  {}: \"cover.png\"     {} Auto-generated if missing with selected backend",
        "cover".bright_cyan(),
        "#".bright_black()
    );
    println!(
        "  {}: \"nb2\"           {} Models: nb2 (default, Gemini Flash), nb (Gemini Pro), gpt (OpenAI)",
        "model".bright_cyan(),
        "#".bright_black()
    );
    println!("  {}", "---".bright_black());
    println!();
    println!("{}", "EXAMPLES:".bright_blue().bold());
    println!(
        "  {}      {} Upload single file (force)",
        "wx-uploader article.md".bright_white().bold(),
        "#".bright_black()
    );
    println!(
        "  {}     {} Process directory (skip published)",
        "wx-uploader ./articles/".bright_white().bold(),
        "#".bright_black()
    );
    println!(
        "  {}      {} Verbose logging",
        "wx-uploader -v ./blog/".bright_white().bold(),
        "#".bright_black()
    );
    println!(
        "  {}  {} Force refresh token & upload",
        "wx-uploader -r article.md".bright_white().bold(),
        "#".bright_black()
    );
}

/// Validates command-line arguments
pub fn validate_args(args: &Args) -> Result<(), String> {
    if !args.path.exists() {
        return Err(format!("Path does not exist: {}", args.path.display()));
    }

    if !args.path.is_file() && !args.path.is_dir() {
        return Err(format!(
            "Path must be a file or directory: {}",
            args.path.display()
        ));
    }

    Ok(())
}

/// Initializes logging based on the verbose flag
pub fn init_logging(verbose: bool) {
    if verbose {
        tracing_subscriber::fmt::init();
    }
}

/// Display startup banner with configuration information
pub fn display_banner(args: &Args) {
    if !args.verbose {
        return;
    }

    println!();
    println!("{}", "wx-uploader".bright_cyan().bold());
    println!("{}", "=".repeat(20).bright_black());
    println!("Path: {}", args.path.display().to_string().bright_white());
    println!(
        "Mode: {}",
        if args.path.is_file() {
            "Single file"
        } else {
            "Directory"
        }
        .bright_green()
    );
    println!("Verbose: {}", args.verbose.to_string().bright_blue());
    println!("{}", "=".repeat(20).bright_black());
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_validate_args_file_exists() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.md");
        fs::write(&file_path, "test content").unwrap();

        let args = Args {
            path: file_path,
            verbose: false,
            clear_cache: false,
        };

        assert!(validate_args(&args).is_ok());
    }

    #[test]
    fn test_validate_args_dir_exists() {
        let temp_dir = TempDir::new().unwrap();
        let args = Args {
            path: temp_dir.path().to_path_buf(),
            verbose: false,
            clear_cache: false,
        };

        assert!(validate_args(&args).is_ok());
    }

    #[test]
    fn test_validate_args_path_not_exists() {
        let args = Args {
            path: PathBuf::from("nonexistent/path"),
            verbose: false,
            clear_cache: false,
        };

        assert!(validate_args(&args).is_err());
        assert!(validate_args(&args).unwrap_err().contains("does not exist"));
    }

    #[test]
    fn test_init_logging_verbose() {
        // This test mainly ensures the function doesn't panic
        // In practice, logging setup would be tested differently
        init_logging(true);
        init_logging(false);
    }

    #[test]
    fn test_display_banner() {
        let temp_dir = TempDir::new().unwrap();
        let args = Args {
            path: temp_dir.path().to_path_buf(),
            verbose: true,
            clear_cache: false,
        };

        // This test mainly ensures the function doesn't panic
        display_banner(&args);

        let args = Args {
            path: temp_dir.path().to_path_buf(),
            verbose: false,
            clear_cache: false,
        };

        display_banner(&args);
    }

    #[test]
    fn test_args_parsing() {
        // This test verifies the Args structure can be created
        let args = Args {
            path: PathBuf::from("test.md"),
            verbose: true,
            clear_cache: false,
        };

        assert_eq!(args.path, PathBuf::from("test.md"));
        assert!(args.verbose);
    }
}
