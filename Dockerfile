# ---- Builder Stage ----
FROM rust:slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/edgewit

# Create a dummy project to cache dependencies
COPY Cargo.toml build.rs ./
# Create a dummy src/main.rs to allow cargo to build the dependencies
RUN mkdir -p src/bin benches && \
    echo "fn main() {}" > src/main.rs && \
    echo "fn main() {}" > src/bin/generate_openapi.rs && \
    echo "fn main() {}" > benches/edgewit_bench.rs && \
    cargo build --release && \
    rm -rf src

# Copy the actual source code
COPY src ./src

# Pass build arguments
ARG GIT_COMMIT_HASH
ARG BUILD_DATE
ENV GIT_COMMIT_HASH=${GIT_COMMIT_HASH}
ENV BUILD_DATE=${BUILD_DATE}

# Touch the main file to invalidate the dummy build cache for the main crate
# and build the actual application
RUN touch src/main.rs && cargo build --release

# ---- Runtime Stage ----
# Use debian-slim to provide a minimal glibc environment
FROM debian:bookworm-slim

# Install runtime dependencies (e.g., ca-certificates for TLS if needed later)
RUN apt-get update && \
    apt-get install -y ca-certificates util-linux && \
    rm -rf /var/lib/apt/lists/*

# Create a non-root user and group for security
RUN groupadd -r edgewit && useradd -r -g edgewit -s /bin/false edgewit

# Create the data directory for the WAL and Tantivy segments and ensure proper ownership/permissions
# Ensure /data (and subdirs) are owned by the non-root `edgewit` user so runtime writes (WAL, indexes) succeed.
RUN mkdir -p /data/indexes && chown -R edgewit:edgewit /data && chmod -R 0755 /data

WORKDIR /app

# Copy the compiled binary from the builder
COPY --from=builder /usr/src/edgewit/target/release/edgewit /usr/local/bin/edgewit

# Copy entrypoint script and make it executable
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

# Set default environment variables
ENV RUST_LOG=info
ENV EDGEWIT_DATA_DIR=/data
ENV EDGEWIT_BIND_ADDR=0.0.0.0:9200

# Expose port 9200 (OpenSearch default port)
EXPOSE 9200

# Mount point for persistent storage
VOLUME ["/data"]

# Use entrypoint script to fix permissions before running
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["edgewit"]
