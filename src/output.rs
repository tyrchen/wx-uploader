//! Output formatting utilities
//!
//! This module provides centralized formatting for console output to ensure
//! consistent styling and reduce code duplication across the application.

use colored::*;
use std::path::Path;

/// Trait for formatting console output with consistent styling
pub trait OutputFormatter {
    /// Formats a success message with green checkmark
    fn success(&self, message: &str) -> String;

    /// Formats an error message with red X
    fn error(&self, message: &str) -> String;

    /// Formats a warning message with yellow warning symbol
    fn warning(&self, message: &str) -> String;

    /// Formats an info message with blue info symbol
    fn info(&self, message: &str) -> String;

    /// Formats a progress message with blue arrow
    fn progress(&self, message: &str) -> String;

    /// Formats a skip message with yellow skip symbol
    fn skip(&self, message: &str) -> String;

    /// Formats a generation message with cyan art symbol
    fn generation(&self, message: &str) -> String;

    /// Prints a success message
    fn print_success(&self, message: &str) {
        println!("{}", self.success(message));
    }

    /// Prints an error message
    fn print_error(&self, message: &str) {
        eprintln!("{}", self.error(message));
    }

    /// Prints a warning message
    fn print_warning(&self, message: &str) {
        println!("{}", self.warning(message));
    }

    /// Prints an info message
    fn print_info(&self, message: &str) {
        println!("{}", self.info(message));
    }

    /// Prints a progress message
    fn print_progress(&self, message: &str) {
        println!("{}", self.progress(message));
    }

    /// Prints a skip message
    fn print_skip(&self, message: &str) {
        println!("{}", self.skip(message));
    }

    /// Prints a generation message
    fn print_generation(&self, message: &str) {
        println!("{}", self.generation(message));
    }
}

/// Standard console output formatter with colored output
#[derive(Debug, Clone, Copy)]
pub struct ConsoleFormatter;

impl OutputFormatter for ConsoleFormatter {
    fn success(&self, message: &str) -> String {
        format!("{} {}", "✓".bright_green(), message.green())
    }

    fn error(&self, message: &str) -> String {
        format!("{} {}", "✗".bright_red(), message.red())
    }

    fn warning(&self, message: &str) -> String {
        format!("{} {}", "⚠".bright_yellow(), message.yellow())
    }

    fn info(&self, message: &str) -> String {
        format!("{} {}", "ℹ".bright_blue(), message.dimmed())
    }

    fn progress(&self, message: &str) -> String {
        format!("{} {}", "🔄".bright_blue(), message.bright_white())
    }

    fn skip(&self, message: &str) -> String {
        format!("{} {}", "⏭".bright_yellow(), message.dimmed())
    }

    fn generation(&self, message: &str) -> String {
        format!("{} {}", "🎨".bright_cyan(), message.bright_white())
    }
}

/// Extensions for file path formatting
pub trait FilePathFormatter {
    /// Formats a file operation message with consistent path display
    fn format_file_operation(&self, operation: &str, path: &Path) -> String;

    /// Formats an upload success message
    fn format_upload_success(&self, path: &Path) -> String;

    /// Formats an upload failure message
    fn format_upload_failure(&self, path: &Path) -> String;

    /// Formats a skip message for already published files
    fn format_skip_published(&self, path: &Path) -> String;

    /// Formats a cover generation message
    fn format_cover_generation(&self, path: &Path) -> String;

    /// Formats a cover generation success message
    fn format_cover_success(&self, filename: &str) -> String;

    /// Formats a cover generation failure message
    fn format_cover_failure(&self) -> String;

    /// Formats an image prompt message
    fn format_image_prompt(&self, prompt: &str) -> String;

    /// Formats a target path message
    fn format_target_path(&self, path: &Path) -> String;

    /// Formats an image save success message
    fn format_image_saved(&self, path: &Path) -> String;
}

impl<T: OutputFormatter> FilePathFormatter for T {
    fn format_file_operation(&self, operation: &str, path: &Path) -> String {
        format!("{}: {}", operation, path.display())
    }

    fn format_upload_success(&self, path: &Path) -> String {
        self.success(&format!("uploaded: {}", path.display()))
    }

    fn format_upload_failure(&self, path: &Path) -> String {
        self.error(&format!("failed: {}", path.display()))
    }

    fn format_skip_published(&self, path: &Path) -> String {
        self.skip(&format!("skipped: {}", path.display()))
    }

    fn format_cover_generation(&self, path: &Path) -> String {
        self.generation(&format!("generating cover: {}", path.display()))
    }

