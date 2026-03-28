# mail-smtp-mcp-rs

`mail-smtp-mcp-rs` is a secure SMTP Model Context Protocol (MCP) server that runs over stdio.
It provides a focused tool surface for listing configured SMTP accounts and sending policy-bounded email messages.

## Features

- **SMTP-focused MCP surface**: two tools, `smtp_list_accounts` and `smtp_send_message`
- **Secure-by-default send gate**: live sends are blocked unless `MAIL_SMTP_SEND_ENABLED=true`
- **Policy enforcement**: recipient allowlists and configurable limits for recipients, message size, body size, and attachments
- **Input hardening**: strict email validation, CR/LF header-injection checks, strict base64 decoding, and safe attachment filename checks
- **Structured responses**: consistent envelope with `summary`, `data`, and execution `meta`
- **Multi-account support**: discover and route by `MAIL_SMTP_<ACCOUNT>_*` environment sections
- **Rust + tokio runtime**: async MCP server implementation with `rmcp`

## Installation

Pick one of the supported install methods.

Supported release artifacts:

- macOS: `aarch64-apple-darwin`, `x86_64-apple-darwin`
- Linux: `x86_64-unknown-linux-gnu`, `x86_64-unknown-linux-musl`
- Windows: `x86_64-pc-windows-msvc`

Linux musl support covers `x86_64` systems such as Alpine. Linux ARM64 npm installs are not supported.

### NPX (recommended)

```bash
npx -y @bradsjm/mail-smtp-mcp-rs@latest
```

Or install globally:

```bash
npm install -g @bradsjm/mail-smtp-mcp-rs
mail-smtp-mcp-rs
```

### Curl installer (Linux/macOS)

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/bradsjm/mail-smtp-mcp-rs/releases/download/v0.1.0/mail-smtp-mcp-rs-installer.sh | sh
```

Safer alternative (download, inspect, then run):

```bash
curl --proto '=https' --tlsv1.2 -LsSf -o mail-smtp-mcp-rs-installer.sh https://github.com/bradsjm/mail-smtp-mcp-rs/releases/download/v0.1.0/mail-smtp-mcp-rs-installer.sh
sh mail-smtp-mcp-rs-installer.sh
```

### Docker

Pull and run from GHCR:

```bash
docker pull ghcr.io/bradsjm/mail-smtp-mcp-rs:latest
docker run --rm -i --env-file .env ghcr.io/bradsjm/mail-smtp-mcp-rs:latest
```

Build locally:

```bash
docker build -t mail-smtp-mcp-rs .
docker run --rm -i --env-file .env mail-smtp-mcp-rs
```

### From source

```bash
cargo install --path .
```

Binary path: `~/.cargo/bin/mail-smtp-mcp-rs`.

## Quick Start

### 1) Configure an account and policy env vars

At least one SMTP account section is required at startup.

```bash
# Required account section
MAIL_SMTP_DEFAULT_HOST=smtp.example.com
MAIL_SMTP_DEFAULT_USER=your-user
MAIL_SMTP_DEFAULT_PASS=your-app-password

# Optional account fields
MAIL_SMTP_DEFAULT_PORT=587
MAIL_SMTP_DEFAULT_SECURE=false
MAIL_SMTP_DEFAULT_FROM=noreply@example.com

