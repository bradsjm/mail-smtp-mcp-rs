use std::collections::BTreeMap;
use std::io::{self, Write};

use mail_smtp_mcp_rs::config::{
    DEFAULT_CONNECT_TIMEOUT_MS, DEFAULT_MAX_ATTACHMENT_BYTES, DEFAULT_MAX_ATTACHMENTS,
    DEFAULT_MAX_HTML_CHARS, DEFAULT_MAX_MESSAGE_BYTES, DEFAULT_MAX_RECIPIENTS,
    DEFAULT_MAX_TEXT_CHARS, DEFAULT_SOCKET_TIMEOUT_MS,
};

/// Entry point for the mail-smtp-mcp-rs application.
///
/// Loads environment variables, checks for help flags, and starts the MCP server.
#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    if should_print_help(std::env::args().skip(1)) {
        if let Err(error) = print_help_output() {
            eprintln!("mail-smtp-mcp-rs help output error: {error}");
            std::process::exit(1);
        }
        return;
    }

    if let Err(error) = mail_smtp_mcp_rs::startup::run().await {
        eprintln!("mail-smtp-mcp-rs startup error: {error}");
        std::process::exit(1);
    }
}

/// Returns `true` if the provided arguments request help output (`--help` or `-h`).
fn should_print_help<I>(args: I) -> bool
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    args.into_iter().any(|arg| {
        let arg = arg.as_ref();
        arg == "--help" || arg == "-h"
    })
}

/// Prints the help output to stdout, describing usage and environment variables.
fn print_help_output() -> io::Result<()> {
    let env_map: BTreeMap<String, String> = std::env::vars().collect();
    let output = build_help_output(&env_map);
    let mut stdout = io::stdout().lock();
    stdout.write_all(output.as_bytes())?;
    stdout.flush()
}

/// Builds the help output string, including discovered account sections and policy defaults.
fn build_help_output(env_map: &BTreeMap<String, String>) -> String {
    let account_sections = discover_account_sections(env_map);
    let mut out = String::new();

    out.push_str("mail-smtp-mcp-rs\n");
    out.push_str("Secure SMTP MCP server over stdio\n\n");
    out.push_str("Usage:\n");
    out.push_str("  mail-smtp-mcp-rs\n");
    out.push_str("  mail-smtp-mcp-rs --help\n\n");

    out.push_str("SMTP environment setup\n");
    out.push_str("  Required per account section MAIL_SMTP_<ACCOUNT>_:\n");
    out.push_str("    MAIL_SMTP_<ACCOUNT>_HOST\n");
    out.push_str("    MAIL_SMTP_<ACCOUNT>_USER\n");
    out.push_str("    MAIL_SMTP_<ACCOUNT>_PASS\n");
    out.push_str("  Optional per account section:\n");
    out.push_str("    MAIL_SMTP_<ACCOUNT>_PORT (default: 587 or 465 when secure=true)\n");
    out.push_str("    MAIL_SMTP_<ACCOUNT>_SECURE (default: false)\n");
    out.push_str("    MAIL_SMTP_<ACCOUNT>_FROM\n\n");

    out.push_str("Discovered account sections (from current environment)\n");
    if account_sections.is_empty() {
        out.push_str("  (none discovered)\n");
    } else {
        for section in &account_sections {
            out.push_str(&format!("  [{}]\n", section));
            for suffix in ["HOST", "USER", "PASS", "PORT", "SECURE", "FROM"] {
                let key = format!("MAIL_SMTP_{}_{}", section, suffix);
                let value = env_map.get(&key).map(String::as_str);
                out.push_str(&format!("    {}={}\n", key, redact_value(&key, value)));
            }
        }
    }
    out.push('\n');

    out.push_str("Global policy defaults\n");
    out.push_str("  MAIL_SMTP_SEND_ENABLED=false\n");
    out.push_str(&format!(
        "  MAIL_SMTP_MAX_RECIPIENTS={}\n",
        DEFAULT_MAX_RECIPIENTS
    ));
    out.push_str(&format!(
        "  MAIL_SMTP_MAX_MESSAGE_BYTES={}\n",
        DEFAULT_MAX_MESSAGE_BYTES
    ));
    out.push_str(&format!(
        "  MAIL_SMTP_MAX_ATTACHMENTS={}\n",
        DEFAULT_MAX_ATTACHMENTS
    ));
    out.push_str(&format!(
        "  MAIL_SMTP_MAX_ATTACHMENT_BYTES={}\n",
        DEFAULT_MAX_ATTACHMENT_BYTES
    ));
    out.push_str(&format!(
        "  MAIL_SMTP_MAX_TEXT_CHARS={}\n",
        DEFAULT_MAX_TEXT_CHARS
    ));
    out.push_str(&format!(
        "  MAIL_SMTP_MAX_HTML_CHARS={}\n",
        DEFAULT_MAX_HTML_CHARS
    ));
    out.push_str(&format!(
        "  MAIL_SMTP_CONNECT_TIMEOUT_MS={}\n",
        DEFAULT_CONNECT_TIMEOUT_MS
    ));
    out.push_str(&format!(
        "  MAIL_SMTP_SOCKET_TIMEOUT_MS={}\n\n",
        DEFAULT_SOCKET_TIMEOUT_MS
    ));

    out.push_str("Send gate policy\n");
    out.push_str("  Sending is disabled by default.\n");
    out.push_str("  Enable live sends only with MAIL_SMTP_SEND_ENABLED=true.\n");

    out
}

/// Discovers all unique account sections from the environment variable map.
fn discover_account_sections(env_map: &BTreeMap<String, String>) -> Vec<String> {
    let mut sections: Vec<String> = env_map
        .keys()
        .filter_map(|key| {
            let remainder = key.strip_prefix("MAIL_SMTP_")?;
            for suffix in ["_HOST", "_USER", "_PASS", "_PORT", "_SECURE", "_FROM"] {
                if let Some(section) = remainder.strip_suffix(suffix)
                    && !section.is_empty()
                {
                    return Some(section.to_owned());
                }
            }
            None
        })
        .collect();

    sections.sort();
    sections.dedup();
    sections
}

/// Redacts secret values (such as passwords) for display in help output.
fn redact_value(key: &str, value: Option<&str>) -> String {
    match value {
        Some(v) if is_secret_key(key) && !v.is_empty() => "<redacted>".to_owned(),
        Some("") => "<empty>".to_owned(),
        Some(v) => v.to_owned(),
        None => "<unset>".to_owned(),
    }
}

/// Returns `true` if the key is considered secret (e.g., contains "PASS").
fn is_secret_key(key: &str) -> bool {
    key.to_ascii_uppercase().contains("PASS")
}
