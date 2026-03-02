use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart, header::ContentType};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::{Message, SmtpTransport, Transport};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ErrorData, ServerCapabilities, ServerInfo};
use rmcp::{Json, ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{AccountConfig, ServerConfig, list_account_metadata};
use crate::errors::AppError;
use crate::policy::{enforce_recipient_policy, normalize_recipients};
use crate::validation::{
    MessageSizeParts, contains_carriage_return_or_line_feed, decode_base64_strict,
    estimate_base64_transport_bytes, estimate_message_bytes, is_safe_filename,
    validate_email_address,
};

const HARD_MAX_RECIPIENTS: usize = 50;
const HARD_MAX_ATTACHMENTS: usize = 10;
const HARD_MAX_ATTACHMENT_BYTES: usize = 5_000_000;
const HARD_MAX_TEXT_CHARS: usize = 100_000;
const HARD_MAX_HTML_CHARS: usize = 200_000;
const HARD_MAX_SUBJECT_CHARS: usize = 256;

pub const TOOL_NAMES: [&str; 2] = ["smtp_list_accounts", "smtp_send_message"];

#[derive(Clone)]
pub struct McpServer {
    config: Arc<ServerConfig>,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Serialize, JsonSchema)]
/// Metadata for responses, including timing information.
struct Meta {
    now_utc: String,
    duration_ms: u128,
}

