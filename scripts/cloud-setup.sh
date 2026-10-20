#!/usr/bin/env bash
# One-paste setup for a fresh Ubuntu 22.04/24.04 cloud server, so OmniPlayer can be
# run entirely from an iPhone (no computer needed). Paste this whole file into
# your provider's "User data" / "Cloud-init" / "Startup script" box when you create
# the server (see docs/IPHONE_ONLY.md), or run it as root: bash cloud-setup.sh
#
# What it does:
#   1. installs Docker, adds swap so the first build doesn't run out of memory
#   2. downloads OmniPlayer from GitHub
#   3. writes .env with your admin password and a free HTTPS address
#      https://<your-ip>.sslip.io (Let's Encrypt via Caddy, no domain needed)
#   4. opens the firewall ports and starts everything; it restarts by itself after a reboot
#
# Put your own dashboard password here before pasting (12+ characters). Recommended:
# if you leave it empty a strong one is generated and printed to the server's
# console log (viewable in your provider's web console) and /root/omniplayer-password.txt.
ADMIN_PASSWORD=""
REPO="https://github.com/froelicherzachary80-collab/OmniPlayer.git"
BRANCH="claude/omniplayer-platform"
DIR=/opt/omniplayer

set -euo pipefail
touch /var/log/omniplayer-setup.log && chmod 600 /var/log/omniplayer-setup.log
exec > >(tee -a /var/log/omniplayer-setup.log) 2>&1
echo "== OmniPlayer cloud setup $(date -u) =="
export DEBIAN_FRONTEND=noninteractive

if ! command -v docker >/dev/null; then
  apt-get update -y
  apt-get install -y ca-certificates curl git
  curl -fsSL https://get.docker.com | sh
fi
apt-get install -y git curl >/dev/null

# The first build compiles Rust; give small (4 GB) servers some swap.
if [ ! -f /swapfile ] && [ "$(free -m | awk '/Swap/{print $2}')" -lt 2000 ]; then
  fallocate -l 4G /swapfile && chmod 600 /swapfile && mkswap /swapfile && swapon /swapfile
  echo '/swapfile none swap sw 0 0' >> /etc/fstab
fi

if [ -d "$DIR/.git" ]; then
  git -C "$DIR" fetch --depth 1 origin "$BRANCH" && git -C "$DIR" reset --hard FETCH_HEAD
else
  git clone --depth 1 --branch "$BRANCH" "$REPO" "$DIR"
fi
cd "$DIR"

IP="$(curl -4 -fsS https://api.ipify.org || curl -4 -fsS https://ifconfig.me)"
DOMAIN="${IP//./-}.sslip.io"
SHOW_PW=0
if [ ! -f .env ]; then
  if [ -z "$ADMIN_PASSWORD" ]; then
    ADMIN_PASSWORD="$(head -c 48 /dev/urandom | base64 | tr -dc 'A-Za-z0-9')"
    ADMIN_PASSWORD="${ADMIN_PASSWORD:0:20}"
    SHOW_PW=1
  fi
  if [ "${#ADMIN_PASSWORD}" -lt 12 ]; then
    echo "ADMIN_PASSWORD must be at least 12 characters" >&2
    exit 1
  fi
  cp .env.example .env
  sed -i "s|^OMNI_ADMIN_PASSWORD=.*|OMNI_ADMIN_PASSWORD=$ADMIN_PASSWORD|" .env
  sed -i "s|^OMNI_SECRET_KEY=.*|OMNI_SECRET_KEY=$(od -An -tx1 -N32 /dev/urandom | tr -d ' \n')|" .env
  sed -i "s|^OMNI_PUBLIC_HOST=.*|OMNI_PUBLIC_HOST=$IP|" .env
  sed -i "s|^OMNI_DOMAIN=.*|OMNI_DOMAIN=$DOMAIN|" .env
  chmod 600 .env
fi

if command -v ufw >/dev/null && ufw status | grep -q active; then
  ufw allow 22/tcp; ufw allow 80/tcp; ufw allow 443/tcp; ufw allow 8189/udp
  ufw allow 25565:25575/tcp; ufw allow 19132:19140/udp
fi

COMPOSE="docker compose -f infra/docker/docker-compose.yml -f infra/docker/docker-compose.domain.yml"
$COMPOSE up -d --build

if [ "$SHOW_PW" = 1 ]; then
  echo "OmniPlayer admin password: $ADMIN_PASSWORD" > /root/omniplayer-password.txt
  chmod 600 /root/omniplayer-password.txt
  # Shows up in the provider's web console / boot log, readable from an iPhone browser.
  echo "OmniPlayer admin password: $ADMIN_PASSWORD" > /dev/console 2>/dev/null || true
fi

echo
echo "================================================================"
echo " OmniPlayer is starting (the first build takes 10-20 minutes)."
echo " Open on your iPhone:  https://$DOMAIN"
[ "$SHOW_PW" = 1 ] && echo " Admin password:       $ADMIN_PASSWORD  (also in /root/omniplayer-password.txt)"
echo " Minecraft Java:       $IP   (port 25565)"
echo " Minecraft on iPhone:  $IP   port 19132 (after turning on iPhone players)"
echo " Setup log:            /var/log/omniplayer-setup.log"
echo "================================================================"
