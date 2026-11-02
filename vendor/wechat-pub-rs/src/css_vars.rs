//! CSS variable parser and replacement module for inlining CSS custom properties.
//!
//! This module provides functionality to parse CSS variables from :root blocks,
//! resolve nested variable references, and replace all var(--variable-name)
//! occurrences with their resolved values for better WeChat editor compatibility.

use regex::Regex;
use std::collections::HashMap;
use thiserror::Error;

/// Errors that can occur during CSS variable processing.
#[derive(Error, Debug)]
pub enum CssVarError {
    #[error("Circular reference detected in CSS variables: {0}")]
    CircularReference(String),
    #[error("Undefined CSS variable: {0}")]
    UndefinedVariable(String),
    #[error("Invalid CSS variable syntax: {0}")]
    InvalidSyntax(String),
}

/// A parsed CSS variable with its name and value.
#[derive(Debug, Clone, PartialEq)]
pub struct CssVariable {
    pub name: String,
    pub value: String,
}

impl CssVariable {
    /// Creates a new CSS variable.
    pub fn new(name: String, value: String) -> Self {
        Self { name, value }
    }
}

/// CSS variable parser and processor.
#[derive(Debug)]
pub struct CssVariableProcessor {
    /// Compiled regex for matching :root blocks
    root_regex: Regex,
    /// Compiled regex for matching variable declarations
    var_decl_regex: Regex,
    /// Compiled regex for matching var() with fallback values
    var_fallback_regex: Regex,
}

impl CssVariableProcessor {
    /// Creates a new CSS variable processor with compiled regex patterns.
    pub fn new() -> Self {
        Self {
            // Match :root blocks with any whitespace and content
            root_regex: Regex::new(r":root\s*\{([^}]*)\}").unwrap(),
            // Match CSS variable declarations: --variable-name: value;
            var_decl_regex: Regex::new(r"--([^:]+):\s*([^;]+);").unwrap(),
            // Match var() with fallback: var(--variable-name, fallback)
            var_fallback_regex: Regex::new(r"var\(--([^,)]+)(?:,\s*([^)]+))?\)").unwrap(),
        }
    }

    /// Parses CSS variables from :root blocks in the given CSS content.
    ///
    /// # Arguments
    /// * `css_content` - The CSS content to parse
    ///
    /// # Returns
    /// A HashMap mapping variable names to their values
    ///
    /// # Example
    /// ```rust
    /// use wechat_pub_rs::css_vars::CssVariableProcessor;
    ///
    /// let processor = CssVariableProcessor::new();
    /// let css = ":root { --primary-color: #4870ac; --text-color: #40464f; }";
    /// let variables = processor.parse_variables(css).unwrap();
    ///
    /// assert_eq!(variables.get("primary-color"), Some(&"#4870ac".to_string()));
    /// assert_eq!(variables.get("text-color"), Some(&"#40464f".to_string()));
    /// ```
    pub fn parse_variables(
        &self,
        css_content: &str,
    ) -> Result<HashMap<String, String>, CssVarError> {
        let mut variables = HashMap::new();

        // Find all :root blocks
        for root_match in self.root_regex.captures_iter(css_content) {
            if let Some(root_content) = root_match.get(1) {
                // Parse variable declarations within this :root block
                for var_match in self.var_decl_regex.captures_iter(root_content.as_str()) {
                    if let (Some(name), Some(value)) = (var_match.get(1), var_match.get(2)) {
                        let var_name = name.as_str().trim().to_string();
                        let var_value = value.as_str().trim().to_string();
                        variables.insert(var_name, var_value);
                    }
                }
            }
        }

        Ok(variables)
    }

    /// Resolves CSS variables, handling nested variable references.
    ///
    /// This method processes variables that reference other variables and resolves
    /// them to their final values, detecting circular references.
    ///
    /// # Arguments
    /// * `variables` - HashMap of raw variable declarations
    ///
    /// # Returns
    /// A HashMap with all variables resolved to their final values
    ///
    /// # Example
    /// ```rust
    /// use std::collections::HashMap;
    /// use wechat_pub_rs::css_vars::CssVariableProcessor;
    ///
    /// let processor = CssVariableProcessor::new();
    /// let mut variables = HashMap::new();
    /// variables.insert("primary-color".to_string(), "#4870ac".to_string());
    /// variables.insert("header-color".to_string(), "var(--primary-color)".to_string());
    ///
    /// let resolved = processor.resolve_variables(variables).unwrap();
    /// assert_eq!(resolved.get("header-color"), Some(&"#4870ac".to_string()));
    /// ```
    pub fn resolve_variables(
        &self,
        variables: HashMap<String, String>,
    ) -> Result<HashMap<String, String>, CssVarError> {
        let mut resolved = HashMap::new();
        let mut resolving = std::collections::HashSet::new();

        // Create a copy of variable names to iterate over
        let var_names: Vec<String> = variables.keys().cloned().collect();

        for var_name in var_names {
            self.resolve_variable(&var_name, &variables, &mut resolved, &mut resolving)?;
        }

        Ok(resolved)
    }

