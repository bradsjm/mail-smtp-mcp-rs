use std::collections::HashMap;
use std::sync::Once;

use rmcp::ServiceExt;
use rmcp::transport::stdio;
use tracing_subscriber::EnvFilter;

use crate::config::{list_account_ids, load_server_config, missing_required_account_env};
use crate::errors::AppError;
use crate::server::McpServer;

static LOG_INIT: Once = Once::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupCheck {
    pub ok: bool,
    pub account_ids: Vec<String>,
    pub missing_env: Vec<String>,
}

pub fn check_startup_env(env: &HashMap<String, String>) -> StartupCheck {
    let account_ids = list_account_ids(env);
    let mut missing_env = Vec::new();

    for account_id in &account_ids {
        missing_env.extend(missing_required_account_env(env, account_id));
    }

    StartupCheck {
        ok: !account_ids.is_empty() && missing_env.is_empty(),
        account_ids,
        missing_env,
    }
}

pub async fn run() -> Result<(), AppError> {
    init_logging();
    let env: HashMap<String, String> = std::env::vars().collect();
    let startup = check_startup_env(&env);

    if startup.account_ids.is_empty() {
        eprintln!("No SMTP accounts configured. Set MAIL_SMTP_<ID>_HOST/USER/PASS.");
        return Err(AppError::ConfigMissing(
            "No SMTP accounts configured.".to_owned(),
        ));
    }

    if !startup.missing_env.is_empty() {
        eprintln!("SMTP startup configuration is incomplete. Missing required variables:");
        for key in &startup.missing_env {
            eprintln!("- {key}");
        }
        return Err(AppError::ConfigMissing(
            "Missing required SMTP account variables.".to_owned(),
        ));
    }

    let config = load_server_config(&env);
    let service = McpServer::new(config)
        .serve(stdio())
        .await
        .map_err(|error| AppError::UnknownError(format!("failed to start MCP service: {error}")))?;

    service
        .waiting()
        .await
        .map_err(|error| AppError::UnknownError(format!("MCP service failed: {error}")))?;

    Ok(())
}

fn init_logging() {
    LOG_INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    });
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::check_startup_env;

    fn env_map(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn startup_fails_when_no_accounts_exist() {
        let env = HashMap::new();
        let startup = check_startup_env(&env);

        assert!(!startup.ok);
        assert!(startup.account_ids.is_empty());
    }

    #[test]
    fn startup_fails_when_required_values_are_missing() {
        let env = env_map(&[("MAIL_SMTP_DEFAULT_HOST", "smtp.example.com")]);
        let startup = check_startup_env(&env);

        assert!(!startup.ok);
        assert_eq!(startup.account_ids, vec!["default".to_string()]);
        assert!(
            startup
                .missing_env
                .contains(&"MAIL_SMTP_DEFAULT_USER".to_string())
        );
        assert!(
            startup
                .missing_env
                .contains(&"MAIL_SMTP_DEFAULT_PASS".to_string())
        );
    }

    #[test]
    fn startup_passes_when_required_values_exist() {
        let env = env_map(&[
            ("MAIL_SMTP_DEFAULT_HOST", "smtp.example.com"),
            ("MAIL_SMTP_DEFAULT_USER", "alice"),
            ("MAIL_SMTP_DEFAULT_PASS", "secret"),
        ]);

        let startup = check_startup_env(&env);
        assert!(startup.ok);
        assert!(startup.missing_env.is_empty());
    }
}
