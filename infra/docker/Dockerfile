# OmniPlayer all-in-one image: API + website + bot binary + server manager.
# Bots and Minecraft servers run as child processes of the API inside this container.

# ── Rust: OmniCore / OmniBrain / OmniView ──
FROM rust:1-bookworm AS rust
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY services/omnicore services/omnicore
COPY services/omnibrain services/omnibrain
COPY services/omniview services/omniview
RUN cargo build --release -p omnibrain

# ── Web app ──
FROM node:22-bookworm AS web
WORKDIR /src
COPY package.json package-lock.json tsconfig.json ./
COPY packages packages
COPY services/server-manager services/server-manager
COPY services/streaming services/streaming
COPY apps apps
RUN npm ci && npm run build -w apps/web

# ── Runtime ──
FROM node:22-bookworm
ARG MEDIAMTX_VERSION=v1.21.2
RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg espeak-ng python3 fonts-dejavu-core ca-certificates curl wget gnupg \
 && wget -qO- https://packages.adoptium.net/artifactory/api/gpg/key/public | gpg --dearmor > /usr/share/keyrings/adoptium.gpg \
 && echo "deb [signed-by=/usr/share/keyrings/adoptium.gpg] https://packages.adoptium.net/artifactory/deb bookworm main" > /etc/apt/sources.list.d/adoptium.list \
 && apt-get update && apt-get install -y --no-install-recommends temurin-25-jre temurin-21-jre \
 && rm -rf /var/lib/apt/lists/*
RUN mkdir -p /app/runtime/bin && curl -fsSL "https://github.com/bluenviron/mediamtx/releases/download/${MEDIAMTX_VERSION}/mediamtx_${MEDIAMTX_VERSION}_linux_amd64.tar.gz" | tar xz -C /app/runtime/bin mediamtx
WORKDIR /app
COPY --from=web /src/node_modules node_modules
COPY --from=web /src/package.json /src/tsconfig.json ./
COPY --from=web /src/packages packages
COPY --from=web /src/services/server-manager services/server-manager
COPY --from=web /src/services/streaming services/streaming
COPY --from=web /src/apps apps
COPY --from=rust /src/target/release/omniplayer-bot target/release/omniplayer-bot
COPY infra infra
COPY services/voicebox services/voicebox
ENV OMNI_DATA_DIR=/data OMNI_PORT=8080 NODE_ENV=production
VOLUME /data
EXPOSE 8080 8554 8888 8889 8189/udp 25565-25575
CMD ["node", "apps/api/src/main.ts"]
