#!/usr/bin/env bash
set -euo pipefail

APP_DIR="/root/soulhub/SoulBook"
SERVICE_NAME="soulbook"
HEALTH_URL="${SOULBOOK_HEALTH_URL:-http://127.0.0.1:3004/agent/v1/system/health}"
LOCK_FILE="/tmp/soulbook-backend-deploy.lock"
LOG_FILE="/tmp/soulbook-backend-deploy.log"

exec > >(tee -a "$LOG_FILE") 2>&1
exec 9>"$LOCK_FILE"

if ! flock -n 9; then
  echo "[$(date -Is)] another backend deployment is already running"
  exit 0
fi

echo "[$(date -Is)] backend deployment started"

cd "$APP_DIR"

OLD_REV="$(git rev-parse --short HEAD)"
OLD_BIN="$APP_DIR/target/release/soulbook"
BACKUP_BIN="$APP_DIR/target/release/soulbook.bak.$(date +%Y%m%d%H%M%S)"

git fetch origin main
git merge --ff-only origin/main
NEW_REV="$(git rev-parse --short HEAD)"

if [ -x "$OLD_BIN" ]; then
  cp "$OLD_BIN" "$BACKUP_BIN"
  chmod +x "$BACKUP_BIN"
  echo "[$(date -Is)] backed up binary to $BACKUP_BIN"
fi

CC="${CC:-clang}" CXX="${CXX:-clang++}" cargo build --release
chmod +x "$OLD_BIN"

systemctl restart "$SERVICE_NAME"
sleep 3

if ! systemctl is-active --quiet "$SERVICE_NAME"; then
  echo "[$(date -Is)] service is not active after restart"
  if [ -x "$BACKUP_BIN" ]; then
    cp "$BACKUP_BIN" "$OLD_BIN"
    chmod +x "$OLD_BIN"
    systemctl restart "$SERVICE_NAME"
  fi
  exit 1
fi

if ! curl -fsS --max-time 15 "$HEALTH_URL" >/dev/null; then
  echo "[$(date -Is)] health check failed: $HEALTH_URL"
  if [ -x "$BACKUP_BIN" ]; then
    cp "$BACKUP_BIN" "$OLD_BIN"
    chmod +x "$OLD_BIN"
    systemctl restart "$SERVICE_NAME"
  fi
  exit 1
fi

echo "[$(date -Is)] backend deployment succeeded: $OLD_REV -> $NEW_REV"
