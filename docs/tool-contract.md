# Tool Contract

This document defines the stable MCP tool surface for `mail-smtp-mcp-rs`.

## Tools

- `smtp_list_accounts`
- `smtp_send_message`

## `smtp_list_accounts`

Description: list configured SMTP account metadata.

Input:

- `account_id?: string`
  - Optional account filter.
  - Match is case-insensitive (normalized to lowercase).

Success envelope:

- `summary: string`
- `data.accounts: Account[]`
- `meta.now_utc: string` (RFC3339)
- `meta.duration_ms: number`

`Account` shape:

- `account_id: string`
- `host: string`
- `port: number`
- `secure: boolean`
- `default_from?: string`

Error behavior:

- Unknown filtered `account_id` -> `CONFIG_MISSING`.

## `smtp_send_message`

Description: validate and send one SMTP message through a configured account.

Input:

- `account_id?: string` (default `"default"`)
- `from?: string` (falls back to account `MAIL_SMTP_<ACCOUNT>_FROM`)
- `reply_to?: string`
- `to: string | string[]` (required)
- `cc?: string | string[]`
- `bcc?: string | string[]`
- `subject: string` (1..=256 chars, no CR/LF)
- `text_body?: string`
- `html_body?: string`
- `attachments?: AttachmentInput[]`

`AttachmentInput`:

- `filename: string` (safe filename required)
- `content_base64: string` (strict base64; no surrounding whitespace)
- `content_type?: string` (must parse as MIME content type when provided)

Success envelope:

- `summary: string`
- `data.account_id: string`
- `data.envelope.from: string`
- `data.envelope.to: string[]`
- `data.envelope.cc?: string[]`
- `data.envelope.bcc?: string[]`
- `data.message_id: null | string` (currently `null`)
- `data.accepted: string[]`
- `data.rejected: string[]` (currently empty on success)
- `meta.now_utc: string` (RFC3339)
- `meta.duration_ms: number`

Validation and policy enforcement:

- Send gate: `MAIL_SMTP_SEND_ENABLED` must be `true`.
- Recipient normalization: trim + lowercase; at least one `to` recipient required.
- Allowlist enforcement via:
  - `MAIL_SMTP_ALLOWLIST_DOMAINS`
  - `MAIL_SMTP_ALLOWLIST_ADDRESSES`
- Hard caps:
  - recipients `<= 50`
  - attachments `<= 10`
  - per-attachment bytes `<= 5_000_000`
  - text chars `<= 100_000`
  - html chars `<= 200_000`
- Policy caps (env-configurable):
  - `MAIL_SMTP_MAX_RECIPIENTS`
  - `MAIL_SMTP_MAX_ATTACHMENTS`
  - `MAIL_SMTP_MAX_ATTACHMENT_BYTES`
  - `MAIL_SMTP_MAX_TEXT_CHARS`
  - `MAIL_SMTP_MAX_HTML_CHARS`
  - `MAIL_SMTP_MAX_MESSAGE_BYTES`
- Message size guard uses an estimate including body bytes, MIME overhead, and attachment base64 transport expansion.

Timeout policy:

- SMTP transport timeout is derived from `MAIL_SMTP_CONNECT_TIMEOUT_MS` and `MAIL_SMTP_SOCKET_TIMEOUT_MS` and applied uniformly across secure and insecure transport modes.

Error code mapping:

- `CONFIG_MISSING`
- `VALIDATION_ERROR`
- `SEND_DISABLED`
- `POLICY_VIOLATION`
- `ATTACHMENT_ERROR`
- `SMTP_ERROR`
- `UNKNOWN_ERROR`

## Environment discovery rules

Account discovery only considers keys matching:

- `MAIL_SMTP_<ACCOUNT>_HOST`
- `MAIL_SMTP_<ACCOUNT>_USER`
- `MAIL_SMTP_<ACCOUNT>_PASS`
- `MAIL_SMTP_<ACCOUNT>_PORT`
- `MAIL_SMTP_<ACCOUNT>_SECURE`
- `MAIL_SMTP_<ACCOUNT>_FROM`

Global policy keys (for example `MAIL_SMTP_SEND_ENABLED`) do not create account IDs.
