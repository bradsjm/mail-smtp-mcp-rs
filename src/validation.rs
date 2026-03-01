use std::sync::OnceLock;

use base64::Engine;
use regex::Regex;

use crate::errors::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageSizeParts {
    pub subject_bytes: usize,
    pub text_bytes: usize,
    pub html_bytes: usize,
    pub attachment_bytes: usize,
    pub attachment_count: usize,
}

pub fn contains_carriage_return_or_line_feed(value: &str) -> bool {
    value.contains('\n') || value.contains('\r')
}

pub fn validate_email_address(value: &str) -> Result<(), AppError> {
    if contains_carriage_return_or_line_feed(value) {
        return Err(AppError::ValidationError(
            "Email address contains invalid line breaks.".to_owned(),
        ));
    }

    if !email_regex().is_match(value.trim()) {
        return Err(AppError::ValidationError(format!(
            "Invalid email address: {}",
            value.trim()
        )));
    }

    Ok(())
}

pub fn normalize_address(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub fn email_domain(value: &str) -> Option<&str> {
    let normalized = value.trim();
    let (_, domain) = normalized.split_once('@')?;
    if domain.is_empty() {
        return None;
    }
    Some(domain)
}

pub fn is_safe_filename(value: &str) -> bool {
    if value.is_empty() || value.len() > 256 {
        return false;
    }
    if value == "." || value == ".." || value.contains('/') || value.contains('\\') {
        return false;
    }
    if value.contains("..") {
        return false;
    }
    !contains_carriage_return_or_line_feed(value)
}

pub fn decode_base64_strict(input: &str) -> Result<Vec<u8>, AppError> {
    if input.trim() != input {
        return Err(AppError::AttachmentError(
            "Invalid base64 content for attachment.".to_owned(),
        ));
    }

    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|_| AppError::AttachmentError("Invalid base64 content for attachment.".to_owned()))
}

pub fn estimate_message_bytes(parts: MessageSizeParts) -> usize {
    const BASE_HEADERS_OVERHEAD: usize = 1024;
    const MULTIPART_BOUNDARY_OVERHEAD: usize = 256;
    const PER_ATTACHMENT_OVERHEAD: usize = 512;

    BASE_HEADERS_OVERHEAD
        + parts.subject_bytes
        + parts.text_bytes
        + parts.html_bytes
        + parts.attachment_bytes
        + MULTIPART_BOUNDARY_OVERHEAD
        + (parts.attachment_count * PER_ATTACHMENT_OVERHEAD)
}

fn email_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)^[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}$")
            .expect("email regex must compile")
    })
}

#[cfg(test)]
mod tests {
    use super::{
        MessageSizeParts, contains_carriage_return_or_line_feed, decode_base64_strict,
        estimate_message_bytes, is_safe_filename, validate_email_address,
    };
    use crate::errors::ErrorCode;

    #[test]
    fn detects_header_injection_characters() {
        assert!(contains_carriage_return_or_line_feed("hello\nworld"));
        assert!(contains_carriage_return_or_line_feed("hello\rworld"));
        assert!(!contains_carriage_return_or_line_feed("hello world"));
    }

    #[test]
    fn validates_email_addresses() {
        validate_email_address("user@example.com").expect("must accept valid address");

        let err =
            validate_email_address("broken-address").expect_err("must reject invalid address");
        assert_eq!(err.code(), ErrorCode::ValidationError);
    }

    #[test]
    fn validates_safe_filename() {
        assert!(is_safe_filename("report.pdf"));
        assert!(!is_safe_filename("../passwd"));
        assert!(!is_safe_filename("nested/path.txt"));
    }

    #[test]
    fn decodes_base64_strictly() {
        let decoded = decode_base64_strict("aGVsbG8=").expect("must decode");
        assert_eq!(decoded, b"hello");

        let err = decode_base64_strict(" aGVsbG8=").expect_err("must reject leading whitespace");
        assert_eq!(err.code(), ErrorCode::AttachmentError);
    }

    #[test]
    fn estimates_message_size_with_overheads() {
        let size = estimate_message_bytes(MessageSizeParts {
            subject_bytes: 10,
            text_bytes: 20,
            html_bytes: 30,
            attachment_bytes: 40,
            attachment_count: 2,
        });

        assert!(size > 100);
    }
}