    fn format_cover_success(&self, filename: &str) -> String {
        format!(
            "{} {} {}",
            "✨".bright_green(),
            "cover generated:".green(),
            filename
        )
    }

    fn format_cover_failure(&self) -> String {
        self.warning("cover generation failed, continuing...")
    }

    fn format_image_prompt(&self, prompt: &str) -> String {
        format!(
            "  {} {}",
            "→".bright_blue(),
            format!("Image prompt: {}", prompt).bright_white()
        )
    }

    fn format_target_path(&self, path: &Path) -> String {
        format!("  {} Target path: {}", "📍".bright_cyan(), path.display())
    }

    fn format_image_saved(&self, path: &Path) -> String {
        format!(
            "  {} Image saved to: {}",
            "💾".bright_green(),
            path.display()
        )
    }
}

/// API error formatter for consistent error reporting
pub trait ApiErrorFormatter {
    /// Formats an OpenAI API error message
    fn format_openai_error(&self, status: u16, response: &str, endpoint: &str) -> String;

    /// Formats a general API error
    fn format_api_error(&self, service: &str, error: &str) -> String;

    /// Formats image generation failure
    fn format_image_generation_failure(&self, error: &str) -> String;

    /// Formats image download failure
    fn format_image_download_failure(&self, error: &str) -> String;
}

impl<T: OutputFormatter> ApiErrorFormatter for T {
    fn format_openai_error(&self, status: u16, response: &str, endpoint: &str) -> String {
        format!(
            "  {} OpenAI API Error:\n    Status: {}\n    Response: {}\n    Endpoint: {}",
            "⚠".bright_yellow(),
            status,
            response,
            endpoint
        )
    }

    fn format_api_error(&self, service: &str, error: &str) -> String {
        format!("  {} {} API Error: {}", "❌".bright_red(), service, error)
    }

    fn format_image_generation_failure(&self, error: &str) -> String {
        format!(
            "  {} Failed to generate image: {}",
            "❌".bright_red(),
            error
        )
    }

    fn format_image_download_failure(&self, error: &str) -> String {
        format!(
            "  {} Failed to download/save image: {}",
            "❌".bright_red(),
            error
        )
    }
}

/// Global formatter instance for consistent usage across the application
pub const FORMATTER: ConsoleFormatter = ConsoleFormatter;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_console_formatter_success() {
        let formatter = ConsoleFormatter;
        let message = formatter.success("Operation completed");
        assert!(message.contains("✓"));
        assert!(message.contains("Operation completed"));
    }

    #[test]
    fn test_console_formatter_error() {
        let formatter = ConsoleFormatter;
        let message = formatter.error("Operation failed");
        assert!(message.contains("✗"));
        assert!(message.contains("Operation failed"));
    }

    #[test]
    fn test_console_formatter_warning() {
        let formatter = ConsoleFormatter;
        let message = formatter.warning("Warning message");
        assert!(message.contains("⚠"));
        assert!(message.contains("Warning message"));
    }

    #[test]
    fn test_file_path_formatter() {
        let formatter = ConsoleFormatter;
        let path = PathBuf::from("/path/to/file.md");

        let upload_success = formatter.format_upload_success(&path);
        assert!(upload_success.contains("uploaded"));
        assert!(upload_success.contains("file.md"));

        let upload_failure = formatter.format_upload_failure(&path);
        assert!(upload_failure.contains("failed"));
        assert!(upload_failure.contains("file.md"));
    }

    #[test]
    fn test_api_error_formatter() {
        let formatter = ConsoleFormatter;

        let openai_error = formatter.format_openai_error(429, "Rate limit", "/endpoint");
        assert!(openai_error.contains("OpenAI API Error"));
        assert!(openai_error.contains("429"));
        assert!(openai_error.contains("Rate limit"));
        assert!(openai_error.contains("/endpoint"));

        let api_error = formatter.format_api_error("WeChat", "Authentication failed");
        assert!(api_error.contains("WeChat API Error"));
        assert!(api_error.contains("Authentication failed"));
    }

    #[test]
    fn test_cover_generation_messages() {
        let formatter = ConsoleFormatter;
        let path = PathBuf::from("/path/to/article.md");

        let generation_msg = formatter.format_cover_generation(&path);
        assert!(generation_msg.contains("generating cover"));
        assert!(generation_msg.contains("article.md"));

        let success_msg = formatter.format_cover_success("cover_image.png");
        assert!(success_msg.contains("cover generated"));
        assert!(success_msg.contains("cover_image.png"));

        let failure_msg = formatter.format_cover_failure();
        assert!(failure_msg.contains("cover generation failed"));
    }
}
