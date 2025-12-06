# Chef stage - prepare recipe
FROM rust:1.90 AS chef
RUN cargo install cargo-chef
WORKDIR /app

# Planner stage - generate recipe
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

# Builder stage - build dependencies then app
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json

# Build dependencies - this layer is cached
RUN cargo chef cook --release --recipe-path recipe.json

# Copy source and build app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
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
