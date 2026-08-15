#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CADDY_PID="$SCRIPT_DIR/.caddy.pid"
CF_PID="$SCRIPT_DIR/.cloudflared.pid"
CF_URL="$SCRIPT_DIR/.cloudflared.url"
LOG_CADDY="$SCRIPT_DIR/.caddy.log"
LOG_CF="$SCRIPT_DIR/.cloudflared.log"

# Bereits gestartet?
if [ -f "$CADDY_PID" ] && kill -0 "$(cat "$CADDY_PID")" 2>/dev/null; then
    echo "log_tailer läuft bereits"
    [ -f "$CF_URL" ] && echo "→ $(cat "$CF_URL")"
    exit 0
fi

# ── 1. Caddy (HTTP, nur localhost) ──────────────────────────────────────────
echo "Starte Caddy..."
caddy start \
    --config "$SCRIPT_DIR/Caddyfile" \
    --pidfile "$CADDY_PID" \
    >> "$LOG_CADDY" 2>&1
sleep 1

# ── 2. Cloudflare Tunnel (HTTPS nach außen) ──────────────────────────────────
echo "Starte Cloudflare Tunnel (holt HTTPS-URL)..."
> "$LOG_CF"   # Log leeren, damit alte URLs nicht verwirren
cloudflared tunnel --url http://127.0.0.1:8080 \
    --no-autoupdate \
    >> "$LOG_CF" 2>&1 &
echo $! > "$CF_PID"

# Auf URL warten (max 20 Sek.)
for i in $(seq 1 20); do
    URL=$(grep -oP 'https://[a-z0-9\-]+\.trycloudflare\.com' "$LOG_CF" 2>/dev/null | tail -1)
    [ -n "$URL" ] && break
    sleep 1
done

if [ -z "$URL" ]; then
    echo "⚠️  Cloudflare-URL nicht gefunden – prüfe $LOG_CF"
    URL="(unbekannt)"
fi

echo "$URL" > "$CF_URL"

echo ""
echo "✅ log_tailer gestartet"
echo ""
echo "  URL:  $URL"
echo ""
echo "  → App im Browser öffnen, dann über das Menü (⋮) oder"
echo "    das Installieren-Symbol in der Adressleiste als PWA installieren."
echo ""
echo "  Hinweis: Die URL ändert sich bei jedem Start."
echo "  Logs: $LOG_CF"
