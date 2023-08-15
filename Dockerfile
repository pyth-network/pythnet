FROM debian:11 as build

ARG RUST_VERSION=1.60.0

# Install OS packages
RUN apt-get update && apt-get install --yes \
    curl libssl-dev libudev-dev pkg-config zlib1g-dev llvm clang cmake make libprotobuf-dev protobuf-compiler

# Install Rust
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --quiet --no-modify-path
ENV PATH="/root/.cargo/bin:${PATH}"

RUN rustup update && rustup default ${RUST_VERSION}

# Build
WORKDIR /src
COPY . .

RUN cargo fetch
RUN cargo clean && cargo +${RUST_VERSION}-x86_64-unknown-linux-gnu build --release \
    --bin solana \
    --bin solana-keygen \
    --bin solana-genesis \
    --bin solana-faucet \
    --bin solana-test-validator \
    --bin solana-validator

FROM debian:11-slim

# Install packages needed to run solana-test-validator
RUN apt-get update && apt-get install -y bzip2 tar

# Copy artifacts from other images
COPY --from=build \
    /src/target/release/solana \
    /src/target/release/solana-faucet \
    /src/target/release/solana-genesis \
    /src/target/release/solana-keygen \
    /src/target/release/solana-test-validator \
    /src/target/release/solana-validator \
    /usr/local/bin/
