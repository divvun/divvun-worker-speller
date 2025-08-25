# Build stage
FROM rust:1.75-bullseye as builder

WORKDIR /usr/src/app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build for release
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install required packages
RUN dpkg --add-architecture amd64 && \
    apt-get update && apt-get install -y \
    wget \
    gnupg2 \
    ca-certificates \
    lsb-release \
    && rm -rf /var/lib/apt/lists/*

# Download and install the nightly repository script
RUN wget https://apertium.projectjj.com/apt/install-nightly.sh \
    && bash install-nightly.sh \
    && rm install-nightly.sh

# Install divvun dependencies and curl for health checks
RUN apt-get update && apt-get install -y \
    divvun-gramcheck:amd64 \
    hfst:amd64 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder stage
COPY --from=builder /usr/src/app/target/release/divvun-worker-speller /usr/local/bin/divvun-worker-speller

# Create non-root user
RUN useradd -r -u 1000 speller

# Create data directory and set permissions
RUN mkdir -p /data && chown speller:speller /data

USER speller

EXPOSE 4000

# Health check for Kubernetes and Docker
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:4000/health || exit 1

ENTRYPOINT ["/usr/local/bin/divvun-worker-speller"]
