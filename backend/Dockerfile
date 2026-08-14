# Chef stage - prepare recipe
# Keep in sync with .mise.toml and the CI workflows: sqlx 0.9.0 requires
# rustc 1.94.0. Dependabot does not watch this pin (no docker ecosystem in
# .github/dependabot.yml), so it has to move by hand.
#
# The Debian suite must be pinned explicitly, not left to the default `rust:X.Y`
# tag: that default has moved past bookworm, and a binary linked against its
# newer glibc dies on the bookworm-slim runtime below with
# `GLIBC_2.38 not found`. Builder and runtime have to name the same suite.
FROM rust:1.94-bookworm AS chef
RUN cargo install cargo-chef
WORKDIR /app

# Planner stage - generate recipe
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
RUN cargo chef prepare --recipe-path recipe.json

# Builder stage - build dependencies then app
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

# Build dependencies - this layer is cached
RUN cargo chef cook --release --recipe-path recipe.json

# Copy source and build app
COPY src ./src
COPY migrations ./migrations
COPY Cargo.toml Cargo.lock ./
RUN cargo build --release --bin server

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/server /app/server

# Copy migrations for sqlx
COPY --from=builder /app/migrations /app/migrations

# Expose port
EXPOSE 8080

# Run the binary
CMD ["/app/server"]
