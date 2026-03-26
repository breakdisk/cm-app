FROM rust:1.78-slim-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY libs/ libs/
COPY services/driver-ops/ services/driver-ops/
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
RUN cargo build --release --bin driver_ops

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/driver_ops /app/driver_ops
EXPOSE 8006
ENTRYPOINT ["/app/driver_ops"]
