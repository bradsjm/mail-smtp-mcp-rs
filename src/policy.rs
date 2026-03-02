use crate::config::PolicyConfig;
use crate::errors::AppError;
use crate::validation::{email_domain, normalize_address, validate_email_address};

/// Represents the recipients of an email, including To, Cc, and Bcc fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipients {
    /// List of primary recipients (To).
    pub to: Vec<String>,
    /// List of carbon copy recipients (Cc).
    pub cc: Vec<String>,
    /// List of blind carbon copy recipients (Bcc).
    pub bcc: Vec<String>,
}

impl Recipients {
    /// Returns the total number of recipients (to + cc + bcc).
    pub fn total(&self) -> usize {
        self.to.len() + self.cc.len() + self.bcc.len()
    }

    /// Returns an iterator over all recipient email addresses.
    pub fn all(&self) -> impl Iterator<Item = &str> {
        self.to
            .iter()
            .chain(self.cc.iter())
            .chain(self.bcc.iter())
            .map(String::as_str)
    }
}

/// Normalizes and validates recipient lists, ensuring at least one "to" recipient is present.
///
/// Trims whitespace, lowercases addresses, and validates email format.
/// Returns an error if no "to" recipient is provided or if any address is invalid.
pub fn normalize_recipients(
    to: Vec<String>,
    cc: Vec<String>,
    bcc: Vec<String>,
) -> Result<Recipients, AppError> {
    let normalized = Recipients {
        to: normalize_list(to)?,
        cc: normalize_list(cc)?,
        bcc: normalize_list(bcc)?,
    };

    if normalized.to.is_empty() {
        return Err(AppError::ValidationError(
            "At least one recipient is required in to.".to_owned(),
        ));
    }

    Ok(normalized)
}

/// Enforces recipient-related policy constraints (max recipients, allowlists).
///
/// Returns an error if the recipient count exceeds policy or if recipients are not allowed.
pub fn enforce_recipient_policy(
    policy: &PolicyConfig,
    recipients: &Recipients,
) -> Result<(), AppError> {
    if recipients.total() > policy.max_recipients {
        return Err(AppError::PolicyViolation(format!(
            "Recipient limit exceeded (max {}).",
            policy.max_recipients
        )));
    }

    if policy.allowlist_domains.is_empty() && policy.allowlist_addresses.is_empty() {
        return Ok(());
    }

    for recipient in recipients.all() {
        let domain = email_domain(recipient).ok_or_else(|| {
            AppError::PolicyViolation(format!("Invalid email address: {recipient}"))
        })?;

        if policy.allowlist_addresses.contains(recipient)
            || policy.allowlist_domains.contains(domain)
        {
            continue;
        }

        return Err(AppError::PolicyViolation(format!(
            "Recipient blocked by allowlist: {recipient}"
        )));
    }

    Ok(())
}

// Normalizes and validates a list of email addresses.
fn normalize_list(input: Vec<String>) -> Result<Vec<String>, AppError> {
    let mut out = Vec::new();

    for raw in input {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }

        validate_email_address(trimmed)?;
        out.push(normalize_address(trimmed));
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::{enforce_recipient_policy, normalize_recipients};
    use crate::config::load_policy_config;
    use crate::errors::ErrorCode;

    fn env_map(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn normalizes_and_lowercases_recipient_lists() {
        let recipients = normalize_recipients(
            vec![ " Alice@Example.COM ".to_string() ],
            vec![ "  ".to_string(), "Bob@Example.com".to_string() ],
            vec![],
        )
        .expect("must normalize");

        assert_eq!(recipients.to, vec!["alice@example.com".to_string()]);
        assert_eq!(recipients.cc, vec!["bob@example.com".to_string()]);
    }

    #[test]
    fn requires_at_least_one_to_recipient() {
        let err = normalize_recipients(vec![], vec!["cc@example.com".to_string()], vec![])
            .expect_err("must fail");

        assert_eq!(err.code(), ErrorCode::ValidationError);
    }

    #[test]
    fn enforces_max_recipient_policy() {
        let mut policy = load_policy_config(&HashMap::new());
        policy.max_recipients = 1;

        let recipients = normalize_recipients(
            vec!["a@example.com".to_string()],
            vec!["b@example.com".to_string()],
            vec![],
        )
        .expect("must normalize");

        let err = enforce_recipient_policy(&policy, &recipients).expect_err("must fail");
        assert_eq!(err.code(), ErrorCode::PolicyViolation);
    }

    #[test]
    fn enforces_allowlist_rules() {
        let mut policy = load_policy_config(&env_map(&[(
            "MAIL_SMTP_ALLOWLIST_DOMAINS",
            "allowed.example",
        )]));
        policy.allowlist_addresses = HashSet::new();

        let allowed = normalize_recipients(vec!["ok@allowed.example".to_string()], vec![], vec![])
            .expect("must normalize");
        enforce_recipient_policy(&policy, &allowed).expect("must pass");

        let blocked =
            normalize_recipients(vec!["nope@blocked.example".to_string()], vec![], vec![])
                .expect("must normalize");
        let err = enforce_recipient_policy(&policy, &blocked).expect_err("must fail");
        assert_eq!(err.code(), ErrorCode::PolicyViolation);
    }
}
