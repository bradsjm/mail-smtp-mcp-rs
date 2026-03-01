#!/usr/bin/env bash
set -euo pipefail

IMAGE="greenmail/standalone:2.1.8"
NAME="mail-smtp-mcp-rs-greenmail-inspector-test"
GREENMAIL_OPTS="-Dgreenmail.setup.test.smtp -Dgreenmail.setup.test.imap -Dgreenmail.hostname=0.0.0.0 -Dgreenmail.users=sender:secret@example.com,recipient:secret@example.com"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

GREENMAIL_EXTERNAL="${GREENMAIL_EXTERNAL:-0}"
GREENMAIL_HOST="${GREENMAIL_HOST:-localhost}"
GREENMAIL_SMTP_PORT="${GREENMAIL_SMTP_PORT:-3025}"
GREENMAIL_IMAP_PORT="${GREENMAIL_IMAP_PORT:-3143}"

cleanup() {
  if [[ "$GREENMAIL_EXTERNAL" != "1" ]]; then
    docker rm -f "$NAME" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

wait_for_greenmail() {
  local ready=0
  echo "Waiting for GreenMail on ${GREENMAIL_HOST}:${GREENMAIL_SMTP_PORT} and ${GREENMAIL_HOST}:${GREENMAIL_IMAP_PORT}"
  for _ in {1..60}; do
    if bash -c "exec 3<>/dev/tcp/${GREENMAIL_HOST}/${GREENMAIL_SMTP_PORT}" 2>/dev/null \
      && bash -c "exec 3<>/dev/tcp/${GREENMAIL_HOST}/${GREENMAIL_IMAP_PORT}" 2>/dev/null; then
      ready=1
      break
    fi
    sleep 1
  done

  if [[ "$ready" -ne 1 ]]; then
    echo "GreenMail did not become ready in time" >&2
    if [[ "$GREENMAIL_EXTERNAL" != "1" ]]; then
      docker logs "$NAME" >&2 || true
    fi
    exit 1
  fi
}

if [[ "$GREENMAIL_EXTERNAL" != "1" ]]; then
  docker rm -f "$NAME" >/dev/null 2>&1 || true
  docker pull "$IMAGE"

  docker run -d --rm --name "$NAME" \
    -e "GREENMAIL_OPTS=$GREENMAIL_OPTS" \
    -p 3025:3025 \
    -p 3143:3143 \
    "$IMAGE"
fi

wait_for_greenmail

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required for inspector assertions" >&2
  exit 1
fi

cd "$REPO_ROOT"

echo "Building server binary"
cargo build --quiet

SERVER_BIN="$REPO_ROOT/target/debug/mail-smtp-mcp-rs"

export MAIL_SMTP_DEFAULT_HOST="$GREENMAIL_HOST"
export MAIL_SMTP_DEFAULT_PORT="$GREENMAIL_SMTP_PORT"
export MAIL_SMTP_DEFAULT_SECURE="false"
export MAIL_SMTP_DEFAULT_USER="sender"
export MAIL_SMTP_DEFAULT_PASS="secret"
export MAIL_SMTP_DEFAULT_FROM="sender@example.com"
export MAIL_SMTP_SEND_ENABLED="true"
unset MAIL_SMTP_ALLOWLIST_DOMAINS
unset MAIL_SMTP_ALLOWLIST_ADDRESSES

run_inspector() {
  npx --yes @modelcontextprotocol/inspector "$SERVER_BIN" --cli "$@"
}

expect_failure_with_text() {
  local expected_text="$1"
  shift
  set +e
  local output
  output=$(run_inspector "$@" 2>&1)
  local exit_code=$?
  set -e

  if [[ "$exit_code" -eq 0 ]]; then
    echo "Expected inspector call to fail" >&2
    echo "$output" >&2
    exit 1
  fi

  if [[ "$output" != *"$expected_text"* ]]; then
    echo "Inspector failure did not include expected text: ${expected_text}" >&2
    echo "$output" >&2
    exit 1
  fi
}

wait_for_subject_delivery() {
  local subject="$1"
  python3 - "$subject" "$GREENMAIL_HOST" "$GREENMAIL_IMAP_PORT" <<'PY'
import imaplib
import sys
import time

subject = sys.argv[1]
host = sys.argv[2]
port = int(sys.argv[3])

for _ in range(40):
    try:
        conn = imaplib.IMAP4(host, port)
        conn.login("recipient", "secret")
        conn.select("INBOX")
        status, data = conn.search(None, "ALL")
        if status == "OK":
            for msg_id in data[0].split():
                fstatus, payload = conn.fetch(msg_id, "(RFC822)")
                if fstatus != "OK" or not payload:
                    continue
                for part in payload:
                    if isinstance(part, tuple):
                        raw = part[1]
                        if raw and subject.encode() in raw:
                            conn.logout()
                            sys.exit(0)
        conn.logout()
    except Exception:
        pass
    time.sleep(0.25)

sys.exit(1)
PY
}

echo "Checking MCP tool discovery"
TOOLS_JSON=$(run_inspector --method tools/list)
printf '%s\n' "$TOOLS_JSON" | jq -e '.tools | map(.name) | index("smtp_list_accounts") and index("smtp_send_message")' >/dev/null

echo "Sending message through MCP inspector"
NONCE="$(date +%s%N | cut -c1-16)"
SUBJECT="Inspector integration ${NONCE}"
BODY="Inspector path ${NONCE}"
SEND_JSON=$(run_inspector \
  --method tools/call \
  --tool-name smtp_send_message \
  --tool-arg account_id=default \
  --tool-arg to='["recipient@example.com"]' \
  --tool-arg "subject=${SUBJECT}" \
  --tool-arg "text_body=${BODY}")

printf '%s\n' "$SEND_JSON" | jq -e '(.structuredContent.data // .data) as $data | (.isError != true) and $data.account_id == "default" and (($data.accepted // []) | index("recipient@example.com") != null)' >/dev/null

echo "Verifying mail reached GreenMail inbox"
wait_for_subject_delivery "$SUBJECT" || {
  echo "Expected message subject not found in GreenMail inbox" >&2
  exit 1
}

echo "Checking SEND_DISABLED policy over MCP"
export MAIL_SMTP_SEND_ENABLED="false"
expect_failure_with_text "Sending is disabled by server policy." \
  --method tools/call \
  --tool-name smtp_send_message \
  --tool-arg account_id=default \
  --tool-arg to='["recipient@example.com"]' \
  --tool-arg subject="Disabled policy ${NONCE}" \
  --tool-arg text_body="Should fail"

echo "Checking allowlist policy over MCP"
export MAIL_SMTP_SEND_ENABLED="true"
export MAIL_SMTP_ALLOWLIST_DOMAINS="allowed.example"
expect_failure_with_text "Recipient blocked by allowlist" \
  --method tools/call \
  --tool-name smtp_send_message \
  --tool-arg account_id=default \
  --tool-arg to='["recipient@example.com"]' \
  --tool-arg subject="Allowlist policy ${NONCE}" \
  --tool-arg text_body="Should fail"

echo "MCP inspector GreenMail integration checks passed"
