//! iXML grammar validation and preprocessing
//!
//! This module handles validation and preprocessing of iXML grammar text before parsing.
//! It includes comment stripping, syntax validation, and error reporting.

use std::fmt;

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub processed_text: String,
    pub warnings: Vec<ValidationWarning>,
    pub errors: Vec<ValidationError>,
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn new(processed_text: String) -> Self {
        Self {
            processed_text,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn with_error(mut self, error: ValidationError) -> Self {
        self.errors.push(error);
        self
    }

    pub fn with_warning(mut self, warning: ValidationWarning) -> Self {
        self.warnings.push(warning);
        self
    }
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub kind: ValidationErrorKind,
    pub position: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum ValidationErrorKind {
    UncloseComment,
    InvalidCharacter,
    SyntaxError,
}

#[derive(Debug, Clone)]
pub struct ValidationWarning {
    pub message: String,
    pub position: Option<usize>,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.position {
            Some(pos) => write!(f, "Validation error at position {}: {}", pos, self.message),
            None => write!(f, "Validation error: {}", self.message),
        }
    }
}

/// Main validation entry point
pub fn validate_ixml(input: &str) -> ValidationResult {
    // Comments are handled directly by the Earley parser in context-aware manner
    ValidationResult::new(input.to_string())

    // TODO: Phase 2: Basic syntax validation
    // TODO: Phase 3: Character validation
}

/// Strip nested comments {...} from iXML text
#[allow(dead_code)]
fn strip_comments(input: &str) -> Result<String, ValidationError> {
    let mut result = String::new();
    let mut chars = input.char_indices().peekable();

    while let Some((pos, ch)) = chars.next() {
        if ch == '{' {
            // Start of comment - skip until matching }
            let mut depth = 1;
            let start_pos = pos;

            while let Some((_, inner_ch)) = chars.next() {
                if inner_ch == '{' {
                    depth += 1;
                } else if inner_ch == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break; // Found matching close brace
                    }
                }
            }

            // Check if we found the matching close brace
            if depth > 0 {
                return Err(ValidationError {
                    kind: ValidationErrorKind::UncloseComment,
                    position: Some(start_pos),
                    message: format!("Unclosed comment starting at position {}", start_pos),
                });
            }

            // Comment successfully stripped - don't add anything to result
        } else {
            result.push(ch);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_comments() {
        let input = "rule: \"a\".";
        let result = validate_ixml(input);
        assert!(result.is_valid());
        assert_eq!(result.processed_text, "rule: \"a\".");
    }

    // TODO: Comment processing tests removed - current pre-stripping approach is flawed.
    // Comments should be parsed context-aware within the Earley parser, not pre-stripped,
    // since { and } can appear in quoted strings and other contexts.
}
