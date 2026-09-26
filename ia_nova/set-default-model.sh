#!/usr/bin/env bash
set -euo pipefail

OPENWEBUI_URL="http://localhost:3010"
DEFAULT_MODEL="llama3.2:latest"
EMAIL="${OPENWEBUI_EMAIL:-}"
PASSWORD="${OPENWEBUI_PASSWORD:-}"

if [ -z "$EMAIL" ] || [ -z "$PASSWORD" ]; then
    echo "ℹ️  OPENWEBUI_EMAIL / OPENWEBUI_PASSWORD non définis, configuration automatique du modèle ignorée."
    exit 0
fi

for i in $(seq 1 15); do
    if curl -s -o /dev/null -w "%{http_code}" "$OPENWEBUI_URL/api/version" | grep -q "200"; then
        break
    fi
    sleep 2
done

TOKEN=$(curl -s -X POST "$OPENWEBUI_URL/api/v1/auths/signin" \
    -H "Content-Type: application/json" \
    -d "{\"email\":\"$EMAIL\",\"password\":\"$PASSWORD\"}" \
    | python3 -c "import sys,json; print(json.load(sys.stdin).get('token',''))" 2>/dev/null || true)

if [ -z "$TOKEN" ]; then
    echo "⚠️  Impossible de se connecter à Open WebUI. Modèle par défaut non configuré automatiquement."
    exit 0
fi

curl -s -X POST "$OPENWEBUI_URL/api/v1/configs/models" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d "{\"DEFAULT_MODELS\":\"$DEFAULT_MODEL\"}" > /dev/null

echo "✅ Modèle par défaut configuré sur $DEFAULT_MODEL."
