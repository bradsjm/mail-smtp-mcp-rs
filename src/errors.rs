use rmcp::model::ErrorData;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    ConfigMissing,
    ValidationError,
    SendDisabled,
    PolicyViolation,
    AttachmentError,
    SmtpError,
    UnknownError,
}

impl ErrorCode {
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

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0}")]
    ConfigMissing(String),
    #[error("{0}")]
    ValidationError(String),
    #[error("{0}")]
    SendDisabled(String),
    #[error("{0}")]
    PolicyViolation(String),
    #[error("{0}")]
    AttachmentError(String),
    #[error("{0}")]
    SmtpError(String),
    #[error("{0}")]
    UnknownError(String),
}

impl AppError {
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
