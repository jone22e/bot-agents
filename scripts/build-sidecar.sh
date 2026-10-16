#!/usr/bin/env bash
# Compila o runtime `dotsd` e o coloca como sidecar do app Tauri
# (apps/desktop/src-tauri/binaries/dotsd-<target-triple>).
# Uso: scripts/build-sidecar.sh [debug|release|universal]   (padrao: release)
#   universal: arm64 + x86_64 num binario so (dotsd-universal-apple-darwin), para o instalador publicado.
set -euo pipefail
PROFILE="${1:-release}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
DEST="$ROOT/apps/desktop/src-tauri/binaries"
mkdir -p "$DEST"
if [ "$PROFILE" = "universal" ]; then
  TRIPLE="universal-apple-darwin"
  cargo build -p dotsd --release --target aarch64-apple-darwin
  cargo build -p dotsd --release --target x86_64-apple-darwin
  lipo -create -output "$DEST/dotsd-$TRIPLE" "$ROOT/target/aarch64-apple-darwin/release/dotsd" "$ROOT/target/x86_64-apple-darwin/release/dotsd"
  # o Tauri compila cada arquitetura em separado e exige o sidecar com o nome de cada uma
  for arch in aarch64 x86_64; do
    cp "$ROOT/target/$arch-apple-darwin/release/dotsd" "$DEST/dotsd-$arch-apple-darwin"
    codesign --force --sign - "$DEST/dotsd-$arch-apple-darwin" >/dev/null 2>&1 || true
  done
else
  TRIPLE="$(rustc --print host-tuple)"
  if [ "$PROFILE" = "release" ]; then
    cargo build -p dotsd --release
    SRC="$ROOT/target/release/dotsd"
  else
    cargo build -p dotsd
    SRC="$ROOT/target/debug/dotsd"
  fi
  cp "$SRC" "$DEST/dotsd-$TRIPLE"
fi
chmod +x "$DEST/dotsd-$TRIPLE"
# Assinatura ad-hoc: evita "Killed: 9" em arm64 para binarios copiados (o build publicado assina de novo com o Developer ID).
codesign --force --sign - "$DEST/dotsd-$TRIPLE" >/dev/null 2>&1 || true
echo "sidecar: $DEST/dotsd-$TRIPLE ($("$DEST/dotsd-$TRIPLE" --version))"

# Ajudante de ditado (macOS): transcreve o audio do compositor neste Mac.
if [ "$(uname)" = "Darwin" ]; then
  if [ "$PROFILE" = "universal" ]; then
    (cd "$ROOT/native/macos/Transcribe" && swift build -c release --arch arm64 --arch x86_64 >/dev/null)
    cp "$ROOT/native/macos/Transcribe/.build/apple/Products/Release/dots-transcribe" "$DEST/dots-transcribe-$TRIPLE"
    for arch in aarch64 x86_64; do
      cp "$DEST/dots-transcribe-$TRIPLE" "$DEST/dots-transcribe-$arch-apple-darwin"
      chmod +x "$DEST/dots-transcribe-$arch-apple-darwin"
    done
  else
    (cd "$ROOT/native/macos/Transcribe" && swift build -c release >/dev/null)
    cp "$ROOT/native/macos/Transcribe/.build/release/dots-transcribe" "$DEST/dots-transcribe-$TRIPLE"
  fi
  chmod +x "$DEST/dots-transcribe-$TRIPLE"
  codesign --force --sign - "$DEST/dots-transcribe-$TRIPLE" >/dev/null 2>&1 || true
  echo "sidecar: $DEST/dots-transcribe-$TRIPLE"
fi
