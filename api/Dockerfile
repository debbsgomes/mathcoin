FROM rust:1.86-slim-bookworm AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src/ ./src/
COPY migrations/ ./migrations/
# .sqlx/ cache for offline builds (SQLX_OFFLINE).
# Currently empty — the project uses sqlx::query / sqlx::query_as (runtime-checked).
# If you adopt sqlx::query! / sqlx::query_as! (compile-time-checked macros),
#   run `cargo sqlx prepare` to populate this directory.
COPY .sqlx/ ./.sqlx/

ENV SQLX_OFFLINE=true
RUN cargo build --release && cp target/release/mathcoin-api /usr/local/bin/mathcoin-api

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/bin/mathcoin-api /usr/local/bin/mathcoin-api
COPY migrations/ /app/migrations/

WORKDIR /app

EXPOSE 3000

CMD ["mathcoin-api"]
