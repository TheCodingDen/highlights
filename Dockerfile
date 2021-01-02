FROM rust:1.49.0-slim-buster AS builder
SHELL ["/bin/bash", "-Eeuo", "pipefail", "-c"]
RUN USER=root cargo new --bin highlights && \
    apt-get update && \
    apt-get install -y --no-install-recommends musl-tools=1.1.21-2 pkg-config=0.29-6 libssl-dev=1.1.1d-0+deb10u4 && \
    rustup component add rustfmt clippy && \
    cargo install cargo-audit && \
    mkdir highlights/.cargo && \
    target="$(rustup target list --installed | cut -d "-" -f1)-unknown-linux-musl" && \
    rustup target add "$target" && \
    printf "[build]\ntarget = \"%s\"" "$target" > highlights/.cargo/config
ENV RUSTFLAGS=-Clinker=musl-gcc
WORKDIR /highlights
COPY ["Cargo.toml", "Cargo.lock", "./"]
RUN cargo audit -D unsound -D yanked && \
    cargo fetch && \
    rm -rf src/*.rs
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
