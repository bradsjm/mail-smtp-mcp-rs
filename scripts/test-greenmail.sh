#!/usr/bin/env bash
set -euo pipefail

IMAGE="greenmail/standalone:2.1.8"
NAME="mail-smtp-mcp-rs-greenmail-test"
GREENMAIL_OPTS="-Dgreenmail.setup.test.smtp -Dgreenmail.setup.test.imap -Dgreenmail.hostname=0.0.0.0 -Dgreenmail.users=sender:secret@example.com,recipient:secret@example.com"

EXTERNAL_ENDPOINT=0
if [[ -n "${GREENMAIL_HOST+x}" || -n "${GREENMAIL_SMTP_PORT+x}" || -n "${GREENMAIL_IMAP_PORT+x}" ]]; then
  EXTERNAL_ENDPOINT=1
fi

GREENMAIL_HOST="${GREENMAIL_HOST:-localhost}"
GREENMAIL_SMTP_PORT="${GREENMAIL_SMTP_PORT:-3025}"
GREENMAIL_IMAP_PORT="${GREENMAIL_IMAP_PORT:-3143}"

STARTED_CONTAINER=0

cleanup() {
  if [[ "$STARTED_CONTAINER" -eq 1 ]]; then
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
    if [[ "$STARTED_CONTAINER" -eq 1 ]]; then
      docker logs "$NAME" >&2 || true
    fi
    exit 1
  fi
}

greenmail_reachable() {
  if bash -c "exec 3<>/dev/tcp/${GREENMAIL_HOST}/${GREENMAIL_SMTP_PORT}" 2>/dev/null \
    && bash -c "exec 3<>/dev/tcp/${GREENMAIL_HOST}/${GREENMAIL_IMAP_PORT}" 2>/dev/null; then
    return 0
  fi
  return 1
}

ensure_docker_available() {
  if ! command -v docker >/dev/null 2>&1; then
    cat >&2 <<EOF
docker is required to start GreenMail automatically.

Options:
  1) Install Docker (or provide a docker-compatible CLI on PATH)
  2) Use an externally managed GreenMail endpoint by setting one or more of:
     GREENMAIL_HOST, GREENMAIL_SMTP_PORT, GREENMAIL_IMAP_PORT
EOF
    exit 1
  fi
}

if [[ "$EXTERNAL_ENDPOINT" -eq 0 ]] && greenmail_reachable; then
  EXTERNAL_ENDPOINT=1
  echo "Detected running GreenMail endpoint on default host/ports"
fi

if [[ "$EXTERNAL_ENDPOINT" -eq 1 ]]; then
  echo "Using externally managed GreenMail endpoint"
else
  ensure_docker_available
  docker rm -f "$NAME" >/dev/null 2>&1 || true
  docker pull "$IMAGE"

  docker run -d --rm --name "$NAME" \
    -e "GREENMAIL_OPTS=$GREENMAIL_OPTS" \
    -p 3025:3025 \
    -p 3143:3143 \
    "$IMAGE"
  STARTED_CONTAINER=1
fi

wait_for_greenmail

echo "Running ignored GreenMail integration tests"
GREENMAIL_HOST="$GREENMAIL_HOST" \
GREENMAIL_SMTP_PORT="$GREENMAIL_SMTP_PORT" \
GREENMAIL_IMAP_PORT="$GREENMAIL_IMAP_PORT" \
cargo test --test smtp_greenmail -- --ignored --nocapture
