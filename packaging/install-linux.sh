#!/usr/bin/env bash
# Install GRAZZARK for the current user: the game goes in ~/.local/bin and a
# menu entry (which opens a terminal for it) in ~/.local/share/applications.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"
install -m 755 "$here/grazzark" "$HOME/.local/bin/grazzark"
sed "s|^Exec=.*|Exec=$HOME/.local/bin/grazzark|" "$here/grazzark.desktop" > "$HOME/.local/share/applications/grazzark.desktop"
echo "Installed. Run 'grazzark' from a terminal, or find GRAZZARK in your applications menu."
