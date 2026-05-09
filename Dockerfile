# syntax=docker/dockerfile:1.7

FROM lukemathwalker/cargo-chef:latest-rust-1.88.0 AS chef
WORKDIR /app
RUN apt update && apt install lld clang -y


FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,id=toki-api-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=toki-api-cargo-git,target=/usr/local/cargo/git \
    --mount=type=cache,id=toki-api-target,target=/app/target \
    cargo chef cook --profile deploy --bin toki-api --recipe-path recipe.json

# Build application
COPY . .
RUN --mount=type=cache,id=toki-api-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=toki-api-cargo-git,target=/usr/local/cargo/git \
    --mount=type=cache,id=toki-api-target,target=/app/target \
    cargo build --profile deploy --bin toki-api \
    && cp /app/target/deploy/toki-api /app/toki-api-bin

FROM ubuntu:22.04 AS runtime
WORKDIR /app
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates \
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*
# Copy necessary files from builder
COPY --from=builder /app/toki-api-bin toki-api
COPY --from=builder /app/toki-api/config config
EXPOSE 8080
ENTRYPOINT ["./toki-api"]
