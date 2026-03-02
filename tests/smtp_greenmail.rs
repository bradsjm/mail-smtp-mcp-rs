/// Integration tests for mail-smtp-mcp-rs using a GreenMail test server.
///
/// These tests verify end-to-end SMTP and IMAP interactions, including message delivery,
/// attachment handling, policy enforcement, and send gating.
use std::collections::HashMap;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use async_imap::Client;
use futures_util::TryStreamExt;
use mail_smtp_mcp_rs::config::load_server_config;
use mail_smtp_mcp_rs::errors::ErrorCode;
use mail_smtp_mcp_rs::server::McpServer;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout};

/// Generates a unique string based on the current timestamp for test isolation.
fn nonce() -> String {
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0);
    micros.to_string()
}

/// Test harness for interacting with a GreenMail SMTP/IMAP server.
struct GreenmailHarness {
    smtp_host: String,
    smtp_port: u16,
    imap_host: String,
    imap_port: u16,
    smtp_user: String,
    smtp_pass: String,
    imap_user: String,
    imap_pass: String,
}

impl GreenmailHarness {
    /// Starts the GreenMail test harness, waiting until both SMTP and IMAP are ready.
    async fn start() -> Result<Self, String> {
        let smtp_host = std::env::var("GREENMAIL_HOST").unwrap_or_else(|_| "localhost".to_owned());
        let imap_host = smtp_host.clone();
        let smtp_port = std::env::var("GREENMAIL_SMTP_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(3025);
        let imap_port = std::env::var("GREENMAIL_IMAP_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(3143);

        let harness = Self {
            smtp_host,
            smtp_port,
            imap_host,
            imap_port,
            smtp_user: "sender".to_owned(),
            smtp_pass: "secret".to_owned(),
            imap_user: "recipient".to_owned(),
            imap_pass: "secret".to_owned(),
        };

        harness.wait_until_ready().await?;
        Ok(harness)
    }

    /// Waits until both SMTP and IMAP services are ready to accept connections.
    async fn wait_until_ready(&self) -> Result<(), String> {
        let mut last_error = String::new();
        for _ in 0..60 {
            let smtp = self.smtp_probe().await;
            let imap = self.imap_probe().await;

            if smtp.is_ok() && imap.is_ok() {
                return Ok(());
            }

            last_error = format!("smtp={smtp:?} imap={imap:?}");
            sleep(Duration::from_secs(1)).await;
        }

        Err(format!(
            "GreenMail did not become ready in time: {last_error}"
        ))
    }

    /// Attempts to connect and authenticate to the IMAP server to verify readiness.
    async fn imap_probe(&self) -> Result<(), String> {
        let tcp = timeout(
            Duration::from_secs(2),
            TcpStream::connect((self.imap_host.as_str(), self.imap_port)),
        )
        .await
        .map_err(|_| "imap probe tcp connect timeout".to_owned())
        .and_then(|r| r.map_err(|e| format!("imap probe tcp connect failed: {e}")))?;

        let mut client = Client::new(tcp);
        let greeting = timeout(Duration::from_secs(2), client.read_response())
            .await
            .map_err(|_| "imap probe greeting timeout".to_owned())
            .and_then(|r| r.map_err(|e| format!("imap probe greeting failed: {e}")))?;
        if greeting.is_none() {
            return Err("imap probe server closed before greeting".to_owned());
        }

        let mut session = timeout(
            Duration::from_secs(2),
            client.login(&self.imap_user, &self.imap_pass),
        )
        .await
        .map_err(|_| "imap probe login timeout".to_owned())?
        .map_err(|(e, _)| format!("imap probe login failed: {e}"))?;

        timeout(Duration::from_secs(2), session.select("INBOX"))
            .await
            .map_err(|_| "imap probe select timeout".to_owned())
            .and_then(|r| r.map_err(|e| format!("imap probe select failed: {e}")))?;

        let _ = session.logout().await;
        Ok(())
    }

    /// Attempts to connect and send an EHLO to the SMTP server to verify readiness.
    async fn smtp_probe(&self) -> Result<(), String> {
        let mut stream = timeout(
            Duration::from_secs(2),
            TcpStream::connect((self.smtp_host.as_str(), self.smtp_port)),
        )
        .await
        .map_err(|_| "smtp probe connect timeout".to_owned())
        .and_then(|r| r.map_err(|e| format!("smtp probe connect failed: {e}")))?;

        let mut banner = [0_u8; 512];
        let banner_len = timeout(Duration::from_secs(2), stream.read(&mut banner))
            .await
            .map_err(|_| "smtp probe banner timeout".to_owned())
            .and_then(|r| r.map_err(|e| format!("smtp probe banner read failed: {e}")))?;
        let banner_text = String::from_utf8_lossy(&banner[..banner_len]);
        if !banner_text.starts_with("220") {
            return Err(format!("smtp probe banner unexpected: {banner_text}"));
        }

        timeout(
            Duration::from_secs(2),
            stream.write_all(b"EHLO localhost\r\n"),
        )
        .await
        .map_err(|_| "smtp probe EHLO write timeout".to_owned())
        .and_then(|r| r.map_err(|e| format!("smtp probe EHLO write failed: {e}")))?;

        let mut ehlo = [0_u8; 1024];
        let ehlo_len = timeout(Duration::from_secs(2), stream.read(&mut ehlo))
            .await
            .map_err(|_| "smtp probe EHLO read timeout".to_owned())
            .and_then(|r| r.map_err(|e| format!("smtp probe EHLO read failed: {e}")))?;
        let ehlo_text = String::from_utf8_lossy(&ehlo[..ehlo_len]);
        if !ehlo_text.contains("250") {
            return Err(format!("smtp probe EHLO unexpected: {ehlo_text}"));
        }

        Ok(())
    }

    /// Returns a HashMap of environment variables for configuring the test server.
    fn base_env(&self, send_enabled: bool) -> HashMap<String, String> {
        HashMap::from([
            ("MAIL_SMTP_DEFAULT_HOST".to_owned(), self.smtp_host.clone()),
            (
                "MAIL_SMTP_DEFAULT_PORT".to_owned(),
                self.smtp_port.to_string(),
            ),
            ("MAIL_SMTP_DEFAULT_SECURE".to_owned(), "false".to_owned()),
            ("MAIL_SMTP_DEFAULT_USER".to_owned(), self.smtp_user.clone()),
            ("MAIL_SMTP_DEFAULT_PASS".to_owned(), self.smtp_pass.clone()),
            (
                "MAIL_SMTP_DEFAULT_FROM".to_owned(),
                "sender@example.com".to_owned(),
            ),
            (
                "MAIL_SMTP_SEND_ENABLED".to_owned(),
                if send_enabled { "true" } else { "false" }.to_owned(),
            ),
        ])
    }

    /// Fetches all messages from the INBOX of the test IMAP account.
    async fn inbox_messages(&self) -> Result<Vec<String>, String> {
        let tcp = timeout(
            Duration::from_secs(3),
            TcpStream::connect((self.imap_host.as_str(), self.imap_port)),
        )
        .await
        .map_err(|_| "imap tcp connect timeout".to_owned())
        .and_then(|r| r.map_err(|e| format!("imap tcp connect failed: {e}")))?;

        let mut client = Client::new(tcp);
        let greeting = timeout(Duration::from_secs(3), client.read_response())
            .await
            .map_err(|_| "imap greeting timeout".to_owned())
            .and_then(|r| r.map_err(|e| format!("imap greeting failed: {e}")))?;
        if greeting.is_none() {
            return Err("imap server closed before greeting".to_owned());
        }

        let mut session = timeout(
            Duration::from_secs(3),
            client.login(&self.imap_user, &self.imap_pass),
        )
        .await
        .map_err(|_| "imap login timeout".to_owned())?
        .map_err(|(e, _)| format!("imap login failed: {e}"))?;

        timeout(Duration::from_secs(3), session.select("INBOX"))
            .await
            .map_err(|_| "imap select timeout".to_owned())
            .and_then(|r| r.map_err(|e| format!("imap select failed: {e}")))?;

        let mut uids: Vec<u32> = timeout(Duration::from_secs(3), session.uid_search("ALL"))
            .await
            .map_err(|_| "imap uid search timeout".to_owned())
            .and_then(|r| r.map_err(|e| format!("imap uid search failed: {e}")))?
            .into_iter()
            .collect();
        uids.sort_unstable();

        let mut raw_messages = Vec::new();
        for uid in uids {
            let stream = timeout(
                Duration::from_secs(3),
                session.uid_fetch(uid.to_string(), "RFC822"),
            )
            .await
            .map_err(|_| "imap uid fetch timeout".to_owned())
            .and_then(|r| r.map_err(|e| format!("imap uid fetch failed: {e}")))?;

            let fetches = timeout(Duration::from_secs(3), stream.try_collect::<Vec<_>>())
                .await
                .map_err(|_| "imap uid fetch stream timeout".to_owned())
                .and_then(|r| r.map_err(|e| format!("imap uid fetch stream failed: {e}")))?;

            for fetch in fetches {
                if let Some(body) = fetch.body() {
                    raw_messages.push(String::from_utf8_lossy(body).to_string());
                }
            }
        }

        let _ = session.logout().await;
        Ok(raw_messages)
    }

    /// Returns the number of messages currently in the INBOX.
    async fn inbox_count(&self) -> Result<usize, String> {
        Ok(self.inbox_messages().await?.len())
    }

    /// Asserts that the inbox message count remains stable over a number of attempts.
    async fn assert_inbox_count_stable(
        &self,
        baseline: usize,
        attempts: usize,
        delay: Duration,
    ) -> Result<(), String> {
        for _ in 0..attempts {
            let count = self.inbox_count().await?;
            if count != baseline {
                return Err(format!(
                    "expected inbox count to remain {baseline}, got {count}"
                ));
            }
            sleep(delay).await;
        }

        Ok(())
    }
}

/// Creates an MCP server instance from the provided environment configuration.
fn server_from_env(env: HashMap<String, String>) -> McpServer {
    let config = load_server_config(&env);
    McpServer::new(config)
}

/// Integration test: verifies that a message is successfully delivered to the recipient's INBOX.
#[tokio::test]
#[serial_test::serial]
#[ignore = "requires GreenMail"]
async fn send_message_success_delivers_mail() {
    let harness = GreenmailHarness::start()
        .await
        .expect("greenmail must start");
    let server = server_from_env(harness.base_env(true));
    let subject = format!("integration-send-success-{}", nonce());
    let text_body = "hello integration";

    let response = server
        .invoke_send_message_for_test(json!({
            "account_id": "default",
            "to": ["recipient@example.com"],
            "subject": subject,
            "text_body": text_body
        }))
        .await
        .expect("send must succeed");

    assert_eq!(response["data"]["account_id"], "default");
    assert!(
        response["data"]["accepted"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
    );

    let mut delivered = false;
    for _ in 0..20 {
        let messages = harness.inbox_messages().await.expect("imap read must work");
        if messages
            .iter()
            .any(|raw| raw.contains(&format!("Subject: {subject}")) && raw.contains(text_body))
        {
            delivered = true;
            break;
        }
        sleep(Duration::from_millis(300)).await;
    }

    assert!(delivered, "expected delivered mail in INBOX");
}

/// Integration test: verifies that a message with an attachment is delivered and the MIME part is present.
#[tokio::test]
#[serial_test::serial]
#[ignore = "requires GreenMail"]
async fn send_message_with_attachment_delivers_mime_part() {
    let harness = GreenmailHarness::start()
        .await
        .expect("greenmail must start");
    let server = server_from_env(harness.base_env(true));
    let subject = format!("integration-attachment-{}", nonce());

    server
        .invoke_send_message_for_test(json!({
            "account_id": "default",
            "to": ["recipient@example.com"],
            "subject": subject,
            "text_body": "see attachment",
            "attachments": [
                {
                    "filename": "report.txt",
                    "content_base64": "aGVsbG8gYXR0YWNobWVudA==",
                    "content_type": "text/plain"
                }
            ]
        }))
        .await
        .expect("send with attachment must succeed");

    let mut delivered = false;
    for _ in 0..20 {
        let messages = harness.inbox_messages().await.expect("imap read must work");
        if messages.iter().any(|raw| {
            let raw_lower = raw.to_ascii_lowercase();
            let has_filename = raw_lower.contains("filename=report.txt")
                || raw_lower.contains("filename=\"report.txt\"")
                || raw_lower.contains("filename*=utf-8''report.txt");

            raw.contains(&format!("Subject: {subject}"))
                && has_filename
                && raw_lower.contains("content-type: text/plain")
        }) {
            delivered = true;
            break;
        }
        sleep(Duration::from_millis(300)).await;
    }

    assert!(delivered, "expected attachment mail in INBOX");
}

/// Integration test: verifies that sending is blocked when MAIL_SMTP_SEND_ENABLED is false.
#[tokio::test]
#[serial_test::serial]
#[ignore = "requires GreenMail"]
async fn send_disabled_blocks_live_send() {
    let harness = GreenmailHarness::start()
        .await
        .expect("greenmail must start");
    let baseline_count = harness.inbox_count().await.expect("imap read must work");
    let server = server_from_env(harness.base_env(false));

    let err = server
        .invoke_send_message_for_test(json!({
            "account_id": "default",
            "to": ["recipient@example.com"],
            "subject": "integration-send-disabled",
            "text_body": "should not send"
        }))
        .await
        .expect_err("send must fail when disabled");

    assert_eq!(err.code(), ErrorCode::SendDisabled);
    harness
        .assert_inbox_count_stable(baseline_count, 8, Duration::from_millis(250))
        .await
        .expect("send-disabled must not deliver mail");
}

/// Integration test: verifies that allowlist policy blocks recipients before SMTP delivery.
#[tokio::test]
#[serial_test::serial]
#[ignore = "requires GreenMail"]
async fn allowlist_blocks_before_smtp() {
    let harness = GreenmailHarness::start()
        .await
        .expect("greenmail must start");
    let baseline_count = harness.inbox_count().await.expect("imap read must work");
    let mut env = harness.base_env(true);
    env.insert(
        "MAIL_SMTP_ALLOWLIST_DOMAINS".to_owned(),
        "allowed.example".to_owned(),
    );

    let server = server_from_env(env);
    let err = server
        .invoke_send_message_for_test(json!({
            "account_id": "default",
            "to": ["blocked@example.com"],
            "subject": "integration-allowlist-block",
            "text_body": "must block"
        }))
        .await
        .expect_err("send must fail when allowlist blocks");

    assert_eq!(err.code(), ErrorCode::PolicyViolation);
    harness
        .assert_inbox_count_stable(baseline_count, 8, Duration::from_millis(250))
        .await
        .expect("allowlist rejection must not deliver mail");
}
