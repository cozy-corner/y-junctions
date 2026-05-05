# Multi-binary image hosting all walking-skeleton pipeline jobs.
# Cloud Run Jobs override `command` per Job to pick which binary to run.
#
# Build context is the repository root so that `backend/` is in scope.

FROM rust:1.90-slim-bookworm AS builder

WORKDIR /app
RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config \
 && rm -rf /var/lib/apt/lists/*

COPY backend/ /app/backend/
WORKDIR /app/backend

RUN cargo build --release \
      --bin pipeline-download-osm \
      --bin pipeline-extract-three-way \
      --bin pipeline-prepare-serving \
      --bin pipeline-load-to-cockroach

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --create-home --uid 65532 appuser

COPY --from=builder /app/backend/target/release/pipeline-download-osm /usr/local/bin/
COPY --from=builder /app/backend/target/release/pipeline-extract-three-way /usr/local/bin/
COPY --from=builder /app/backend/target/release/pipeline-prepare-serving /usr/local/bin/
COPY --from=builder /app/backend/target/release/pipeline-load-to-cockroach /usr/local/bin/

USER 65532:65532

# Placeholder; Cloud Run Job spec sets the actual `command` per job.
CMD ["pipeline-download-osm", "--help"]
