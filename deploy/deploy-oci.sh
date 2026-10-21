#!/usr/bin/env bash
set -euo pipefail

APP_DIR="${APP_DIR:-/home/ubuntu/my_english}"
DOMAIN="english-note.duckdns.org"

echo "==> Building release binary in ${APP_DIR}..."
cd "${APP_DIR}"
cargo build --release

echo "==> Installing systemd unit (my-english.service)..."
sudo cp deploy/my-english.service /etc/systemd/system/my-english.service
sudo systemctl daemon-reload
sudo systemctl enable --now my-english
sudo systemctl restart my-english

if command -v nginx >/dev/null 2>&1; then
    echo "==> Updating Nginx reverse proxy config for ${DOMAIN}..."
    sudo cp deploy/nginx-english-note.conf "/etc/nginx/sites-available/${DOMAIN}"
    sudo ln -sf "/etc/nginx/sites-available/${DOMAIN}" "/etc/nginx/sites-enabled/${DOMAIN}"
    sudo nginx -t
    sudo systemctl reload nginx
fi

echo "==> Checking health endpoint..."
curl -fsS http://127.0.0.1:3000/health
echo ""
echo "==> Deployment complete for https://${DOMAIN}"