impl Meta {
    fn now(duration_ms: u64) -> Self {
        Self {
            now_utc: Utc::now().to_rfc3339(),
            duration_ms: u128::from(duration_ms),
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct ToolEnvelope<T>
where
    T: JsonSchema,
{
    summary: String,
    data: T,
    meta: Meta,
}

#[derive(Debug, Deserialize, JsonSchema)]
/// Input for listing accounts, optionally filtered by account ID.
struct ListAccountsInput {
    account_id: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
/// Data returned when listing accounts.
struct ListAccountsData {
    accounts: Vec<ListAccountMetadata>,
}

#[derive(Debug, Serialize, JsonSchema)]
/// Metadata for a single account in the list accounts response.
struct ListAccountMetadata {
    account_id: String,
    host: String,
    port: u16,
    secure: bool,
    default_from: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct SendMessageData {
    account_id: String,
    envelope: Envelope,
    message_id: Option<String>,
    accepted: Option<Vec<String>>,
    rejected: Option<Vec<String>>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct Envelope {
    from: String,
    to: Vec<String>,
    cc: Option<Vec<String>>,
    bcc: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(untagged)]
/// Helper enum for accepting either a single string or a list of strings.
enum StringOrList {
    One(String),
    Many(Vec<String>),
}

impl StringOrList {
    fn into_vec(self) -> Vec<String> {
        match self {
            Self::One(value) => vec![value],
            Self::Many(values) => values,
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
/// Input for an email attachment.
struct AttachmentInput {
    filename: String,
    content_base64: String,
    content_type: Option<String>,
}

/// Represents a prepared attachment with decoded bytes.
struct PreparedAttachment {
    filename: String,
    bytes: Vec<u8>,
    content_type: ContentType,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SendMessageInput {
    #[serde(default = "default_account_id")]
    account_id: String,
    from: Option<String>,
    reply_to: Option<String>,
    to: StringOrList,
    cc: Option<StringOrList>,
    bcc: Option<StringOrList>,
    subject: String,
    text_body: Option<String>,
    html_body: Option<String>,
    attachments: Option<Vec<AttachmentInput>>,
}

fn default_account_id() -> String {
    "default".to_owned()
}

#[tool_router]
impl McpServer {
    pub fn new(config: ServerConfig) -> Self {
        Self {
            config: Arc::new(config),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "smtp_list_accounts",
        description = "List configured SMTP accounts"
    )]
    async fn list_accounts(
        &self,
        Parameters(input): Parameters<ListAccountsInput>,
    ) -> Result<Json<ToolEnvelope<ListAccountsData>>, ErrorData> {
        let started = Instant::now();
        finalize_tool(started, self.list_accounts_impl(input))
    }

    #[tool(name = "smtp_send_message", description = "Send an SMTP email message")]
    async fn send_message(
        &self,
        Parameters(input): Parameters<SendMessageInput>,
    ) -> Result<Json<ToolEnvelope<SendMessageData>>, ErrorData> {
        let started = Instant::now();
        finalize_tool(started, self.send_message_impl(input).await)
    }

    fn list_accounts_impl(
        &self,
        input: ListAccountsInput,
    ) -> Result<(String, ListAccountsData), AppError> {
        let mut accounts = list_account_metadata(&self.config.accounts)
            .into_iter()
            .map(|account| ListAccountMetadata {
                account_id: account.account_id,
                host: account.host,
                port: account.port,
                secure: account.secure,
                default_from: account.default_from,
            })
            .collect::<Vec<_>>();
        if let Some(account_id) = input.account_id {
            let needle = account_id.to_ascii_lowercase();
            accounts.retain(|account| account.account_id == needle);
            if accounts.is_empty() {
                return Err(AppError::ConfigMissing(format!(
                    "Account not configured: {needle}"
                )));
            }
        }

        let summary = format!("Found {} configured SMTP account(s).", accounts.len());
        Ok((summary, ListAccountsData { accounts }))
    }

    async fn send_message_impl(
        &self,
        input: SendMessageInput,
    ) -> Result<(String, SendMessageData), AppError> {
        if !self.config.policy.send_enabled {
            return Err(AppError::SendDisabled(
                "Sending is disabled by server policy.".to_owned(),
            ));
        }

        let account = self.find_account(&input.account_id)?;
        let from = resolve_sender(&input, account)?;

        if contains_carriage_return_or_line_feed(&input.subject) {
            return Err(AppError::ValidationError(
                "Subject contains invalid line breaks.".to_owned(),
            ));
        }
        if input.subject.is_empty() || input.subject.chars().count() > HARD_MAX_SUBJECT_CHARS {
            return Err(AppError::ValidationError(format!(
                "subject must be between 1 and {HARD_MAX_SUBJECT_CHARS} characters."
            )));
        }

        let text_body = input.text_body.unwrap_or_default();
        let html_body = input.html_body.unwrap_or_default();
        if text_body.is_empty() && html_body.is_empty() {
            return Err(AppError::ValidationError(
                "Either text_body or html_body is required.".to_owned(),
            ));
        }
        if text_body.chars().count() > HARD_MAX_TEXT_CHARS {
            return Err(AppError::ValidationError(format!(
                "text_body exceeds hard max of {HARD_MAX_TEXT_CHARS}."
            )));
        }
        if html_body.chars().count() > HARD_MAX_HTML_CHARS {
            return Err(AppError::ValidationError(format!(
                "html_body exceeds hard max of {HARD_MAX_HTML_CHARS}."
            )));
        }
        if text_body.chars().count() > self.config.policy.max_text_chars {
            return Err(AppError::PolicyViolation(format!(
                "text_body exceeds policy max of {}.",
                self.config.policy.max_text_chars
            )));
        }
        if html_body.chars().count() > self.config.policy.max_html_chars {
            return Err(AppError::PolicyViolation(format!(
                "html_body exceeds policy max of {}.",
                self.config.policy.max_html_chars
            )));
        }

        let to = input.to.into_vec();
        let cc = input.cc.map(StringOrList::into_vec).unwrap_or_default();
        let bcc = input.bcc.map(StringOrList::into_vec).unwrap_or_default();

        let recipients = normalize_recipients(to, cc, bcc)?;
        if recipients.total() > HARD_MAX_RECIPIENTS {
            return Err(AppError::ValidationError(format!(
                "Recipients exceed hard max of {HARD_MAX_RECIPIENTS}."
            )));
        }
        enforce_recipient_policy(&self.config.policy, &recipients)?;

        let attachments = input.attachments.unwrap_or_default();
        if attachments.len() > HARD_MAX_ATTACHMENTS {
            return Err(AppError::AttachmentError(format!(
                "Too many attachments (hard max {HARD_MAX_ATTACHMENTS})."
            )));
        }
        if attachments.len() > self.config.policy.max_attachments {
            return Err(AppError::PolicyViolation(format!(
                "Too many attachments (policy max {}).",
                self.config.policy.max_attachments
            )));
        }

        let mut decoded_attachments = Vec::with_capacity(attachments.len());
        let mut attachment_bytes = 0usize;
        for attachment in attachments {
            if !is_safe_filename(&attachment.filename) {
                return Err(AppError::AttachmentError(format!(
                    "Invalid attachment filename: {}",
                    attachment.filename
                )));
            }

            let bytes = decode_base64_strict(&attachment.content_base64)?;
            let byte_len = bytes.len();
            if byte_len > HARD_MAX_ATTACHMENT_BYTES {
                return Err(AppError::AttachmentError(format!(
                    "Attachment exceeds hard max of {HARD_MAX_ATTACHMENT_BYTES} bytes."
                )));
            }
            if byte_len > self.config.policy.max_attachment_bytes {
                return Err(AppError::PolicyViolation(format!(
                    "Attachment exceeds policy max of {} bytes.",
                    self.config.policy.max_attachment_bytes
                )));
            }

            let content_type = match attachment.content_type.as_deref() {
                Some(value) => ContentType::parse(value).map_err(|_| {
                    AppError::AttachmentError(format!(
                        "Invalid attachment content_type for {}.",
                        attachment.filename
                    ))
                })?,
                None => ContentType::TEXT_PLAIN,
            };

            attachment_bytes =
                attachment_bytes.saturating_add(estimate_base64_transport_bytes(byte_len));
            decoded_attachments.push(PreparedAttachment {
                filename: attachment.filename,
                bytes,
                content_type,
            });
        }

        let estimated = estimate_message_bytes(MessageSizeParts {
            subject_bytes: input.subject.len(),
            text_bytes: text_body.len(),
            html_bytes: html_body.len(),
            attachment_bytes,
            attachment_count: decoded_attachments.len(),
        });
        if estimated > self.config.policy.max_message_bytes {
            return Err(AppError::PolicyViolation(format!(
                "Message size exceeds policy max of {} bytes.",
                self.config.policy.max_message_bytes
            )));
        }

        let envelope = Envelope {
            from: from.clone(),
            to: recipients.to.clone(),
            cc: (!recipients.cc.is_empty()).then_some(recipients.cc.clone()),
            bcc: (!recipients.bcc.is_empty()).then_some(recipients.bcc.clone()),
        };

        send_with_smtp(
            account,
            &from,
            input.reply_to.as_deref(),
            &input.subject,
            &text_body,
            &html_body,
            &recipients,
            decoded_attachments,
            self.config.policy.connect_timeout_ms,
            self.config.policy.socket_timeout_ms,
        )?;

        let accepted = recipients.all().map(ToOwned::to_owned).collect::<Vec<_>>();
        let summary = format!("Sent message to {} recipient(s).", recipients.total());
        Ok((
            summary,
            SendMessageData {
                account_id: account.account_id.clone(),
                envelope,
                message_id: None,
                accepted: Some(accepted),
                rejected: Some(Vec::new()),
            },
        ))
    }

    fn find_account(&self, account_id: &str) -> Result<&AccountConfig, AppError> {
        let needle = account_id.to_ascii_lowercase();
        self.config
            .accounts
            .iter()
            .find(|account| account.account_id == needle)
            .ok_or_else(|| AppError::ConfigMissing(format!("Account not configured: {needle}")))
    }

    pub fn invoke_list_accounts_for_test(&self, input: Value) -> Result<Value, AppError> {
        let parsed: ListAccountsInput = serde_json::from_value(input)
            .map_err(|error| AppError::ValidationError(format!("Invalid input: {error}")))?;
        let (summary, data) = self.list_accounts_impl(parsed)?;
        serde_json::to_value(ToolEnvelope {
            summary,
            data,
            meta: Meta::now(0),
        })
        .map_err(|error| AppError::UnknownError(format!("serialize error: {error}")))
    }

    pub async fn invoke_send_message_for_test(&self, input: Value) -> Result<Value, AppError> {
        let parsed: SendMessageInput = serde_json::from_value(input)
            .map_err(|error| AppError::ValidationError(format!("Invalid input: {error}")))?;
        let (summary, data) = self.send_message_impl(parsed).await?;
        serde_json::to_value(ToolEnvelope {
            summary,
            data,
            meta: Meta::now(0),
        })
        .map_err(|error| AppError::UnknownError(format!("serialize error: {error}")))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "SMTP MCP server with two tools: smtp_list_accounts and smtp_send_message. Sending requires MAIL_SMTP_SEND_ENABLED=true."
                    .to_owned(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

fn finalize_tool<T>(
    started: Instant,
    result: Result<(String, T), AppError>,
) -> Result<Json<ToolEnvelope<T>>, ErrorData>
where
    T: JsonSchema,
{
    match result {
        Ok((summary, data)) => Ok(Json(ToolEnvelope {
            summary,
            data,
            meta: Meta::now(duration_ms(started)),
        })),
        Err(error) => Err(error.to_error_data()),
    }
}

fn duration_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

fn resolve_sender(input: &SendMessageInput, account: &AccountConfig) -> Result<String, AppError> {
    let from = input
        .from
        .as_deref()
        .or(account.default_from.as_deref())
        .ok_or_else(|| AppError::ValidationError("Sender address is required.".to_owned()))?
        .trim()
        .to_ascii_lowercase();

    validate_email_address(&from)?;
    if let Some(reply_to) = &input.reply_to {
        validate_email_address(reply_to)?;
    }

    Ok(from)
}

#[allow(clippy::too_many_arguments)]
fn send_with_smtp(
    account: &AccountConfig,
    from: &str,
    reply_to: Option<&str>,
    subject: &str,
    text_body: &str,
    html_body: &str,
    recipients: &crate::policy::Recipients,
    attachments: Vec<PreparedAttachment>,
    connect_timeout_ms: u64,
    socket_timeout_ms: u64,
) -> Result<(), AppError> {
    let mut builder = Message::builder()
        .from(parse_mailbox(from)?)
        .subject(subject.to_owned());

    for address in &recipients.to {
        builder = builder.to(parse_mailbox(address)?);
    }
    for address in &recipients.cc {
        builder = builder.cc(parse_mailbox(address)?);
    }
    for address in &recipients.bcc {
        builder = builder.bcc(parse_mailbox(address)?);
    }
    if let Some(reply_to) = reply_to {
        builder = builder.reply_to(parse_mailbox(reply_to)?);
    }

    let email = if attachments.is_empty() {
        if !text_body.is_empty() && !html_body.is_empty() {
            builder
                .multipart(MultiPart::alternative_plain_html(
                    text_body.to_owned(),
                    html_body.to_owned(),
                ))
                .map_err(|_| {
                    AppError::ValidationError("Unable to build message body.".to_owned())
                })?
        } else if !html_body.is_empty() {
            builder
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_HTML)
                        .body(html_body.to_owned()),
                )
                .map_err(|_| {
                    AppError::ValidationError("Unable to build message body.".to_owned())
                })?
        } else {
            builder.body(text_body.to_owned()).map_err(|_| {
                AppError::ValidationError("Unable to build message body.".to_owned())
            })?
        }
    } else {
        let body_part = if !text_body.is_empty() && !html_body.is_empty() {
            MultiPart::alternative_plain_html(text_body.to_owned(), html_body.to_owned())
        } else if !html_body.is_empty() {
            MultiPart::mixed().singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_HTML)
                    .body(html_body.to_owned()),
            )
        } else {
            MultiPart::mixed().singlepart(SinglePart::plain(text_body.to_owned()))
        };

        let mut mixed = MultiPart::mixed().multipart(body_part);
        for attachment in attachments {
            mixed = mixed.singlepart(
                Attachment::new(attachment.filename)
                    .body(attachment.bytes, attachment.content_type),
            );
        }

        builder
            .multipart(mixed)
            .map_err(|_| AppError::ValidationError("Unable to build message body.".to_owned()))?
    };

    let credentials = Credentials::new(
        account.user.clone(),
        account.pass.expose_secret().to_owned(),
    );
    let timeout = Some(std::time::Duration::from_millis(
        connect_timeout_ms.max(socket_timeout_ms),
    ));
    let transport = if account.secure {
        SmtpTransport::relay(&account.host)
            .map_err(|_| AppError::SmtpError("SMTP transport configuration failed.".to_owned()))?
            .credentials(credentials)
            .authentication(vec![Mechanism::Login])
            .port(account.port)
            .timeout(timeout)
            .build()
    } else {
        SmtpTransport::builder_dangerous(&account.host)
            .credentials(credentials)
            .authentication(vec![Mechanism::Login])
            .port(account.port)
            .timeout(timeout)
            .build()
    };

    let verified = transport
        .test_connection()
        .map_err(|error| AppError::SmtpError(format!("SMTP connection test failed: {error}")))?;
    if !verified {
        return Err(AppError::SmtpError(
            "SMTP connection test failed: server did not validate connection".to_owned(),
        ));
    }

    transport
        .send(&email)
        .map_err(|error| AppError::SmtpError(format!("SMTP send failed: {error}")))?;

    Ok(())
}

fn parse_mailbox(value: &str) -> Result<Mailbox, AppError> {
    value
        .parse::<Mailbox>()
        .map_err(|_| AppError::ValidationError(format!("Invalid email address: {value}")))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{ListAccountsInput, McpServer, TOOL_NAMES};
    use crate::config::{PolicyConfig, ServerConfig};
    use crate::errors::ErrorCode;

    fn empty_server() -> McpServer {
        McpServer::new(ServerConfig {
            accounts: Vec::new(),
            policy: PolicyConfig {
                send_enabled: false,
                allowlist_domains: HashSet::new(),
                allowlist_addresses: HashSet::new(),
                max_recipients: 10,
                max_message_bytes: 2_500_000,
                max_attachments: 5,
                max_attachment_bytes: 2_000_000,
                max_text_chars: 20_000,
                max_html_chars: 50_000,
                connect_timeout_ms: 10_000,
                socket_timeout_ms: 20_000,
            },
        })
    }

    #[test]
    fn tool_names_match_contract() {
        assert_eq!(TOOL_NAMES, ["smtp_list_accounts", "smtp_send_message"]);
    }

    #[test]
    fn list_accounts_missing_filter_returns_config_missing() {
        let server = empty_server();
        let err = server
            .list_accounts_impl(ListAccountsInput {
                account_id: Some("default".to_owned()),
            })
            .expect_err("must fail");
        assert_eq!(err.code(), ErrorCode::ConfigMissing);
    }
}
