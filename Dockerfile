# Builds helmsight from source into a minimal static image.
#   docker build -t helmsight .

FROM node:22-alpine AS web
WORKDIR /src/web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
RUN npm run build

FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev build-base
WORKDIR /src
COPY . .
COPY --from=web /src/web/dist web/dist
ENV HELMSIGHT_REQUIRE_WEB=1
RUN cargo build --release --locked -p helmsight \
 && install -D -m 0755 target/release/helmsight /out/helmsight \
 && mkdir -p /out/var/lib/helmsight /out/etc/helmsight

FROM alpine:3 AS certs
RUN apk add --no-cache ca-certificates

FROM scratch
COPY --from=certs /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=build --chown=65532:65532 /out/ /
USER 65532:65532
ENV HELMSIGHT_CONFIG=/etc/helmsight/helmsight.toml
VOLUME ["/var/lib/helmsight"]
EXPOSE 8080
ENTRYPOINT ["/helmsight"]
CMD ["serve"]
