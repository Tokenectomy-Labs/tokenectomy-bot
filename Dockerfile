# Build Stage
FROM rust:1.85-alpine AS builder

RUN apk add --no-cache musl-dev gcc git

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build --release --bin tokenectomy-bot

# Runtime Stage
FROM alpine:3.21

RUN apk add --no-cache git ca-certificates

COPY --from=builder /app/target/release/tokenectomy-bot /usr/local/bin/tokenectomy-bot

ENTRYPOINT ["tokenectomy-bot"]
CMD ["--help"]
