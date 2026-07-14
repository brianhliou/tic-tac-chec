# syntax=docker/dockerfile:1

FROM rust:1.88-slim-bookworm AS build
WORKDIR /src
COPY solver/Cargo.toml solver/Cargo.lock ./solver/
COPY solver/src ./solver/src
COPY solver/web ./solver/web
RUN cargo build --manifest-path solver/Cargo.toml --release --bin tablebase_server

FROM debian:bookworm-slim AS artifact
ARG TABLEBASE_URL=https://github.com/brianhliou/tic-tac-chec/releases/download/tablebase-v1/post-opening-travel.ttb
ADD --checksum=sha256:f80c1899e57941a2251ffa554645ad06e66d4e5fbd349b6d2b949efd2c526c53 ${TABLEBASE_URL} /artifact/post-opening-travel.ttb

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin app
WORKDIR /app
COPY --from=build /src/solver/target/release/tablebase_server /app/tablebase_server
COPY --from=artifact /artifact/post-opening-travel.ttb /app/post-opening-travel.ttb
RUN chmod 0555 /app/tablebase_server && chmod 0444 /app/post-opening-travel.ttb

ENV HOST=0.0.0.0
ENV PORT=8080
ENV TABLEBASE_WORKERS=8
EXPOSE 8080

USER app
CMD ["/app/tablebase_server", "/app/post-opening-travel.ttb"]
