# Stage 1: Build release binary
FROM rust:1.99.1-bookworm AS builder

WORKDIR /app

# Copy workspace manifests
COPY Cargo.toml Cargo.lock ./

# Copy member crate sources
COPY domain ./domain
COPY application ./application
COPY infrastructure ./infrastructure
COPY bootstrap ./bootstrap
COPY config ./config

# Build release binaries for CLI and microservice daemon
RUN cargo build --release -p luminair-cli -p luminair-server

# Stage 2: Distroless runtime image (Debian 12 glibc, non-root user)
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

WORKDIR /app

# Copy compiled binaries and configuration
COPY --from=builder /app/target/release/luminair-cli /usr/local/bin/luminair-cli
COPY --from=builder /app/target/release/luminair-server /usr/local/bin/luminair-server
COPY --from=builder /app/config /app/config

# Run as nonroot user (UID 65532)
USER nonroot:nonroot

ENTRYPOINT ["luminair-server"]
