use std::collections::{BTreeSet, HashMap, HashSet};

use secrecy::SecretString;

pub const DEFAULT_MAX_RECIPIENTS: usize = 10;
pub const DEFAULT_MAX_MESSAGE_BYTES: usize = 2_500_000;
pub const DEFAULT_MAX_ATTACHMENTS: usize = 5;
pub const DEFAULT_MAX_ATTACHMENT_BYTES: usize = 2_000_000;
pub const DEFAULT_MAX_TEXT_CHARS: usize = 20_000;
pub const DEFAULT_MAX_HTML_CHARS: usize = 50_000;
pub const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 10_000;
pub const DEFAULT_SOCKET_TIMEOUT_MS: u64 = 20_000;

/// Configuration for an SMTP account.
#[derive(Debug, Clone)]
pub struct AccountConfig {
    /// The unique identifier for the account.
    pub account_id: String,
    /// SMTP server hostname.
    pub host: String,
    /// SMTP server port.
    pub port: u16,
    /// Whether to use a secure connection (SMTPS).
    pub secure: bool,
    /// Username for SMTP authentication.
    pub user: String,
    /// Password for SMTP authentication.
    pub pass: SecretString,
    /// Optional default "from" address for this account.
    pub default_from: Option<String>,
}

/// Metadata for an SMTP account, omitting sensitive fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountMetadata {
    /// The unique identifier for the account.
    pub account_id: String,
    /// SMTP server hostname.
    pub host: String,
    /// SMTP server port.
    pub port: u16,
    /// Whether the account uses a secure connection.
    pub secure: bool,
    /// Optional default "from" address.
    pub default_from: Option<String>,
}

/// Policy configuration for sending emails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyConfig {
    /// Whether sending is enabled.
    pub send_enabled: bool,
    /// Allowed recipient domains.
    pub allowlist_domains: HashSet<String>,
    /// Allowed recipient email addresses.
    pub allowlist_addresses: HashSet<String>,
    /// Maximum number of recipients per message.
    pub max_recipients: usize,
    /// Maximum allowed message size in bytes.
    pub max_message_bytes: usize,
    /// Maximum number of attachments per message.
    pub max_attachments: usize,
    /// Maximum allowed size per attachment in bytes.
    pub max_attachment_bytes: usize,
    /// Maximum allowed characters in the text body.
    pub max_text_chars: usize,
    /// Maximum allowed characters in the HTML body.
    pub max_html_chars: usize,
    /// Connection timeout in milliseconds.
    pub connect_timeout_ms: u64,
    /// Socket timeout in milliseconds.
    pub socket_timeout_ms: u64,
}

/// Top-level server configuration, including accounts and policy.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Configured SMTP accounts.
    pub accounts: Vec<AccountConfig>,
    /// Policy configuration.
    pub policy: PolicyConfig,
}

/// Lists all discovered SMTP account IDs from the environment variables.
pub fn list_account_ids(env: &HashMap<String, String>) -> Vec<String> {
    let mut ids = BTreeSet::new();

    for key in env.keys() {
        let Some(rest) = key.strip_prefix("MAIL_SMTP_") else {
            continue;
        };
        let Some((candidate, suffix)) = rest.rsplit_once('_') else {
            continue;
        };
        if !matches!(
            suffix,
            "HOST" | "USER" | "PASS" | "PORT" | "SECURE" | "FROM"
        ) {
            continue;
        }
        if candidate.is_empty() || !candidate.chars().all(|ch| ch.is_ascii_alphanumeric()) {
            continue;
        }

        ids.insert(candidate.to_ascii_lowercase());
    }

    ids.into_iter().collect()
}

/// Returns the required environment variable keys for a given account ID.
pub fn required_account_keys(account_id: &str) -> [String; 3] {
    let normalized = account_id.to_ascii_uppercase();
    [
        format!("MAIL_SMTP_{normalized}_HOST"),
        format!("MAIL_SMTP_{normalized}_USER"),
        format!("MAIL_SMTP_{normalized}_PASS"),
    ]
}

/// Returns a list of missing required environment variable keys for the given account ID.
pub fn missing_required_account_env(
    env: &HashMap<String, String>,
    account_id: &str,
) -> Vec<String> {
    let mut missing = Vec::new();
    for key in required_account_keys(account_id) {
        if read_env_string(env, &key).is_none() {
            missing.push(key);
        }
    }
    missing
}

