use rmcp::model::ErrorData;
use serde_json::json;
use thiserror::Error;

/// Error codes representing different failure types in the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// Configuration is missing or incomplete.
    ConfigMissing,
    /// Input validation failed.
    ValidationError,
    /// Sending is disabled by policy.
    SendDisabled,
    /// Policy violation occurred.
    PolicyViolation,
    /// Attachment-related error.
    AttachmentError,
    /// SMTP protocol error.
    SmtpError,
    /// Unknown or unexpected error.
    UnknownError,
}

impl ErrorCode {
    /// Returns the string representation of the error code.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConfigMissing => "CONFIG_MISSING",
            Self::ValidationError => "VALIDATION_ERROR",
            Self::SendDisabled => "SEND_DISABLED",
            Self::PolicyViolation => "POLICY_VIOLATION",
            Self::AttachmentError => "ATTACHMENT_ERROR",
            Self::SmtpError => "SMTP_ERROR",
            Self::UnknownError => "UNKNOWN_ERROR",
        }
    }
}

/// Application error type encompassing all error cases.
#[derive(Debug, Error)]
pub enum AppError {
    /// Configuration is missing or incomplete.
    #[error("{0}")]
    ConfigMissing(String),
    /// Input validation failed.
    #[error("{0}")]
    ValidationError(String),
    /// Sending is disabled by policy.
    #[error("{0}")]
    SendDisabled(String),
    /// Policy violation occurred.
    #[error("{0}")]
    PolicyViolation(String),
    /// Attachment-related error.
    #[error("{0}")]
    AttachmentError(String),
    /// SMTP protocol error.
    #[error("{0}")]
    SmtpError(String),
    /// Unknown or unexpected error.
    #[error("{0}")]
    UnknownError(String),
}

impl AppError {
    /// Returns the error code associated with this error.
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::ConfigMissing(_) => ErrorCode::ConfigMissing,
            Self::ValidationError(_) => ErrorCode::ValidationError,
            Self::SendDisabled(_) => ErrorCode::SendDisabled,
            Self::PolicyViolation(_) => ErrorCode::PolicyViolation,
            Self::AttachmentError(_) => ErrorCode::AttachmentError,
            Self::SmtpError(_) => ErrorCode::SmtpError,
            Self::UnknownError(_) => ErrorCode::UnknownError,
        }
    }

    /// Returns the error message associated with this error.
    pub fn message(&self) -> &str {
        match self {
            Self::ConfigMissing(message)
            | Self::ValidationError(message)
            | Self::SendDisabled(message)
            | Self::PolicyViolation(message)
            | Self::AttachmentError(message)
            | Self::SmtpError(message)
            | Self::UnknownError(message) => message,
        }
    }

    /// Converts this error into an `ErrorData` structure for API responses.
    pub fn to_error_data(&self) -> ErrorData {
        let message = self.message().to_owned();
        let data = Some(json!({ "code": self.code().as_str() }));

        match self.code() {
            ErrorCode::ValidationError => ErrorData::invalid_params(message, data),
            ErrorCode::ConfigMissing
            | ErrorCode::SendDisabled
            | ErrorCode::PolicyViolation
            | ErrorCode::AttachmentError => ErrorData::invalid_request(message, data),
            ErrorCode::SmtpError | ErrorCode::UnknownError => {
                ErrorData::internal_error(message, data)
            }
        }
    }
}
