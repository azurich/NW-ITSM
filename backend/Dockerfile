# NW-ITSM server image: the Rust API and the built React application.
# Build from the repository root: docker build -f backend/Dockerfile .

FROM node:22-bookworm-slim AS frontend
WORKDIR /src/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run build

FROM rust:1.97-bookworm AS backend
WORKDIR /src/backend
COPY backend/ ./
RUN cargo build --release --locked --bin nw-server

FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --no-create-home nw
COPY --from=backend /src/backend/target/release/nw-server /usr/local/bin/nw-server
COPY --from=frontend /src/frontend/dist /srv/nw-itsm/app
ENV NW_BIND_ADDRESS=0.0.0.0:8080 \
    NW_STATIC_DIR=/srv/nw-itsm/app
USER nw
EXPOSE 8080
CMD ["nw-server"]