/// Resolves the configuration for a specific SMTP account from environment variables.
/// Returns an error with missing keys if required variables are not set.
pub fn resolve_account_config(
    env: &HashMap<String, String>,
    account_id: &str,
) -> Result<AccountConfig, Vec<String>> {
    let normalized = account_id.to_ascii_lowercase();
    let prefix = format!("MAIL_SMTP_{}", normalized.to_ascii_uppercase());
    let missing = missing_required_account_env(env, &normalized);

    if !missing.is_empty() {
        return Err(missing);
    }

    let secure = read_env_bool(env, &format!("{prefix}_SECURE"), false);
    let default_port = if secure { 465_u16 } else { 587_u16 };
    let port = read_env_u16(env, &format!("{prefix}_PORT"), default_port);
    let host = read_env_string(env, &format!("{prefix}_HOST"));
    let user = read_env_string(env, &format!("{prefix}_USER"));
    let pass = read_env_string(env, &format!("{prefix}_PASS"));

    let (Some(host), Some(user), Some(pass)) = (host, user, pass) else {
        return Err(missing_required_account_env(env, &normalized));
    };

    Ok(AccountConfig {
        account_id: normalized,
        host,
        port,
        secure,
        user,
        pass: SecretString::new(pass.into()),
        default_from: read_env_string(env, &format!("{prefix}_FROM")),
    })
}

/// Returns a list of metadata for all configured accounts.
pub fn list_account_metadata(accounts: &[AccountConfig]) -> Vec<AccountMetadata> {
    accounts
        .iter()
        .map(|account| AccountMetadata {
            account_id: account.account_id.clone(),
            host: account.host.clone(),
            port: account.port,
            secure: account.secure,
            default_from: account.default_from.clone(),
        })
        .collect()
}

/// Loads the policy configuration from environment variables, applying defaults as needed.
pub fn load_policy_config(env: &HashMap<String, String>) -> PolicyConfig {
    PolicyConfig {
        send_enabled: read_env_bool(env, "MAIL_SMTP_SEND_ENABLED", false),
        allowlist_domains: read_env_csv_set(env, "MAIL_SMTP_ALLOWLIST_DOMAINS"),
        allowlist_addresses: read_env_csv_set(env, "MAIL_SMTP_ALLOWLIST_ADDRESSES"),
        max_recipients: read_env_usize(env, "MAIL_SMTP_MAX_RECIPIENTS", DEFAULT_MAX_RECIPIENTS),
        max_message_bytes: read_env_usize(
            env,
            "MAIL_SMTP_MAX_MESSAGE_BYTES",
            DEFAULT_MAX_MESSAGE_BYTES,
        ),
        max_attachments: read_env_usize(env, "MAIL_SMTP_MAX_ATTACHMENTS", DEFAULT_MAX_ATTACHMENTS),
        max_attachment_bytes: read_env_usize(
            env,
            "MAIL_SMTP_MAX_ATTACHMENT_BYTES",
            DEFAULT_MAX_ATTACHMENT_BYTES,
        ),
        max_text_chars: read_env_usize(env, "MAIL_SMTP_MAX_TEXT_CHARS", DEFAULT_MAX_TEXT_CHARS),
        max_html_chars: read_env_usize(env, "MAIL_SMTP_MAX_HTML_CHARS", DEFAULT_MAX_HTML_CHARS),
        connect_timeout_ms: read_env_u64(
            env,
            "MAIL_SMTP_CONNECT_TIMEOUT_MS",
            DEFAULT_CONNECT_TIMEOUT_MS,
        ),
        socket_timeout_ms: read_env_u64(
            env,
            "MAIL_SMTP_SOCKET_TIMEOUT_MS",
            DEFAULT_SOCKET_TIMEOUT_MS,
        ),
    }
}

/// Loads the full server configuration (accounts and policy) from environment variables.
pub fn load_server_config(env: &HashMap<String, String>) -> ServerConfig {
    let mut accounts = Vec::new();
    for account_id in list_account_ids(env) {
        if let Ok(account) = resolve_account_config(env, &account_id) {
            accounts.push(account);
        }
    }

    ServerConfig {
        accounts,
        policy: load_policy_config(env),
    }
}

