//! Utility functions and helpers.
//!
//! This module provides security-focused utilities with input validation
//! and safe path handling to prevent common vulnerabilities.

use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;
use std::{collections::HashSet, ffi::OsStr};
use tracing::warn;

/// Checks if a file exists and is readable with path validation.
/// Returns false for invalid or potentially dangerous paths.
pub async fn file_exists(path: &Path) -> bool {
    // Validate path for security
    if !is_safe_path(path) {
        warn!("Unsafe path access attempt: {:?}", path);
        return false;
    }

    tokio::fs::metadata(path).await.is_ok()
}

/// Gets the file extension from a path.
pub fn get_file_extension(path: &Path) -> Option<&str> {
    path.extension().and_then(|ext| ext.to_str())
}

/// Validates that a path points to a markdown file.
pub fn is_markdown_file(path: &Path) -> bool {
    match get_file_extension(path) {
        Some(ext) => matches!(ext.to_lowercase().as_str(), "md" | "markdown"),
        None => false,
    }
}

/// Validates that a path points to an image file.
pub fn is_image_file(path: &Path) -> bool {
    match get_file_extension(path) {
        Some(ext) => matches!(
            ext.to_lowercase().as_str(),
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp"
        ),
        None => false,
    }
}

/// Validates WeChat app credentials format.
pub fn validate_app_credentials(app_id: &str, app_secret: &str) -> Result<(), String> {
    if app_id.is_empty() {
        return Err("App ID cannot be empty".to_string());
    }

    if app_secret.is_empty() {
        return Err("App secret cannot be empty".to_string());
    }

    // WeChat app IDs typically start with "wx" and are 18 characters long
    if !app_id.starts_with("wx") || app_id.len() != 18 {
        return Err(
            "Invalid app ID format (should start with 'wx' and be 18 characters)".to_string(),
        );
    }

    // WeChat app secrets are typically 32 characters long
    if app_secret.len() != 32 {
        return Err("Invalid app secret format (should be 32 characters)".to_string());
    }

    Ok(())
}

/// Set of dangerous file extensions that should be blocked.
static DANGEROUS_EXTENSIONS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let mut set = HashSet::new();
    set.insert("exe");
    set.insert("bat");
    set.insert("cmd");
    set.insert("com");
    set.insert("scr");
    set.insert("pif");
    set.insert("vbs");
    set.insert("js");
    set.insert("jse");
    set.insert("wsf");
    set.insert("wsh");
    set.insert("msi");
    set.insert("dll");
    set.insert("scf");
    set.insert("lnk");
    set.insert("inf");
    set.insert("reg");
    set
});

/// Validates that a path is safe to access (prevents path traversal and dangerous files).
pub fn is_safe_path(path: &Path) -> bool {
    // Allow files in system temp directories
    if let Some(path_str) = path.to_str()
        && (path_str.contains("/tmp/")
            || path_str.contains("/var/folders/")
            || path_str.contains("\\Temp\\"))
    {
        // Still check for dangerous extensions in temp files
        if let Some(extension) = path.extension().and_then(OsStr::to_str)
            && DANGEROUS_EXTENSIONS.contains(&extension.to_lowercase().as_str())
        {
            return false;
        }
        return true;
    }
    // Check for dangerous file extensions
    if let Some(extension) = path.extension().and_then(OsStr::to_str)
        && DANGEROUS_EXTENSIONS.contains(&extension.to_lowercase().as_str())
    {
        return false;
    }

    // Check each component of the path
    for component in path.components() {
        match component {
            Component::ParentDir => {
                // Allow parent dir components, but they will be validated during resolution
                continue;
            }
            Component::Normal(name) => {
                let name_str = name.to_string_lossy();

                // Check for hidden files (starting with .)
                if name_str.starts_with('.') && name_str.len() > 1 {
                    // Allow common hidden files and temp file patterns
                    if !matches!(name_str.as_ref(), ".gitignore" | ".env" | ".dockerignore")
                        && !name_str.starts_with(".tmp")
                    {
                        return false;
                    }
                }

                // Check for null bytes and other dangerous characters
                if name_str.contains('\0') || name_str.contains('\x01') {
                    return false;
                }

                // Check for reserved names on Windows
                if is_reserved_name(&name_str) {
                    return false;
                }
            }
            Component::RootDir | Component::CurDir => {
                // These are generally safe
                continue;
            }
            Component::Prefix(_) => {
                // Windows drive prefixes are generally safe
                continue;
            }
        }
    }

    true
}

