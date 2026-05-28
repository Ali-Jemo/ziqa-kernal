FROM rust:latest AS base

RUN apt-get update && apt-get install -y --no-install-recommends \
    qemu-system-x86-64 \
    lld \
    && rm -rf /var/lib/apt/lists/*

RUN rustup toolchain install nightly --component rust-src --component llvm-tools-preview \
    && rustup target add x86_64-unknown-none --toolchain nightly \
    && cargo install bootimage

WORKDIR /workspace
COPY . .

FROM base AS dev
CMD ["/bin/bash"]

FROM base AS build
RUN cargo build --bin ziqa-kernel
RUN cargo bootimage

FROM build AS run
CMD ["qemu-system-x86_64", "-drive", "format=raw,file=target/x86_64-unknown-none/debug/bootimage-ziqa-kernel.bin", "-serial", "stdio", "-display", "none"]