    /// Recursively resolves a single variable, detecting circular references.
    fn resolve_variable(
        &self,
        var_name: &str,
        variables: &HashMap<String, String>,
        resolved: &mut HashMap<String, String>,
        resolving: &mut std::collections::HashSet<String>,
    ) -> Result<String, CssVarError> {
        // If already resolved, return the cached value
        if let Some(value) = resolved.get(var_name) {
            return Ok(value.clone());
        }

        // Check for circular reference
        if resolving.contains(var_name) {
            return Err(CssVarError::CircularReference(var_name.to_string()));
        }

        // Get the variable value
        let var_value = variables
            .get(var_name)
            .ok_or_else(|| CssVarError::UndefinedVariable(var_name.to_string()))?;

        // Add to resolving set to detect cycles
        resolving.insert(var_name.to_string());

        // Resolve any var() references in the value
        let resolved_value =
            self.replace_var_references(var_value, variables, resolved, resolving)?;

        // Remove from resolving set and cache the result
        resolving.remove(var_name);
        resolved.insert(var_name.to_string(), resolved_value.clone());

        Ok(resolved_value)
    }

    /// Replaces var() references in a value string with their resolved values.
    fn replace_var_references(
        &self,
        value: &str,
        variables: &HashMap<String, String>,
        resolved: &mut HashMap<String, String>,
        resolving: &mut std::collections::HashSet<String>,
    ) -> Result<String, CssVarError> {
        let mut result = value.to_string();

        // Process all var() calls, including those with fallbacks
        for captures in self.var_fallback_regex.captures_iter(value) {
            if let Some(var_ref) = captures.get(1) {
                let referenced_var = var_ref.as_str().trim();
                let fallback = captures.get(2).map(|m| m.as_str().trim());

                // Try to resolve the referenced variable
                let replacement = if variables.contains_key(referenced_var) {
                    self.resolve_variable(referenced_var, variables, resolved, resolving)?
                } else if let Some(fallback_value) = fallback {
                    // Use fallback value, which might also contain var() references
                    self.replace_var_references(fallback_value, variables, resolved, resolving)?
                } else {
                    return Err(CssVarError::UndefinedVariable(referenced_var.to_string()));
                };

                // Replace the entire var() call with the resolved value
                let var_call = captures.get(0).unwrap().as_str();
                result = result.replace(var_call, &replacement);
            }
        }

        Ok(result)
    }

    /// Processes CSS content by inlining all CSS variables.
    ///
    /// This is the main entry point that parses variables, resolves them,
    /// and replaces all var() calls with their resolved values.
    ///
    /// # Arguments
    /// * `css_content` - The CSS content to process
    ///
    /// # Returns
    /// CSS content with all variables inlined
    ///
    /// # Example
    /// ```rust
    /// use wechat_pub_rs::css_vars::CssVariableProcessor;
    ///
    /// let processor = CssVariableProcessor::new();
    /// let css = r#"
    /// :root {
    ///     --primary-color: #4870ac;
    ///     --header-color: var(--primary-color);
    /// }
    /// .header { color: var(--header-color); }
    /// "#;
    ///
    /// let processed = processor.process_css(css).unwrap();
    /// assert!(processed.contains("color: #4870ac"));
    /// assert!(!processed.contains("var("));
    /// ```
    pub fn process_css(&self, css_content: &str) -> Result<String, CssVarError> {
        // Parse variables from :root blocks
        let raw_variables = self.parse_variables(css_content)?;

        // Resolve all variable references
        let resolved_variables = self.resolve_variables(raw_variables)?;

        // Replace all var() calls with resolved values
        let mut processed_css = css_content.to_string();

        // Replace var() calls throughout the CSS
        for captures in self.var_fallback_regex.captures_iter(css_content) {
            let var_call = captures.get(0).unwrap().as_str();

            if let Some(var_ref) = captures.get(1) {
                let var_name = var_ref.as_str().trim();

                if let Some(resolved_value) = resolved_variables.get(var_name) {
                    processed_css = processed_css.replace(var_call, resolved_value);
                } else if let Some(fallback) = captures.get(2) {
                    // Use fallback value if variable not found
                    let fallback_value = fallback.as_str().trim();
                    processed_css = processed_css.replace(var_call, fallback_value);
                }
                // If no fallback and variable not found, leave the var() call as-is
                // This allows for graceful degradation
            }
        }

        Ok(processed_css)
    }
}

