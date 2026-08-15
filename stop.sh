#!/usr/bin/env bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CADDY_PID="$SCRIPT_DIR/.caddy.pid"
CF_PID="$SCRIPT_DIR/.cloudflared.pid"
CF_URL="$SCRIPT_DIR/.cloudflared.url"

# Cloudflare Tunnel stoppen
if [ -f "$CF_PID" ]; then
    PID=$(cat "$CF_PID")
    kill "$PID" 2>/dev/null || true
    rm -f "$CF_PID" "$CF_URL"
    echo "Cloudflare Tunnel gestoppt"
fi

# Caddy stoppen
if [ -f "$CADDY_PID" ]; then
    caddy stop --config "$SCRIPT_DIR/Caddyfile" 2>/dev/null || \
        kill "$(cat "$CADDY_PID")" 2>/dev/null || true
    rm -f "$CADDY_PID"
    echo "Caddy gestoppt"
fi

echo "✅ log_tailer gestoppt – nicht mehr erreichbar"
