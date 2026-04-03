FROM rust:1.85-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY crates ./crates

RUN cargo build --release -p pokrov-runtime

FROM debian:bookworm-slim

RUN useradd --uid 10001 --create-home --shell /usr/sbin/nologin pokrov
WORKDIR /app

COPY --from=builder /app/target/release/pokrov-runtime /usr/local/bin/pokrov-runtime
COPY config/pokrov.example.yaml /app/config/pokrov.yaml

USER pokrov
EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/pokrov-runtime"]
CMD ["--config", "/app/config/pokrov.yaml"]

