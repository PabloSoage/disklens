#!/bin/sh
# Install what DiskLens needs to build on Debian/Ubuntu (and derivatives).
#   sh scripts/setup.sh          build tools (+ smartmontools, optional fallback backend)
set -eu
SUDO=""
[ "$(id -u)" -ne 0 ] && SUDO="sudo"
if command -v apt-get >/dev/null 2>&1; then
    $SUDO apt-get update
    $SUDO apt-get install -y build-essential pkg-config curl smartmontools
elif command -v dnf >/dev/null 2>&1; then
    $SUDO dnf install -y gcc pkgconf-pkg-config curl smartmontools
else
    echo "Unknown package manager: install a C toolchain, pkg-config and (optionally) smartmontools." >&2
fi
if ! command -v cargo >/dev/null 2>&1; then
    echo "Rust is missing: https://rustup.rs (curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh)"
fi
