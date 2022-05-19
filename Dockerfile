FROM rust:1.61-slim-bullseye AS auditor
RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config=0.29.2-1 libssl-dev=1.1.1n-1+deb11u1 && \
    USER=root cargo new --bin highlights && \
    cargo install cargo-audit
COPY ["Cargo.*", "./"]
RUN cargo audit -D unsound -D yanked

FROM rust:1.61-alpine3.15 AS builder
RUN apk add --no-cache --update musl-dev=1.2.2-r7 && \
    USER=root cargo new --bin highlights
WORKDIR /highlights
COPY ["Cargo.toml", "Cargo.lock", "./"]
RUN cargo build --release && \
    rm src/*.rs && \
    rm ./target/release/deps/highlights*
COPY ["src", "./src"]
RUN cargo build --release

FROM alpine:3.15.0
RUN apk add --no-cache --update tini=0.19.0-r0 && \
    addgroup -g 1000 highlights \
    && adduser -u 1000 -H -D -G highlights -s /bin/sh highlights
ENTRYPOINT ["/sbin/tini", "--"]
USER highlights
WORKDIR /opt/highlights
RUN mkdir data
COPY --from=builder /highlights/target/release/highlights /usr/local/bin/highlights
CMD ["/usr/local/bin/highlights"]
