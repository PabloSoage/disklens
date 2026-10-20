#!/bin/sh
# Build DiskLens and install it system-wide.
#   sh scripts/install.sh            full build (terminal, GUI and web)
#   sh scripts/install.sh --headless terminal and web only, for servers without a display
#   sh scripts/install.sh --service  also enable the web interface as a systemd service
set -eu
cd "$(dirname "$0")/.."
PREFIX="${PREFIX:-/usr/local}"
FEATURES=""
SERVICE=0
for a in "$@"; do
    case "$a" in
        --headless) FEATURES="--no-default-features --features web" ;;
        --service) SERVICE=1 ;;
        *) echo "unknown option $a" >&2; exit 2 ;;
    esac
done
# shellcheck disable=SC2086
cargo build --release $FEATURES
SUDO=""
[ "$(id -u)" -ne 0 ] && SUDO="sudo"
BIN="$PREFIX/bin/disklens"
$SUDO install -Dm755 target/release/disklens "$BIN"
$SUDO install -Dm644 packaging/disklens.desktop "$PREFIX/share/applications/disklens.desktop"
# pkexec checks the executable path written in the policy.
sed "s#/usr/bin/disklens#$BIN#" packaging/io.github.pablosoage.disklens.policy |
    $SUDO tee /usr/share/polkit-1/actions/io.github.pablosoage.disklens.policy >/dev/null
if [ "$SERVICE" -eq 1 ]; then
    sed "s#/usr/bin/disklens#$BIN#" packaging/disklens-web.service |
        $SUDO tee /etc/systemd/system/disklens-web.service >/dev/null
    $SUDO systemctl daemon-reload
    $SUDO systemctl enable --now disklens-web
    echo "Web interface: http://127.0.0.1:8086"
fi
echo "Installed $BIN"
