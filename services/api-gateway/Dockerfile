# syntax=docker/dockerfile:1
FROM rust:1.91-slim-bookworm AS chef
RUN apt-get update && apt-get install -y     pkg-config libssl-dev zlib1g-dev cmake build-essential     && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY libs/ libs/
COPY services/ services/
RUN cargo chef prepare --recipe-path recipe.json

# cacher: recipe + full cargo registry so native build scripts have C sources
FROM chef AS cacher
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --profile docker --locked -j4 --recipe-path recipe.json

FROM chef AS builder
COPY Cargo.toml Cargo.lock ./
COPY libs/ libs/
COPY services/ services/
COPY --from=cacher /app/target target
COPY --from=cacher /usr/local/cargo /usr/local/cargo
RUN cargo build --profile docker --locked -j4 --bin api_gateway

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y     ca-certificates libssl3 zlib1g     && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/docker/api_gateway /app/api_gateway
EXPOSE 8000
ENTRYPOINT ["/app/api_gateway"]
