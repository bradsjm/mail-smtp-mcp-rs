#!/usr/bin/env bash
set -euo pipefail

IMAGE="greenmail/standalone:2.1.8"
NAME="mail-smtp-mcp-rs-greenmail-test"
GREENMAIL_OPTS="-Dgreenmail.setup.test.smtp -Dgreenmail.setup.test.imap -Dgreenmail.hostname=0.0.0.0 -Dgreenmail.users=sender:secret@example.com,recipient:secret@example.com"

cleanup() {
  docker rm -f "$NAME" >/dev/null 2>&1 || true
}
trap cleanup EXIT

docker rm -f "$NAME" >/dev/null 2>&1 || true
docker pull "$IMAGE"

docker run -d --rm --name "$NAME" \
  -e "GREENMAIL_OPTS=$GREENMAIL_OPTS" \
  -p 3025:3025 \
  -p 3143:3143 \
  "$IMAGE"

echo "Waiting for GreenMail on localhost:3025 and localhost:3143"
ready=0
for _ in {1..60}; do
  if bash -c "exec 3<>/dev/tcp/127.0.0.1/3025" 2>/dev/null \
    && bash -c "exec 3<>/dev/tcp/127.0.0.1/3143" 2>/dev/null; then
    ready=1
    break
  fi
  sleep 1
done

if [[ "$ready" -ne 1 ]]; then
  echo "GreenMail did not become ready in time" >&2
  docker logs "$NAME" >&2 || true
  exit 1
fi

echo "Running ignored GreenMail integration tests"
RUN_GREENMAIL_TESTS=1 \
GREENMAIL_EXTERNAL=1 \
GREENMAIL_HOST=localhost \
GREENMAIL_SMTP_PORT=3025 \
GREENMAIL_IMAP_PORT=3143 \
cargo test --test smtp_greenmail -- --nocapture
