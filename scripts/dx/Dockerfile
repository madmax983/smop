# Pristine environment for the clean-room run: only a Rust toolchain + python3 (for block extraction).
FROM rust:1-slim
RUN apt-get update -qq && apt-get install -y -qq python3 >/dev/null
COPY README.md /README.md
COPY scripts/dx/clean_room.sh /clean_room.sh
ENV README=/README.md
CMD ["bash", "/clean_room.sh"]