# Global policy
MAIL_SMTP_SEND_ENABLED=false
MAIL_SMTP_ALLOWLIST_DOMAINS=
MAIL_SMTP_ALLOWLIST_ADDRESSES=
MAIL_SMTP_MAX_RECIPIENTS=10
MAIL_SMTP_MAX_MESSAGE_BYTES=2500000
MAIL_SMTP_MAX_ATTACHMENTS=5
MAIL_SMTP_MAX_ATTACHMENT_BYTES=2000000
MAIL_SMTP_MAX_TEXT_CHARS=20000
MAIL_SMTP_MAX_HTML_CHARS=50000
MAIL_SMTP_CONNECT_TIMEOUT_MS=10000
MAIL_SMTP_SOCKET_TIMEOUT_MS=20000
```

Use app passwords or service credentials where your provider requires them.

### 2) Wire the server into your MCP client

Example MCP config:

```json
{
  "mcpServers": {
    "mail-smtp": {
      "command": "npx",
      "args": ["-y", "@bradsjm/mail-smtp-mcp-rs@latest"],
      "env": {
        "MAIL_SMTP_DEFAULT_HOST": "smtp.example.com",
        "MAIL_SMTP_DEFAULT_USER": "your-user",
        "MAIL_SMTP_DEFAULT_PASS": "your-app-password",
        "MAIL_SMTP_DEFAULT_FROM": "noreply@example.com",
        "MAIL_SMTP_SEND_ENABLED": "false"
      }
    }
  }
}
```

### 3) Enable sending only when ready

Live delivery is intentionally disabled by default:

```bash
MAIL_SMTP_SEND_ENABLED=true
```

## Multiple Accounts

Account IDs are discovered from `MAIL_SMTP_<ACCOUNT>_...` keys and normalized to lowercase.

```bash
# Default account
MAIL_SMTP_DEFAULT_HOST=smtp.gmail.com
MAIL_SMTP_DEFAULT_USER=user@gmail.com
MAIL_SMTP_DEFAULT_PASS=app-password

# Work account
MAIL_SMTP_WORK_HOST=smtp.office365.com
MAIL_SMTP_WORK_USER=user@company.com
MAIL_SMTP_WORK_PASS=work-password
MAIL_SMTP_WORK_SECURE=true
MAIL_SMTP_WORK_PORT=465
```

## Tool Reference

All tools return a common envelope:

```json
{
  "summary": "Human-readable outcome",
  "data": {},
  "meta": {
    "now_utc": "2026-03-01T12:34:56Z",
    "duration_ms": 42
  }
}
```

| Tool | Purpose |
|------|---------|
| `smtp_list_accounts` | List configured account metadata (`account_id`, host, port, secure, optional default sender) |
| `smtp_send_message` | Validate and send one message with optional `cc`, `bcc`, `reply_to`, `html_body`, and attachments |

For complete schema-level details and validation rules, see [docs/tool-contract.md](docs/tool-contract.md).

## Message and Policy Limits

Hard safety caps are enforced in code:

- max recipients: 50
- max attachments: 10
- max bytes per attachment: 5,000,000
- max text chars: 100,000
- max HTML chars: 200,000
- subject length: 1..=256 chars

Policy caps are configurable by env and can be stricter than hard caps.

## Troubleshooting

### Startup fails with missing config

The server requires at least one complete account (`HOST`, `USER`, `PASS`).

### Sending is disabled

If `smtp_send_message` returns a send-disabled error, set:

```bash
MAIL_SMTP_SEND_ENABLED=true
```

### Account lookup fails

`account_id` matching is case-insensitive and normalized to lowercase. Verify that the account section exists and includes required keys.

### SMTP connection or send failures

Check host/port/secure settings, credentials, and network reachability. Tune:

- `MAIL_SMTP_CONNECT_TIMEOUT_MS`
- `MAIL_SMTP_SOCKET_TIMEOUT_MS`

### npm install or npx fails on an unsupported platform

The published npm package supports only the release artifacts listed above. Common unsupported cases include Linux ARM64 and Windows ARM64.

On Linux `x86_64`, both glibc and musl environments are supported. For other platforms, use Docker or install from source if you need to run the server outside the published npm matrix.

## Security Notes

- Secrets are not returned in tool responses.
- Password values are redacted in help diagnostics.
- Recipient policy enforcement supports domain and exact-address allowlists.
- Attachment handling validates filename safety, content type parsing, and strict base64 input.

## Development

Contributor guidance and validation flow are documented in `AGENTS.md`.

```bash
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Optional SMTP integration smoke test:

```bash
scripts/test-greenmail.sh
```

Run GreenMail integration tests directly (requires a running GreenMail endpoint):

```bash
cargo test --test smtp_greenmail -- --ignored --nocapture
```

## License

MIT License. See [LICENSE](LICENSE).