/// Checks if a filename is a Windows reserved name.
fn is_reserved_name(name: &str) -> bool {
    let upper_name = name.to_uppercase();
    let base_name = upper_name.split('.').next().unwrap_or("");

    matches!(
        base_name,
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

/// Checks if a path contains potential traversal sequences.
pub fn has_path_traversal(path: &str) -> bool {
    // Check for common traversal patterns
    path.contains("../")
        || path.contains("..\\")
        || path.contains("/..")
        || path.contains("\\..")
        || path.contains("....")
        || path == ".."
}

/// Sanitizes a filename by removing or replacing dangerous characters.
pub fn sanitize_filename(filename: &str) -> String {
    let mut sanitized = filename
        .chars()
        .filter(|&c| !matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\0'..='\x1F'))
        .collect::<String>();

    // Replace path separators with underscores
    sanitized = sanitized.replace(['/', '\\'], "_");

    // Ensure it doesn't start with a dot (hidden file)
    if sanitized.starts_with('.') && sanitized.len() > 1 {
        sanitized = format!("_{}", &sanitized[1..]);
    }

    // Ensure it's not empty
    if sanitized.is_empty() {
        sanitized = "unnamed".to_string();
    }

    // Truncate if too long
    if sanitized.len() > 255 {
        sanitized.truncate(252);
        sanitized.push_str("...");
    }

    sanitized
}

/// Validates file size limits to prevent DoS attacks.
pub fn validate_file_size(size: u64, max_size: u64, file_type: &str) -> Result<(), String> {
    if size > max_size {
        return Err(format!(
            "{file_type} file too large: {size} bytes (max: {max_size} bytes)"
        ));
    }
    Ok(())
}

/// Extracts the base directory from a file path.
pub fn get_base_directory(file_path: &Path) -> Option<&Path> {
    file_path.parent()
}

/// Resolves relative paths against a base directory with security validation.
/// Prevents path traversal attacks by validating the resolved path.
pub fn resolve_path(base_dir: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let relative = Path::new(relative_path);

    // Check for absolute paths
    if relative.is_absolute() {
        if !is_safe_path(relative) {
            return Err("Absolute path contains unsafe components".to_string());
        }
        return Ok(PathBuf::from(relative_path));
    }

    // Resolve relative path
    let resolved = base_dir.join(relative_path);

    // Validate the resolved path
    if !is_safe_path(&resolved) {
        return Err("Resolved path contains unsafe components".to_string());
    }

    // Ensure the resolved path is still under the base directory
    match resolved.canonicalize() {
        Ok(canonical_resolved) => {
            match base_dir.canonicalize() {
                Ok(canonical_base) => {
                    if canonical_resolved.starts_with(&canonical_base) {
                        Ok(resolved)
                    } else {
                        Err("Path traversal attempt detected".to_string())
                    }
                }
                Err(_) => {
                    // Base directory doesn't exist or can't be canonicalized
                    // Fall back to basic validation
                    if has_path_traversal(relative_path) {
                        Err("Path contains traversal sequences".to_string())
                    } else {
                        Ok(resolved)
                    }
                }
            }
        }
        Err(_) => {
            // File doesn't exist yet, validate the path structure
            if has_path_traversal(relative_path) {
                Err("Path contains traversal sequences".to_string())
            } else {
                Ok(resolved)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_get_file_extension() {
        assert_eq!(get_file_extension(Path::new("test.md")), Some("md"));
        assert_eq!(
            get_file_extension(Path::new("test.markdown")),
            Some("markdown")
        );
        assert_eq!(get_file_extension(Path::new("test.jpg")), Some("jpg"));
        assert_eq!(get_file_extension(Path::new("test")), None);
        assert_eq!(get_file_extension(Path::new(".gitignore")), None);
    }

    #[test]
    fn test_is_markdown_file() {
        assert!(is_markdown_file(Path::new("test.md")));
        assert!(is_markdown_file(Path::new("test.markdown")));
        assert!(is_markdown_file(Path::new("TEST.MD")));
        assert!(!is_markdown_file(Path::new("test.txt")));
        assert!(!is_markdown_file(Path::new("test")));
    }

    #[test]
    fn test_is_image_file() {
        assert!(is_image_file(Path::new("test.jpg")));
        assert!(is_image_file(Path::new("test.PNG")));
        assert!(is_image_file(Path::new("test.gif")));
        assert!(!is_image_file(Path::new("test.txt")));
        assert!(!is_image_file(Path::new("test")));
    }

    #[test]
    fn test_validate_app_credentials() {
        assert!(
            validate_app_credentials("wx1234567890123456", "12345678901234567890123456789012")
                .is_ok()
        );

        assert!(validate_app_credentials("", "12345678901234567890123456789012").is_err());
        assert!(validate_app_credentials("invalid", "12345678901234567890123456789012").is_err());
        assert!(validate_app_credentials("wx123", "12345678901234567890123456789012").is_err());

        assert!(validate_app_credentials("wx1234567890123456", "").is_err());
        assert!(validate_app_credentials("wx1234567890123456", "short").is_err());
    }

    #[test]
    fn test_get_base_directory() {
        assert_eq!(
            get_base_directory(Path::new("/path/to/file.md")),
            Some(Path::new("/path/to"))
        );
        assert_eq!(
            get_base_directory(Path::new("file.md")),
            Some(Path::new(""))
        );
        assert_eq!(get_base_directory(Path::new("/")), None);
    }

    #[test]
    fn test_resolve_path() {
        let base = Path::new("/base/dir");

        assert_eq!(
            resolve_path(base, "relative.md").unwrap(),
            PathBuf::from("/base/dir/relative.md")
        );

        assert_eq!(
            resolve_path(base, "/absolute.md").unwrap(),
            PathBuf::from("/absolute.md")
        );

        assert_eq!(
            resolve_path(base, "./relative.md").unwrap(),
            PathBuf::from("/base/dir/./relative.md")
        );

        assert!(resolve_path(base, "../../../etc/passwd").is_err());
        assert!(resolve_path(base, "..\\..\\windows\\system32").is_err());

        assert!(resolve_path(base, "malware.exe").is_err());
        assert!(resolve_path(base, "script.bat").is_err());
    }

    #[test]
    fn test_is_safe_path() {
        assert!(is_safe_path(Path::new("document.md")));
        assert!(is_safe_path(Path::new("image.jpg")));
        assert!(is_safe_path(Path::new("folder/file.txt")));

        assert!(!is_safe_path(Path::new("malware.exe")));
        assert!(!is_safe_path(Path::new("script.bat")));
        assert!(!is_safe_path(Path::new("virus.scr")));

        assert!(!is_safe_path(Path::new("CON")));
        assert!(!is_safe_path(Path::new("PRN.txt")));
        assert!(!is_safe_path(Path::new("COM1.dat")));

        assert!(!is_safe_path(Path::new(".hidden")));
        assert!(is_safe_path(Path::new(".gitignore")));
        assert!(is_safe_path(Path::new(".env")));
    }

    #[test]
    fn test_has_path_traversal() {
        assert!(has_path_traversal("../etc/passwd"));
        assert!(has_path_traversal("..\\windows\\system32"));
        assert!(has_path_traversal("folder/../../../etc"));
        assert!(has_path_traversal(".."));
        assert!(has_path_traversal("...."));

        assert!(!has_path_traversal("normal/path/file.txt"));
        assert!(!has_path_traversal("file.md"));
        assert!(!has_path_traversal("folder/subfolder/file"));
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("normal_file.txt"), "normal_file.txt");

        assert_eq!(sanitize_filename("file<>:\"|?*.txt"), "file.txt");

        assert_eq!(sanitize_filename("path/to/file.txt"), "path_to_file.txt");
        assert_eq!(sanitize_filename("path\\to\\file.txt"), "path_to_file.txt");

        assert_eq!(sanitize_filename(".hidden"), "_hidden");
        assert_eq!(sanitize_filename(""), "unnamed");

        // Test very long filename
        let long_name = "a".repeat(300);
        let sanitized = sanitize_filename(&long_name);
        assert!(sanitized.len() <= 255);
        assert!(sanitized.ends_with("..."));
    }

    #[test]
    fn test_validate_file_size() {
        // Test valid size
        assert!(validate_file_size(1000, 2000, "test").is_ok());

        // Test oversized file
        let result = validate_file_size(3000, 2000, "image");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("image file too large"));
    }
}
