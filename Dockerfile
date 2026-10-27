# Stage 1: Build release binary
FROM rust:1.85-bookworm AS builder

WORKDIR /app

# Copy workspace manifests
COPY Cargo.toml Cargo.lock ./

# Copy member crate sources
COPY domain ./domain
COPY application ./application
COPY infrastructure ./infrastructure
COPY config ./config

# Build release binary for infrastructure crate
RUN cargo build --release -p infrastructure

# Stage 2: Lightweight runtime image
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy compiled binary and configuration
COPY --from=builder /app/target/release/infrastructure /usr/local/bin/luminair-cli
COPY --from=builder /app/config /app/config

ENTRYPOINT ["luminair-cli"]
