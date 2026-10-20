# ---- build ----
FROM rust:1.85-bookworm AS builder
WORKDIR /src
RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*
COPY . .
RUN cargo build --release -p server

# ---- runtime ----
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /src/target/release/cool-admin-rust /app/cool-admin-rust
COPY apps/server/manifest/config.yaml /app/manifest/config/config.yaml
COPY apps /app/apps
EXPOSE 8001
ENTRYPOINT ["/app/cool-admin-rust"]
