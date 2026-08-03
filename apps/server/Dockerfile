FROM rust:1.87-bookworm AS builder
WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY apps/server ./apps/server
RUN cargo build --release -p mk01-server

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/*
RUN useradd --system --uid 10001 --create-home mk01
COPY --from=builder /workspace/target/release/mk01-server /usr/local/bin/mk01-server
USER mk01
EXPOSE 8080
ENTRYPOINT ["mk01-server"]

