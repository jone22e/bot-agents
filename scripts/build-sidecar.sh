#!/usr/bin/env bash
# Compila o runtime `dotsd` e o coloca como sidecar do app Tauri
# (apps/desktop/src-tauri/binaries/dotsd-<target-triple>).
# Uso: scripts/build-sidecar.sh [debug|release]   (padrao: release)
set -euo pipefail
PROFILE="${1:-release}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
TRIPLE="$(rustc --print host-tuple)"
DEST="$ROOT/apps/desktop/src-tauri/binaries"
mkdir -p "$DEST"
if [ "$PROFILE" = "release" ]; then
  cargo build -p dotsd --release
  SRC="$ROOT/target/release/dotsd"
else
  cargo build -p dotsd
  SRC="$ROOT/target/debug/dotsd"
fi
cp "$SRC" "$DEST/dotsd-$TRIPLE"
chmod +x "$DEST/dotsd-$TRIPLE"
# Assinatura ad-hoc: evita "Killed: 9" em arm64 para binarios copiados.
codesign --force --sign - "$DEST/dotsd-$TRIPLE" >/dev/null 2>&1 || true
echo "sidecar: $DEST/dotsd-$TRIPLE ($("$DEST/dotsd-$TRIPLE" --version))"
