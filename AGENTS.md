# AGENTS.md

Guidance for coding agents working in `mail-smtp-mcp-rs`.

## Project Snapshot

- Language: Rust (edition `2024`)
- Runtime: `tokio`
- Protocol framework: `rmcp`
- Domain: SMTP-backed MCP server over stdio
- Binary crate name: `mail-smtp-mcp-rs`
- Entry point: `src/main.rs`
- Startup orchestration: `src/startup.rs`
- Main MCP server implementation: `src/server.rs`
- Current MCP tools: `smtp_list_accounts`, `smtp_send_message`

## Cursor/Copilot Rules

No repository-specific Cursor or Copilot instruction files were found:

- `.cursorrules`: not present
- `.cursor/rules/`: not present
- `.github/copilot-instructions.md`: not present

If these are added later, treat them as higher-priority local policy and merge them into this file.

## Build, Lint, and Test Commands

Run commands from repo root.

### Build

- Debug build: `cargo build`
- Release build: `cargo build --release`

### Format

- Check formatting: `cargo fmt -- --check`
- Apply formatting: `cargo fmt`

### Lint

- Strict clippy (all targets): `cargo clippy --all-targets -- -D warnings`

### Test

- Run all tests: `cargo test`
- List tests: `cargo test -- --list`
- GreenMail integration smoke test script: `scripts/test-greenmail.sh`
- Integration test binary directly: `cargo test --test smtp_greenmail -- --ignored --nocapture`

### Run a Single Test

Use a test name substring:

- `cargo test tool_names_match_contract`
- `cargo test startup_fails_when_no_accounts_exist`
- `cargo test validates_safe_filename`

Run a single test and show stdout:

- `cargo test tool_names_match_contract -- --exact --nocapture`
- `cargo test --test smtp_greenmail send_message_success_delivers_mail -- --ignored --exact --nocapture`

Run tests in one module/file scope:

- `cargo test config::tests`
- `cargo test policy::tests`
- `cargo test validation::tests`
- `cargo test server::tests`

## Suggested Local Validation Sequence

Before finalizing changes, run:

1. `cargo fmt -- --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test`

For SMTP integration-sensitive changes, also run:

4. `scripts/test-greenmail.sh`

Do not skip clippy or tests for code changes.

## Runtime and Policy Configuration

Required account env vars per account section:

- `MAIL_SMTP_<ACCOUNT>_HOST`
- `MAIL_SMTP_<ACCOUNT>_USER`
- `MAIL_SMTP_<ACCOUNT>_PASS`

Optional account env vars:

- `MAIL_SMTP_<ACCOUNT>_PORT` (default: `587`, or `465` when secure)
- `MAIL_SMTP_<ACCOUNT>_SECURE` (`true`/`false`)
- `MAIL_SMTP_<ACCOUNT>_FROM`

Global policy env vars:

- `MAIL_SMTP_SEND_ENABLED` (defaults to `false`; live sending is gated off by default)
- `MAIL_SMTP_ALLOWLIST_DOMAINS`
- `MAIL_SMTP_ALLOWLIST_ADDRESSES`
- `MAIL_SMTP_MAX_RECIPIENTS`
- `MAIL_SMTP_MAX_MESSAGE_BYTES`
- `MAIL_SMTP_MAX_ATTACHMENTS`
- `MAIL_SMTP_MAX_ATTACHMENT_BYTES`
- `MAIL_SMTP_MAX_TEXT_CHARS`
- `MAIL_SMTP_MAX_HTML_CHARS`
- `MAIL_SMTP_CONNECT_TIMEOUT_MS`
- `MAIL_SMTP_SOCKET_TIMEOUT_MS`

## Docker

The repository includes a multi-stage Dockerfile for running the MCP server.

### Build and Run

- Build image: `docker build -t mail-smtp-mcp-rs .`
- Run over stdio: `docker run --rm -i --env-file .env mail-smtp-mcp-rs`

### Docker Notes for Agents

- Keep MCP transport as stdio (do not add HTTP listener behavior by default).
- Runtime image is currently `debian:bookworm-slim` (not `scratch`).
- Keep `.dockerignore` aligned with repo layout to avoid leaking local files and reduce build context size.
- Docker publish workflow: `.github/workflows/publish-docker.yml`.
- Current Docker publish trigger: manual (`workflow_dispatch`).
- Published image tags in current workflow include `latest` and `vX.Y.Z`.

## CI, Release, and NPM

### GitHub Workflows

- `.github/workflows/integration.yml`: manual GreenMail integration tests.
- `.github/workflows/release.yml`: cargo-dist release flow and npm publish.
- `.github/workflows/publish-docker.yml`: GHCR multi-arch publish.
- `.github/workflows/init-npm-placeholder.yml`: one-time npm placeholder initializer.

