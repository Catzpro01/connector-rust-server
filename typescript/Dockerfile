FROM rust:1.75-slim as builder
WORKDIR /app
COPY n8n-rust ./n8n-rust
RUN apt-get update && apt-get install -y pkg-config libssl-dev && \
    cargo build --release --manifest-path n8n-rust/Cargo.toml && \
    cp n8n-rust/target/release/n8n-server /usr/local/bin/n8n-server && \
    cp n8n-rust/target/release/n8n-cli /usr/local/bin/n8n-cli

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/n8n-server /usr/local/bin/n8n-server
COPY --from=builder /usr/local/bin/n8n-cli /usr/local/bin/n8n-cli
COPY n8n-rust/fixtures /app/fixtures
ENV HOST=0.0.0.0
ENV PORT=3000
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 CMD curl -f http://localhost:3000/health || exit 1
CMD ["n8n-server"]
