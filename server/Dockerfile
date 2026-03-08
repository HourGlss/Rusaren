FROM rust:1-bookworm AS builder

WORKDIR /app/server

COPY server/Cargo.toml server/Cargo.lock ./
COPY server/.cargo ./.cargo
COPY server/bin ./bin
COPY server/crates ./crates
COPY server/static ./static

RUN cargo build --locked --release -p dedicated_server

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --uid 10001 rarena

WORKDIR /app/server

COPY --from=builder /app/server/target/release/dedicated_server /usr/local/bin/dedicated_server
COPY --from=builder /app/server/static ./static

RUN mkdir -p /app/server/var /app/server/static/webclient \
    && chown -R rarena:rarena /app/server

USER rarena

ENV RARENA_BIND=0.0.0.0:3000
ENV RARENA_RECORD_STORE_PATH=/app/server/var/player_records.tsv
ENV RARENA_WEB_CLIENT_ROOT=/app/server/static/webclient
ENV RARENA_LOG_FORMAT=json
ENV RUST_LOG=info,axum=info,tower_http=info

EXPOSE 3000

HEALTHCHECK --interval=15s --timeout=3s --start-period=10s --retries=3 CMD curl --fail --silent http://127.0.0.1:3000/healthz || exit 1

CMD ["dedicated_server"]
