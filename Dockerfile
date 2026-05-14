# syntax=docker/dockerfile:1
#
# Container image for the linear-clone issue tracker (axum + SQLite + the
# bundled React UI). symphony itself is intentionally NOT containerized here:
# it shells out to agent CLIs and needs host git identity / workspace mounts,
# which is a separate design problem.

# ---- Stage 1: build the web UI ----
# vite's outDir is ../crates/linear-clone/static (relative to web/), so the
# build output lands at /src/crates/linear-clone/static.
FROM node:22-slim AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web/ ./
RUN npm run build

# ---- Stage 2: build the linear-clone binary ----
# sqlx::migrate! embeds crates/linear-clone/migrations at compile time, so the
# migrations dir must be present in the build context (it is, via COPY crates/).
# The whole crates/ tree is copied because cargo must parse every workspace
# member's manifest even when only -p linear-clone is built.
FROM rust:1-slim-bookworm AS rust
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
RUN cargo build --release -p linear-clone

# ---- Stage 3: runtime ----
# bookworm-slim matches the rust:1-slim-bookworm build image's glibc. sqlx's
# sqlite feature statically bundles SQLite, so no DB libs are needed here.
FROM debian:bookworm-slim AS runtime
RUN useradd --system --uid 10001 --create-home app
WORKDIR /app
COPY --from=rust /src/target/release/linear-clone /usr/local/bin/linear-clone
COPY --from=web /src/crates/linear-clone/static /app/static
RUN mkdir -p /data && chown app:app /data
USER app
# HOST=0.0.0.0 so the port is reachable from outside the container; the DB
# lives under /data, which docker-compose mounts as a named volume.
ENV LINEAR_CLONE_HOST=0.0.0.0 \
    LINEAR_CLONE_WEB=/app/static \
    LINEAR_CLONE_DB=/data/linear-clone.db
EXPOSE 4000
CMD ["linear-clone"]
