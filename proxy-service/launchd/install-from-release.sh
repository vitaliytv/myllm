#!/usr/bin/env bash
# Завантажує myllm-proxy-service з GitHub Release (CI-збірка, universal-apple-darwin)
# і реєструє як launchd LaunchAgent — без локального Rust toolchain (на відміну від
# install.sh, який білдить cargo build --release із сорців). Використання:
#   ./install-from-release.sh [tag]   # tag за замовч. — latest
set -euo pipefail

LABEL="com.nitra.myllm-proxy-service"
INSTALL_DIR="$HOME/Library/Application Support/myllm-proxy-service/bin"
PLIST_DIR="$HOME/Library/LaunchAgents"
PLIST_PATH="$PLIST_DIR/$LABEL.plist"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="vitaliytv/myllm"
TAG="${1:-latest}"
ASSET="myllm-proxy-service-universal-apple-darwin.tar.gz"

command -v gh >/dev/null || {
  echo "error: потрібен GitHub CLI (gh) — brew install gh" >&2
  exit 1
}

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "==> gh release download $TAG --repo $REPO --pattern $ASSET"
if [ "$TAG" = "latest" ]; then
  gh release download --repo "$REPO" --pattern "$ASSET" --dir "$TMP_DIR" --clobber
else
  gh release download "$TAG" --repo "$REPO" --pattern "$ASSET" --dir "$TMP_DIR" --clobber
fi

tar -xzf "$TMP_DIR/$ASSET" -C "$TMP_DIR"
BUILT_BIN="$TMP_DIR/myllm-proxy-service"
chmod +x "$BUILT_BIN"

mkdir -p "$INSTALL_DIR" "$PLIST_DIR"
cp "$BUILT_BIN" "$INSTALL_DIR/myllm-proxy-service"
echo "==> installed $INSTALL_DIR/myllm-proxy-service"

sed "s#__MYLLM_PROXY_SERVICE_BIN__#$INSTALL_DIR/myllm-proxy-service#" \
  "$SCRIPT_DIR/$LABEL.plist" >"$PLIST_PATH"
echo "==> wrote $PLIST_PATH"

# bootout — best-effort, не падає, якщо сервіс ще не зареєстрований.
launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$PLIST_PATH"
launchctl enable "gui/$(id -u)/$LABEL"
launchctl kickstart -k "gui/$(id -u)/$LABEL"

echo "==> $LABEL запущено (launchctl print gui/$(id -u)/$LABEL — перевірити стан)"
echo "==> лог: /tmp/myllm-proxy-service.log"
echo
echo "Бінарник із CI НЕ підписаний/нотаризований. Якщо launchd/Gatekeeper відмовляється"
echo "його запускати (сервіс у KeepAlive-циклі краш-рестарту), зніми карантин вручну:"
echo "  xattr -d com.apple.quarantine \"$INSTALL_DIR/myllm-proxy-service\""
