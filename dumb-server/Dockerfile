# Builder and runtime share Debian bookworm so the binary's glibc matches.
FROM rust:1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked -p server \
    && cp target/release/server /usr/local/bin/server

FROM debian:bookworm-slim
COPY --from=builder /usr/local/bin/server /usr/local/bin/server
EXPOSE 8080
# The server shuts down gracefully on ctrl-c (SIGINT); docker stop sends this instead of SIGTERM.
STOPSIGNAL SIGINT
ENTRYPOINT ["/usr/local/bin/server"]
