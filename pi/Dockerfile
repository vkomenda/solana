# Original source: https://gist.github.com/pepyakin/2ff227c2d837a2eacd8d3879d5e0c94f
# Changed the target from ARMv7 to Aarch64.

FROM rust:latest

RUN dpkg --add-architecture arm64 && \
    apt-get update && apt-get upgrade -y && \
    apt-get install -y aptitude && \
    aptitude install -y \
        gcc-aarch64-linux-gnu \
        g++-aarch64-linux-gnu \
        pkg-config \
        cmake \
        git \
        clang \
        libclang-dev \
        libhidapi-dev \
        libhidapi-dev:arm64 \
        libssl-dev \
        libssl-dev:arm64 \
        libudev-dev \
        libudev-dev:arm64

RUN rustup install nightly
RUN rustup target add aarch64-unknown-linux-gnu --toolchain nightly
RUN rustup default nightly

ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER aarch64-linux-gnu-gcc
ENV PKG_CONFIG_ALLOW_CROSS 1
ENV PKG_CONFIG_PATH /usr/lib/aarch64-linux-gnu/pkgconfig/

# Disallow the `pkg-config` crate to look for the config for zlib, because build.rs of `libz-sys`
# gets confused and pulls the system-wide library (i.e. of the host) instead of the target when
# cross-compiling. This essentially leads to static linking of zlib.
#
# Alternatively, we can supply LIBZ_SYS_STATIC=1. Weirdly enough, installing libgtk-3-dev:armhf
# also solves the problem somehow.
#
# Here is the related issue: https://github.com/rust-lang/libz-sys/issues/49
ENV ZLIB_NO_PKG_CONFIG 1

RUN useradd rust --user-group --create-home --shell /bin/bash --groups sudo
WORKDIR /home/rust/src