All listed workflows are currently manual (`workflow_dispatch`).

### NPM Distribution

- npm package name: `@bradsjm/mail-smtp-mcp-rs`
- Release workflow publishes npm artifacts with provenance (`npm publish --provenance`).
- Placeholder bootstrap workflow exists to initialize the npm package before trusted publishing setup.

### Dist Configuration

- Dist workspace file currently only declares workspace members in `dist-workspace.toml`.
- If dist installer/publish settings are added or changed, keep this file and CI docs in sync.

## Architecture and File Ownership

- `src/main.rs`: CLI boot, env loading, `--help`, startup delegation.
- `src/startup.rs`: startup env checks, logging init, rmcp stdio service lifecycle.
- `src/config.rs`: SMTP account discovery, policy/env parsing, defaults.
- `src/errors.rs`: `AppError` model and MCP `ErrorData` mapping.
- `src/validation.rs`: email/filename/base64 validation and message size estimation.
- `src/policy.rs`: recipient normalization and allowlist + recipient-limit enforcement.
- `src/server.rs`: MCP tool handlers, SMTP message assembly/send flow, envelope output.
- `src/lib.rs`: module exports.
- `tests/smtp_greenmail.rs`: integration harness for SMTP delivery and policy behavior.
- `scripts/test-greenmail.sh`: local integration smoke runner.

## Code Style Guidelines

### Imports

- Group imports in this order:
  1) standard library (`std::...`)
  2) external crates
  3) internal crate imports (`crate::...`)
- Keep imports explicit; avoid wildcard imports.
- Remove unused imports rather than allowing warnings.

### Formatting

- Use rustfmt defaults; do not hand-format against rustfmt.
- Keep line wrapping and chaining style consistent with existing files.
- Prefer small helper functions over deeply nested expressions.

### Types and Data Modeling

- Prefer concrete types and explicit bounds over implicit conversions.
- Keep serde + `JsonSchema` input/output types aligned with runtime behavior.
- Keep `ToolEnvelope` shape stable (`summary`, `data`, `meta`).
- Keep `meta.now_utc` and `meta.duration_ms` populated on success paths.

### Naming

- Types: `UpperCamelCase`.
- Functions/methods: `snake_case`.
- Constants: `UPPER_SNAKE_CASE`.
- Use SMTP/MCP vocabulary consistently (`account_id`, `allowlist`, `attachment`, `recipient`).

### Error Handling

- Never use `unwrap()` or `expect()` in production code paths.
- Convert failures to `AppError` with actionable context.
- Preserve stable error codes (`CONFIG_MISSING`, `VALIDATION_ERROR`, `SEND_DISABLED`, `POLICY_VIOLATION`, `ATTACHMENT_ERROR`, `SMTP_ERROR`, `UNKNOWN_ERROR`).
- Preserve `AppError` to MCP `ErrorData` mapping behavior in `src/errors.rs`.

### Validation and Bounds

- Validate all user input before SMTP operations.
- Keep hard caps and policy caps enforced for recipients, subject/body, and attachments.
- Keep strict attachment handling (safe filename + strict base64 decode).
- Preserve anti-header-injection checks for email fields and subject.

### Async and Timeouts

- Keep network operations timeout-bounded using policy-configured timeout values.
- Avoid introducing blocking calls on async paths unless intentional and justified.

### Security and Privacy

- Never log or return secrets (`*_PASS`, auth credentials, tokens).
- Keep password redaction behavior in help/startup diagnostics.
- Preserve send gate default-off posture (`MAIL_SMTP_SEND_ENABLED=false`).
- Keep allowlist enforcement behavior deterministic and explicit.

### MCP Tool Behavior

- Keep public tool names stable unless an explicit breaking contract change is intended.
- Keep `smtp_list_accounts` and `smtp_send_message` behavior aligned with `docs/tool-contract.md`.
- If tool contracts change, update `docs/tool-contract.md` in the same change.

### Testing Expectations

- Add or update unit tests near changed logic in `config`, `policy`, `validation`, `server`, or `startup` modules.
- For send-path or policy changes, add/adjust `tests/smtp_greenmail.rs` coverage.
- Keep GreenMail tests opt-in via `#[ignore]` and run with `--ignored`.

## Change Discipline for Agents

- Prefer minimal, focused diffs.
- Do not introduce compatibility shims unless requested.
- Do not silently alter public tool contracts.
- Update docs when behavior or bounds change.
- If adding/changing env vars, update `.env.example`, `docs/tool-contract.md`, and this file.

## Quick Pre-Commit Checklist

- Code formatted
- Clippy clean with warnings denied
- Tests passing
- No secrets in code/log output
- Tool contract, env docs, and policy invariants preserved
