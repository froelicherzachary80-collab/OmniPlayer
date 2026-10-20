#!/usr/bin/env bash
# Runs OmniPlayer on this computer and gives you a public HTTPS link you can open
# on your iPhone anywhere (Cloudflare quick tunnel; free, no account, no port forwarding).
#   ./scripts/share.sh            → https://<random>.trycloudflare.com
# The link changes every run. For a permanent address use your own domain (docs/HOSTING.md).
# Minecraft itself (port 25565) is NOT tunnelled: join from the same Wi-Fi or over Tailscale.
set -euo pipefail
cd "$(dirname "$0")/.."
[ -f .env ] && set -a && . ./.env && set +a
PORT="${OMNI_PORT:-8080}"

if [ -z "${OMNI_ADMIN_PASSWORD:-}" ]; then
  echo "Set OMNI_ADMIN_PASSWORD in .env first — this link is public on the internet." >&2
  exit 1
fi
if [ "${#OMNI_ADMIN_PASSWORD}" -lt 12 ]; then
  echo "OMNI_ADMIN_PASSWORD must be at least 12 characters for a public link." >&2
  exit 1
fi

mkdir -p runtime/bin
CF=runtime/bin/cloudflared
if ! command -v cloudflared >/dev/null && [ ! -x "$CF" ]; then
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) asset=cloudflared-linux-amd64 ;;
    Linux-aarch64) asset=cloudflared-linux-arm64 ;;
    Darwin-*) asset=cloudflared-darwin-amd64.tgz ;;
    *) echo "Install cloudflared manually: https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/" >&2; exit 1 ;;
  esac
  echo "Downloading cloudflared…"
  if [[ "$asset" == *.tgz ]]; then
    curl -fsSL "https://github.com/cloudflare/cloudflared/releases/latest/download/$asset" | tar xz -C runtime/bin
  else
    curl -fsSL -o "$CF" "https://github.com/cloudflare/cloudflared/releases/latest/download/$asset"
  fi
  chmod +x "$CF"
fi
command -v cloudflared >/dev/null && CF=cloudflared

./scripts/start.sh &
APP=$!
trap 'kill $APP 2>/dev/null; kill $TUN 2>/dev/null' EXIT
for _ in $(seq 1 60); do curl -fs "http://127.0.0.1:$PORT/api/health" >/dev/null && break; sleep 1; done

LOG=$(mktemp)
"$CF" tunnel --no-autoupdate --url "http://127.0.0.1:$PORT" >"$LOG" 2>&1 &
TUN=$!
for _ in $(seq 1 30); do
  URL=$(grep -o 'https://[a-z0-9-]*\.trycloudflare\.com' "$LOG" | head -1 || true)
  [ -n "$URL" ] && break
  sleep 1
done
LAN=$(hostname -I 2>/dev/null | awk '{print $1}' || ipconfig getifaddr en0 2>/dev/null || echo "this-computer")
echo
echo "  OmniPlayer is running"
echo "    On this Wi-Fi:   http://$LAN:$PORT"
if [ -n "${URL:-}" ]; then
  echo "    Anywhere:        $URL   (dashboard at /admin, MineLive at /live)"
else
  echo "    Public link failed — see $LOG"
fi
echo "    Minecraft:       $LAN:25565 (same Wi-Fi or Tailscale)"
echo "  Ctrl+C to stop."
wait $APP
