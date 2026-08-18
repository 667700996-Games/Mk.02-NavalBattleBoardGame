FROM rust:1.87-bookworm AS builder
ARG SOURCE_DATE_EPOCH
WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY apps/server ./apps/server
RUN cargo build --locked --release -p mk01-server

FROM debian:bookworm-slim AS runtime
ARG SOURCE_DATE_EPOCH
ARG VCS_REF
ARG VERSION
ARG BUILD_DATE
LABEL org.opencontainers.image.created=$BUILD_DATE \
  org.opencontainers.image.revision=$VCS_REF \
  org.opencontainers.image.version=$VERSION \
  org.opencontainers.image.source="https://github.com/299-792-458/Mk.01-GameProject-NavalBattleBoardGame"
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/*
RUN useradd --system --uid 10001 --create-home mk01
COPY --from=builder /workspace/target/release/mk01-server /usr/local/bin/mk01-server
USER mk01
EXPOSE 8080
ENTRYPOINT ["mk01-server"]
