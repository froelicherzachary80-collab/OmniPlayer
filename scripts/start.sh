#!/usr/bin/env bash
# Starts VoiceBox and the OmniPlayer API (which manages MediaMTX, bots and servers).
set -euo pipefail
cd "$(dirname "$0")/.."
[ -f .env ] && set -a && . ./.env && set +a
[ -x target/release/omniplayer-bot ] || cargo build --release -p omnibrain
[ -f apps/web/dist/index.html ] || npm run build:web
python3 services/voicebox/voicebox.py &
VB=$!
trap 'kill $VB 2>/dev/null' EXIT
node apps/api/src/main.ts
