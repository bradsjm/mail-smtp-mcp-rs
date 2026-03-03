FROM rust:1-alpine AS builder
WORKDIR /app
RUN apk add --no-cache musl-dev
COPY . .
RUN cargo build --release

FROM alpine:latest
WORKDIR /app
COPY --from=builder /app/target/release/mail-smtp-mcp-rs /mail-smtp-mcp-rs
ENTRYPOINT ["/mail-smtp-mcp-rs"]
