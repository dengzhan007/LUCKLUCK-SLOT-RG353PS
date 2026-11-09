# Build as linux/arm64 so all output binaries use the native AArch64/musl ABI.
FROM alpine:3.22
RUN apk add --no-cache \
    bash build-base cmake git make linux-headers pkgconf python3 pax-utils \
    zlib-dev libpng-dev libzip-dev
ENV MAKE_JOBS=2
WORKDIR /build
COPY build-cores.sh /build/build-cores.sh
RUN chmod 0755 /build/build-cores.sh
ENTRYPOINT ["/bin/bash", "/build/build-cores.sh"]
