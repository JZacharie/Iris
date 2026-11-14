# Multi-stage Dockerfile inspired by Pylos & Kusanagi
FROM rust:1.95-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY crates ./crates

RUN cargo build --release --bin iris-server

FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -u 10001 iris

WORKDIR /app
COPY --from=builder /app/target/release/iris-server /usr/local/bin/iris-server
RUN mkdir -p /app/data && chown -R iris:iris /app

USER iris

ENV IRIS_HOST=0.0.0.0 \
    IRIS_PORT=8080 \
    IRIS_STATE_FILE=/app/data/iris_state.json

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/api/health || exit 1

ENTRYPOINT ["/usr/local/bin/iris-server"]
