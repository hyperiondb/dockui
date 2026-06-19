FROM node:24-alpine AS frontend
WORKDIR /app/frontend
COPY frontend/package.json ./
RUN npm install
COPY frontend/ ./
RUN npm run build

FROM rust:1-bookworm AS backend
WORKDIR /build
COPY backend/ backend/
COPY --from=frontend /app/frontend/dist ./frontend/dist
WORKDIR /build/backend
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/backend/target \
    cargo build --release && cp target/release/dockui /usr/local/bin/dockui

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=backend /usr/local/bin/dockui /usr/local/bin/dockui
ENV DOCKUI_BIND=0.0.0.0:8080 \
    DOCKUI_DATA_DIR=/data
VOLUME /data
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/dockui"]
