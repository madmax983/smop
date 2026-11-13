# Clean-room first run in a pristine container: only a Rust toolchain.
# smop comes from crates.io, as a newcomer gets it.
#   docker build -f xtask/clean-room.Dockerfile -t smop-cleanroom .
#   docker run --rm smop-cleanroom
FROM rust:1-slim
COPY . /smop
WORKDIR /smop
RUN cargo build --quiet --package xtask
CMD ["cargo", "xtask", "clean-room"]
