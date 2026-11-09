FROM rust:1.96-alpine3.22
RUN apk add --no-cache \
    git build-base cmake zip unzip pkgconf musl-utils linux-headers binutils \
    zlib-dev libpng-dev libzip-dev sdl2-dev mesa-dev
ENV CARGO_HOME=/usr/local/cargo
ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_NET_RETRY=10
ENV CARGO_TERM_COLOR=always
