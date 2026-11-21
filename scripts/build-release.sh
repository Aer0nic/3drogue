#!/usr/bin/env bash
# Build standalone releases of GRAZZARK into dist/:
#   dist/grazzark-linux-x86_64.tar.gz    (always)
#   dist/grazzark-windows-x86_64.zip     (when the Windows target is available)
#
# Windows builds from Linux need:
#   rustup target add x86_64-pc-windows-gnu
#   sudo apt install mingw-w64            (provides x86_64-w64-mingw32-gcc)
# On Windows itself, just run `cargo build --release` instead.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
DIST=dist
rm -rf "$DIST"
mkdir -p "$DIST"

echo "==> Linux x86_64"
cargo build --release
LIN="$DIST/grazzark-$VERSION-linux-x86_64"
mkdir -p "$LIN"
cp target/release/grazzark "$LIN/"
cp packaging/grazzark.desktop packaging/install-linux.sh "$LIN/"
chmod +x "$LIN/grazzark" "$LIN/install-linux.sh"
cp README.md "$LIN/"
cp packaging/PLAY-LINUX.txt "$LIN/README-FIRST.txt"
tar -C "$DIST" -czf "$DIST/grazzark-linux-x86_64.tar.gz" "$(basename "$LIN")"
echo "    $DIST/grazzark-linux-x86_64.tar.gz"

if rustup target list --installed 2>/dev/null | grep -q x86_64-pc-windows-gnu && command -v x86_64-w64-mingw32-gcc >/dev/null; then
    echo "==> Windows x86_64"
    cargo build --release --target x86_64-pc-windows-gnu
    WIN="$DIST/grazzark-$VERSION-windows-x86_64"
    mkdir -p "$WIN"
    cp target/x86_64-pc-windows-gnu/release/grazzark.exe "$WIN/"
    cp README.md "$WIN/"
    # Windows Notepad wants CRLF line endings.
    sed 's/$/\r/' packaging/PLAY-WINDOWS.txt > "$WIN/README-FIRST.txt"
    (cd "$DIST" && rm -f grazzark-windows-x86_64.zip && \
        if command -v zip >/dev/null; then zip -qr grazzark-windows-x86_64.zip "$(basename "$WIN")"; \
        else python3 -m zipfile -c grazzark-windows-x86_64.zip "$(basename "$WIN")"; fi)
    echo "    $DIST/grazzark-windows-x86_64.zip"
else
    echo "==> Skipping Windows (install the x86_64-pc-windows-gnu target and mingw-w64 to build it)"
fi
