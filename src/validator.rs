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
    match normalize_comments(input) {
        Ok(processed_text) => ValidationResult::new(processed_text),
        Err(error) => ValidationResult::new(input.to_string()).with_error(error),
    }

    // TODO: Phase 2: Basic syntax validation
    // TODO: Phase 3: Character validation
}

/// Replace nested comments with whitespace before bootstrap parsing.
///
/// Comments are grammar spacing in ixml. Replacing a complete comment with one
/// space keeps comments usable as required spacing between tokens without making
/// the bootstrap parser explore every character of long comment bodies.
fn normalize_comments(input: &str) -> Result<String, ValidationError> {
    let mut result = String::new();
    let mut chars = input.char_indices().peekable();
    let mut quote = None;

    while let Some((pos, ch)) = chars.next() {
        if let Some(active_quote) = quote {
            result.push(ch);

            if ch == active_quote {
                if matches!(chars.peek(), Some((_, next)) if *next == active_quote) {
                    let (_, doubled_quote) = chars.next().expect("peeked quote should exist");
                    result.push(doubled_quote);
                } else {
                    quote = None;
                }
            }
            continue;
        }

        match ch {
            '{' => {
                result.push_str(&comment_replacement(&mut chars, pos)?);
            }
            '"' | '\'' => {
                quote = Some(ch);
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }

    Ok(result)
}

fn comment_replacement<I>(
    chars: &mut std::iter::Peekable<I>,
    start_pos: usize,
) -> Result<String, ValidationError>
where
    I: Iterator<Item = (usize, char)>,
{
    let mut replacement = String::from(" ");
    let mut depth = 1usize;

    for (_, ch) in chars.by_ref() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(replacement);
                }
            }
            '\n' | '\r' => replacement.push(ch),
            _ => {}
        }
    }

    Err(ValidationError {
        kind: ValidationErrorKind::UncloseComment,
        position: Some(start_pos),
        message: format!("Unclosed comment starting at position {}", start_pos),
    })
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

    #[test]
    fn comments_become_spacing() {
        let input = "A{comment}B.";
        let result = validate_ixml(input);

        assert!(result.is_valid());
        assert_eq!(result.processed_text, "A B.");
    }

    #[test]
    fn nested_comments_become_spacing() {
        let input = "A {outer {inner} still outer} = \"a\".";
        let result = validate_ixml(input);

        assert!(result.is_valid());
        assert_eq!(result.processed_text, "A   = \"a\".");
    }

    #[test]
    fn comments_preserve_line_breaks() {
        let input = "A:{one\n two} \"a\".";
        let result = validate_ixml(input);

        assert!(result.is_valid());
        assert_eq!(result.processed_text, "A: \n \"a\".");
    }

    #[test]
    fn braces_inside_strings_are_not_comments() {
        let input = "A: \"{\"; '''}'''. {comment}";
        let result = validate_ixml(input);

        assert!(result.is_valid());
        assert_eq!(result.processed_text, "A: \"{\"; '''}'''.  ");
    }

    #[test]
    fn unterminated_comment_is_validation_error() {
        let input = "A: \"a\". {unterminated";
        let result = validate_ixml(input);

        assert!(!result.is_valid());
        assert!(matches!(
            result.errors[0].kind,
            ValidationErrorKind::UncloseComment
        ));
    }
}
