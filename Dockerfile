FROM rust:1.49.0-slim-buster AS builder
RUN USER=root cargo new --bin highlights && \
    apt-get update && \
    apt-get install -y --no-install-recommends musl-tools=1.1.21-2 pkg-config=0.29-6 libssl-dev=1.1.1d-0+deb10u4 && \
    rustup target add x86_64-unknown-linux-musl && \
    rustup component add rustfmt clippy && \
    cargo install cargo-audit && \
    mkdir highlights/.cargo && \
    printf "[build]\ntarget = \"x86_64-unknown-linux-musl\"" > highlights/.cargo/config
ENV RUSTFLAGS=-Clinker=musl-gcc
WORKDIR /highlights
COPY ["Cargo.toml", "Cargo.lock", "./"]
RUN cargo audit -D unsound -D yanked && \
    cargo fetch
COPY [".", "./"]
RUN cargo fmt -- --check && \
    cargo clippy && \
    cargo build --release && \
    cargo install --path .

FROM alpine:3.12.3
RUN addgroup -g 1000 highlights \
    && adduser -u 1000 -H -D -G highlights -s /bin/sh highlights
WORKDIR /bot
USER highlights
COPY --from=builder /usr/local/cargo/bin/highlights ./
CMD ["./highlights"]