fn read_env_string(env: &HashMap<String, String>, key: &str) -> Option<String> {
    env.get(key).and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

fn read_env_bool(env: &HashMap<String, String>, key: &str, fallback: bool) -> bool {
    match read_env_string(env, key) {
        Some(value) => value.eq_ignore_ascii_case("true"),
        None => fallback,
    }
}

fn read_env_u16(env: &HashMap<String, String>, key: &str, fallback: u16) -> u16 {
    match read_env_string(env, key) {
        Some(value) => value.parse::<u16>().unwrap_or(fallback),
        None => fallback,
    }
}

fn read_env_u64(env: &HashMap<String, String>, key: &str, fallback: u64) -> u64 {
    match read_env_string(env, key) {
        Some(value) => value.parse::<u64>().unwrap_or(fallback),
        None => fallback,
    }
}

fn read_env_usize(env: &HashMap<String, String>, key: &str, fallback: usize) -> usize {
    match read_env_string(env, key) {
        Some(value) => value.parse::<usize>().unwrap_or(fallback),
        None => fallback,
    }
}

fn read_env_csv_set(env: &HashMap<String, String>, key: &str) -> HashSet<String> {
    match read_env_string(env, key) {
        Some(value) => value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_ascii_lowercase)
            .collect(),
        None => HashSet::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{
        DEFAULT_CONNECT_TIMEOUT_MS, DEFAULT_MAX_ATTACHMENT_BYTES, DEFAULT_MAX_ATTACHMENTS,
        DEFAULT_MAX_HTML_CHARS, DEFAULT_MAX_MESSAGE_BYTES, DEFAULT_MAX_RECIPIENTS,
        DEFAULT_MAX_TEXT_CHARS, DEFAULT_SOCKET_TIMEOUT_MS, list_account_ids, load_policy_config,
        missing_required_account_env, resolve_account_config,
    };

    fn env_map(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn lists_normalized_sorted_account_ids() {
        let env = env_map(&[
            ("MAIL_SMTP_BETA_HOST", "smtp.beta.local"),
            ("MAIL_SMTP_ALPHA_USER", "user"),
            ("MAIL_SMTP_BETA_PASS", "pass"),
        ]);

        let ids = list_account_ids(&env);
        assert_eq!(ids, vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    fn reports_missing_required_keys() {
        let env = env_map(&[("MAIL_SMTP_DEFAULT_HOST", "smtp.example.com")]);
        let missing = missing_required_account_env(&env, "default");

        assert_eq!(
            missing,
            vec![
                "MAIL_SMTP_DEFAULT_USER".to_string(),
                "MAIL_SMTP_DEFAULT_PASS".to_string()
            ]
        );
    }

    #[test]
    fn resolves_account_defaults_and_overrides() {
        let env = env_map(&[
            ("MAIL_SMTP_DEFAULT_HOST", "smtp.example.com"),
            ("MAIL_SMTP_DEFAULT_USER", "alice"),
            ("MAIL_SMTP_DEFAULT_PASS", "secret"),
        ]);

        let account = resolve_account_config(&env, "DEFAULT").expect("must resolve account");
        assert_eq!(account.account_id, "default");
        assert_eq!(account.port, 587);
        assert!(!account.secure);

        let secure_env = env_map(&[
            ("MAIL_SMTP_ALT_HOST", "smtp.example.com"),
            ("MAIL_SMTP_ALT_USER", "bob"),
            ("MAIL_SMTP_ALT_PASS", "secret"),
            ("MAIL_SMTP_ALT_SECURE", "true"),
        ]);

        let secure_account =
            resolve_account_config(&secure_env, "alt").expect("must resolve account");
        assert_eq!(secure_account.port, 465);
        assert!(secure_account.secure);
    }

    #[test]
    fn policy_defaults_apply_when_unset() {
        let env = HashMap::new();
        let policy = load_policy_config(&env);

        assert!(!policy.send_enabled);
        assert_eq!(policy.max_recipients, DEFAULT_MAX_RECIPIENTS);
        assert_eq!(policy.max_message_bytes, DEFAULT_MAX_MESSAGE_BYTES);
        assert_eq!(policy.max_attachments, DEFAULT_MAX_ATTACHMENTS);
        assert_eq!(policy.max_attachment_bytes, DEFAULT_MAX_ATTACHMENT_BYTES);
        assert_eq!(policy.max_text_chars, DEFAULT_MAX_TEXT_CHARS);
        assert_eq!(policy.max_html_chars, DEFAULT_MAX_HTML_CHARS);
        assert_eq!(policy.connect_timeout_ms, DEFAULT_CONNECT_TIMEOUT_MS);
        assert_eq!(policy.socket_timeout_ms, DEFAULT_SOCKET_TIMEOUT_MS);
    }

    #[test]
    fn global_policy_env_vars_do_not_create_account_ids() {
        let env = env_map(&[
            ("MAIL_SMTP_DEFAULT_HOST", "smtp.example.com"),
            ("MAIL_SMTP_DEFAULT_USER", "alice"),
            ("MAIL_SMTP_DEFAULT_PASS", "secret"),
            ("MAIL_SMTP_SEND_ENABLED", "true"),
            ("MAIL_SMTP_MAX_RECIPIENTS", "10"),
            ("MAIL_SMTP_CONNECT_TIMEOUT_MS", "5000"),
        ]);

        let ids = list_account_ids(&env);
        assert_eq!(ids, vec!["default".to_string()]);
    }
}
