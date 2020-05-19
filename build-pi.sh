#!/bin/bash

# Run this script from the directory with the Cargo project.

cd pi

docker build . -t "solana-pi-aarch64"

cd ..

docker run --rm -it \
       --env BINDGEN_EXTRA_CLANG_ARGS="--sysroot=/usr/aarch64-linux-gnu" \
       --env CARGO_HOME=/home/rust/src/cargo_home \
       -v "$(pwd)":/home/rust/src solana-pi-aarch64 \
       cargo build --target=aarch64-unknown-linux-gnu --release
