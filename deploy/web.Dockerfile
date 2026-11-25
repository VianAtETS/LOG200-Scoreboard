# syntax=docker/dockerfile:1

# Image de la page seule, pour l'heberger soi-meme ou la tester en local contre un autre serveur.
# Construite depuis la racine du depot : docker build -f deploy/web.Dockerfile .

ARG BUN_VERSION=1.4.5
ARG NODE_VERSION=26.13.1

# Site statique (Astro) : le meme pour toutes les plateformes, construit une fois sur celle du build.
FROM --platform=$BUILDPLATFORM oven/bun:${BUN_VERSION}-alpine AS bun
FROM --platform=$BUILDPLATFORM node:${NODE_VERSION}-alpine3.24 AS build
ARG BUN_VERSION
ARG NODE_VERSION
WORKDIR /web
COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun
RUN test "$(bun --version)" = "${BUN_VERSION}" \
    && test "$(node --version)" = "v${NODE_VERSION}"
COPY web/package.json web/bun.lock ./
RUN --mount=type=cache,id=log200-web-bun-${BUN_VERSION},target=/root/.bun/install/cache \
    bun install --frozen-lockfile
COPY web ./
RUN bun run typecheck && bun run build

FROM caddy:2.11.7-alpine
COPY --from=build /web/dist /srv/web
COPY --chmod=755 deploy/web-entrypoint.sh /usr/local/bin/
ENTRYPOINT ["web-entrypoint.sh"]
