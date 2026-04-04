# syntax=docker/dockerfile:1
FROM rust:1.91-slim-bookworm AS chef
RUN apt-get update && apt-get install -y pkg-config libssl-dev librdkafka-dev && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY libs/ libs/
COPY services/ services/
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS cacher
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --profile docker --locked -j4 --recipe-path recipe.json

FROM chef AS builder
COPY Cargo.toml Cargo.lock ./
COPY libs/ libs/
COPY services/ services/
COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
RUN cargo build --profile docker --locked -j4 --bin hub_ops

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y ca-certificates libssl3 librdkafka1 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/docker/hub_ops /app/hub_ops
EXPOSE 8009
ENTRYPOINT ["/app/hub_ops"]
