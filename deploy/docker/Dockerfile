# syntax=docker/dockerfile:1
FROM rust:bookworm AS build
WORKDIR /src
COPY . .
RUN apt-get update && apt-get install -y --no-install-recommends unzip && rm -rf /var/lib/apt/lists/*
RUN curl -fsSLo /tmp/bun.zip "https://github.com/oven-sh/bun/releases/download/bun-v$(cat .bun-version)/bun-linux-$(case "$(uname -m)" in x86_64) echo x64;; aarch64) echo aarch64;; esac).zip" \
    && unzip /tmp/bun.zip -d /tmp/bun && cp /tmp/bun/*/bun /usr/local/bin/bun
RUN bun install --frozen-lockfile
RUN bun run --filter @omochi-ui/core build && bun run --filter @omochi-ui/renderer build && bun run --filter @omochi-ui/hub-ui build
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --manifest-path crates/Cargo.toml --locked --release -p omochi-cli -p omochi-utils

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl libgcc-s1 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 omochi && useradd --uid 10001 --gid 10001 --create-home omochi \
    && install -d -o 10001 -g 10001 -m 0700 /run/omochi /run/omochi/python-example /var/lib/omochi
COPY --from=build /src/crates/target/release/omochi-ui /src/crates/target/release/omochi-utils /usr/local/bin/
ENV XDG_RUNTIME_DIR=/run OMOCHI_CONFIG_DIR=/var/lib/omochi
USER 10001:10001
EXPOSE 5555
ENTRYPOINT ["omochi-ui"]
CMD ["hub", "--host", "0.0.0.0"]
