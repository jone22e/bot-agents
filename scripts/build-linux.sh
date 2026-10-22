#!/usr/bin/env bash
# Gera o pacote .deb x86_64 em um ambiente Linux isolado e copia para o destino informado.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${1:-$ROOT/tmp/linux}"
command -v docker >/dev/null || { echo "Docker nao encontrado." >&2; exit 1; }
mkdir -p "$OUT"

docker build --platform linux/amd64 -f "$ROOT/scripts/Dockerfile.linux" -t dots-linux-build "$ROOT"
CONTAINER="$(docker create --platform linux/amd64 dots-linux-build)"
trap 'docker rm -f "$CONTAINER" >/dev/null 2>&1 || true' EXIT
docker cp "$CONTAINER:/artifacts/." "$OUT/"
echo "Pacote Linux em $OUT"
