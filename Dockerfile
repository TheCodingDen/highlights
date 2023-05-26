FROM --platform=$BUILDPLATFORM rust:1.69-slim-bullseye AS auditor
RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config=0.29.2-1 libssl-dev=1.1.1n-0+deb11u4 && \
    USER=root cargo new --bin highlights && \
    cargo install cargo-audit
COPY ["Cargo.*", "./"]
RUN cargo audit -D unsound -D yanked

FROM --platform=$BUILDPLATFORM rust:1.69-alpine3.17 AS builder
RUN apk add --no-cache --update musl-dev=1.2.3-r5 && \
    USER=root cargo new --bin highlights

ARG RUSTTARGET
ARG MUSLHOST
ARG MUSLTARGET
RUN if [[ ! -z "$RUSTTARGET" ]]; then \
        rustup target add $RUSTTARGET && \
        wget https://more.musl.cc/11.2.1/$MUSLHOST/$MUSLTARGET-cross.tgz && \
        tar xzf $MUSLTARGET-cross.tgz; \
    fi

WORKDIR /highlights
COPY ["Cargo.toml", "Cargo.lock", "./"]
RUN cargo fetch ${RUSTTARGET:+--target $RUSTTARGET}
RUN if [[ ! -z "$RUSTTARGET" ]]; then \
        export TARGET_CC=/$MUSLTARGET-cross/bin/$MUSLTARGET-gcc; \
        mkdir .cargo && \
        echo "[target.$RUSTTARGET]" > .cargo/config.toml && \
        echo "linker = \"$TARGET_CC\"" >> .cargo/config.toml; \
    fi; \
    cargo build --release ${RUSTTARGET:+--target $RUSTTARGET} && \
    rm src/main.rs target/$RUSTTARGET/release/deps/highlights*
COPY ["src", "./src"]
RUN if [[ ! -z "$RUSTTARGET" ]]; then \
        export TARGET_CC=/$MUSLTARGET-cross/bin/$MUSLTARGET-gcc; \
    fi; \
    cargo build --release ${RUSTTARGET:+--target $RUSTTARGET} && \
    if [[ ! -z "$RUSTTARGET" ]]; then \
        mv target/$RUSTTARGET/release/highlights target/release/highlights; \
    fi

FROM alpine:3.17.0
RUN apk add --no-cache --update tini=0.19.0-r1 && \
    addgroup -g 1000 highlights \
    && adduser -u 1000 -H -D -G highlights -s /bin/sh highlights
ENTRYPOINT ["/sbin/tini", "--"]
USER highlights
WORKDIR /opt/highlights
RUN mkdir data
COPY --from=builder /highlights/target/release/highlights /usr/local/bin/highlights
CMD ["/usr/local/bin/highlights"]
