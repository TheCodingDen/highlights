FROM rust:slim as builder
WORKDIR /usr/src/highlights
COPY ./Cargo.toml ./Cargo.lock ./
COPY ./src/ ./src
RUN cargo install --path .

FROM debian:buster-slim
COPY --from=builder /usr/local/cargo/bin/highlights /usr/local/bin/highlights
WORKDIR /opt/highlights
CMD ["/usr/local/bin/highlights"]
