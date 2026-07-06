#!/usr/bin/env bash
# Зупиняє й видаляє launchd LaunchAgent myllm-proxy-service (не видаляє
# ~/Library/Application Support/myllm-proxy-service/requests.jsonl — історія
# запитів лишається на диску, якщо треба її переглянути пізніше).
set -euo pipefail

LABEL="com.nitra.myllm-proxy-service"
PLIST_PATH="$HOME/Library/LaunchAgents/$LABEL.plist"

launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
rm -f "$PLIST_PATH"
echo "==> $LABEL зупинено, $PLIST_PATH видалено"
echo "==> бінарник і requests.jsonl у '~/Library/Application Support/myllm-proxy-service/' лишились — видали вручну за потреби"
