#!/usr/bin/env bash
# Збирає myllm-proxy-service (release) і реєструє його як launchd LaunchAgent —
# піднімається при вході в систему, автоперезапускається при краші, працює
# без відкритого вікна myllm.
set -euo pipefail

LABEL="com.nitra.myllm-proxy-service"
INSTALL_DIR="$HOME/Library/Application Support/myllm-proxy-service/bin"
PLIST_DIR="$HOME/Library/LaunchAgents"
PLIST_PATH="$PLIST_DIR/$LABEL.plist"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

echo "==> cargo build --release -p myllm-proxy-service"
(cd "$REPO_ROOT" && cargo build --release -p myllm-proxy-service)

BUILT_BIN="$REPO_ROOT/target/release/myllm-proxy-service"
if [ ! -x "$BUILT_BIN" ]; then
  echo "error: не знайдено зібраний бінарник $BUILT_BIN" >&2
  exit 1
fi

mkdir -p "$INSTALL_DIR" "$PLIST_DIR"
cp "$BUILT_BIN" "$INSTALL_DIR/myllm-proxy-service"
echo "==> installed $INSTALL_DIR/myllm-proxy-service"

sed "s#__MYLLM_PROXY_SERVICE_BIN__#$INSTALL_DIR/myllm-proxy-service#" \
  "$SCRIPT_DIR/$LABEL.plist" > "$PLIST_PATH"
echo "==> wrote $PLIST_PATH"

# bootout — best-effort, не падає, якщо сервіс ще не зареєстрований.
launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$PLIST_PATH"
launchctl enable "gui/$(id -u)/$LABEL"
launchctl kickstart -k "gui/$(id -u)/$LABEL"

echo "==> $LABEL запущено (launchctl print gui/$(id -u)/$LABEL — перевірити стан)"
echo "==> лог: /tmp/myllm-proxy-service.log"
