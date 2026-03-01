FROM rust:1.85-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/mail-smtp-mcp-rs /usr/local/bin/mail-smtp-mcp-rs
ENTRYPOINT ["/usr/local/bin/mail-smtp-mcp-rs"]
