# One image: Rust API + the built React app (served by the API from WEB_DIR).
FROM node:22-alpine AS web
WORKDIR /web
COPY apps/web/package.json apps/web/package-lock.json ./
RUN npm ci
COPY apps/web ./
RUN npm run build

FROM rust:1-bookworm AS api
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations
RUN cargo build --release -p finguard-api

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* && useradd -r -u 10001 app
COPY --from=api /src/target/release/finguard-api /usr/local/bin/finguard-api
COPY --from=web /web/dist /srv/web
ENV WEB_DIR=/srv/web
USER app
EXPOSE 8080
CMD ["finguard-api"]
