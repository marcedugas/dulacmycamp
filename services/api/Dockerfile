# ── build ──
FROM rust:1.95-slim-bookworm AS builder

RUN apt-get update \
 && apt-get install -y --no-install-recommends pkg-config \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Compile dependencies against a stub binary first, so that editing our own
# source doesn't rebuild the whole dependency tree on every deploy.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src \
 && echo 'fn main() {}' > src/main.rs \
 && echo '' > src/lib.rs \
 && cargo build --release \
 && rm -rf src

COPY src ./src
COPY migrations ./migrations

# Touch so cargo sees our real sources as newer than the stub build.
RUN touch src/main.rs src/lib.rs && cargo build --release

# ── runtime ──
FROM debian:bookworm-slim

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates util-linux \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --create-home --uid 10001 camp

COPY --from=builder /build/target/release/dulacmycamp-api /usr/local/bin/dulacmycamp-api
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh
RUN chmod +x /usr/local/bin/docker-entrypoint.sh

# Deliberately no USER here — the entrypoint starts as root (needed to chown
# the mounted upload volume) and drops to `camp` itself via setpriv before
# the app ever runs. See docker-entrypoint.sh.
ENV BIND_ADDR=0.0.0.0:8080
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["dulacmycamp-api"]