impl Default for CssVariableProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_variables() {
        let processor = CssVariableProcessor::new();
        let css = r#"
        :root {
            --primary-color: #4870ac;
            --text-color: #40464f;
            --bg-color: #ffffff;
        }
        "#;

        let variables = processor.parse_variables(css).unwrap();

        assert_eq!(variables.get("primary-color"), Some(&"#4870ac".to_string()));
        assert_eq!(variables.get("text-color"), Some(&"#40464f".to_string()));
        assert_eq!(variables.get("bg-color"), Some(&"#ffffff".to_string()));
    }

    #[test]
    fn test_parse_nested_variables() {
        let processor = CssVariableProcessor::new();
        let css = r#"
        :root {
            --primary-color: #4870ac;
            --header-color: var(--primary-color);
            --shadow: 3px 3px 10px var(--shadow-color);
            --shadow-color: #eee;
        }
        "#;

        let variables = processor.parse_variables(css).unwrap();
        let resolved = processor.resolve_variables(variables).unwrap();

        assert_eq!(resolved.get("primary-color"), Some(&"#4870ac".to_string()));
        assert_eq!(resolved.get("header-color"), Some(&"#4870ac".to_string()));
        assert_eq!(
            resolved.get("shadow"),
            Some(&"3px 3px 10px #eee".to_string())
        );
    }

    #[test]
    fn test_circular_reference_detection() {
        let processor = CssVariableProcessor::new();
        let mut variables = HashMap::new();
        variables.insert("a".to_string(), "var(--b)".to_string());
        variables.insert("b".to_string(), "var(--a)".to_string());

        let result = processor.resolve_variables(variables);
        assert!(matches!(result, Err(CssVarError::CircularReference(_))));
    }

    #[test]
    fn test_var_with_fallback() {
        let processor = CssVariableProcessor::new();
        let css = r#"
        :root {
            --existing-color: #4870ac;
        }
        .test {
            color: var(--non-existent, #ff0000);
            background: var(--existing-color, #000000);
        }
        "#;

        let processed = processor.process_css(css).unwrap();

        assert!(processed.contains("color: #ff0000"));
        assert!(processed.contains("background: #4870ac"));
    }

    #[test]
    fn test_complete_css_processing() {
        let processor = CssVariableProcessor::new();
        let css = r#"
        :root {
            --text-color: #40464f;
            --primary-color: #4870ac;
            --bg-color: #ffffff;
            --marker-color: #a2b6d4;
            --source-color: #a8a8a9;
            --header-span-color: var(--primary-color);
            --block-bg-color: #f6f8fa;
        }

        #wepub {
            color: var(--text-color);
            background-color: var(--bg-color);
        }

        #wepub h1 {
            color: var(--header-span-color);
        }

        #wepub blockquote {
            background-color: var(--block-bg-color);
            border-left: 4px solid var(--marker-color);
        }
        "#;

        let processed = processor.process_css(css).unwrap();

        // Verify that all var() calls have been replaced
        assert!(!processed.contains("var("));

        // Verify specific replacements
        assert!(processed.contains("color: #40464f"));
        assert!(processed.contains("background-color: #ffffff"));
        assert!(processed.contains("color: #4870ac")); // header-span-color resolved
        assert!(processed.contains("background-color: #f6f8fa"));
        assert!(processed.contains("border-left: 4px solid #a2b6d4"));
    }

    #[test]
    fn test_multiple_root_blocks() {
        let processor = CssVariableProcessor::new();
        let css = r#"
        :root {
            --primary-color: #4870ac;
        }

        /* Some other CSS */
        .test { margin: 10px; }

        :root {
            --secondary-color: #ff0000;
            --combined: var(--primary-color);
        }
        "#;

        let variables = processor.parse_variables(css).unwrap();
        let resolved = processor.resolve_variables(variables).unwrap();

        assert_eq!(resolved.get("primary-color"), Some(&"#4870ac".to_string()));
        assert_eq!(
            resolved.get("secondary-color"),
            Some(&"#ff0000".to_string())
        );
        assert_eq!(resolved.get("combined"), Some(&"#4870ac".to_string()));
    }

    #[test]
    fn test_complex_variable_values() {
        let processor = CssVariableProcessor::new();
        let css = r#"
        :root {
            --font-family: "Helvetica Neue", Arial, sans-serif;
            --box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
            --border: 1px solid var(--border-color);
            --border-color: #e1e4e8;
            --transition: all 0.3s ease-in-out;
        }
        "#;

        let processed = processor.process_css(css).unwrap();

        assert!(processed.contains(r#"font-family: "Helvetica Neue", Arial, sans-serif"#));
        assert!(processed.contains("box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1)"));
        assert!(processed.contains("border: 1px solid #e1e4e8"));
        assert!(processed.contains("transition: all 0.3s ease-in-out"));
    }
}
