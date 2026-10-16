#!/usr/bin/env bash
# Gera e publica uma versão do DOTS para macOS (ver RELEASE.md). Mesmo fluxo do ovrgit (scripts/release.sh).
#   scripts/release.sh patch|minor|major|x.y.z   sobe a versão e publica
#   scripts/release.sh current                   publica a versão que já está nos arquivos
#   scripts/release.sh build                     só gera o instalador local (sem assinar, sem publicar)
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"

BUMP="${1:-patch}"
REPO="jone22e/bot-agents"
PROFILE="${APPLE_KEYCHAIN_PROFILE:-ovseer-notary}"
IDENTITY="${APPLE_SIGNING_IDENTITY:-Developer ID Application: C2S Business Ltda (3UG973ND38)}"
UPDATER_KEY="${TAURI_UPDATER_KEY:-$HOME/.tauri/dots-updater.key}"
TARGET="universal-apple-darwin"
VERSION_FILES=(Cargo.toml Cargo.lock apps/desktop/src-tauri/tauri.conf.json apps/desktop/package.json)

# node (nvm), cargo e gh nem sempre estão no PATH do shell
NVM_BIN="$(ls -d "$HOME"/.nvm/versions/node/*/bin 2>/dev/null | tail -1 || true)"
export PATH="$HOME/.cargo/bin:${NVM_BIN:+$NVM_BIN:}/opt/homebrew/bin:/usr/local/bin:$PATH"

fail() { echo "✗ $1" >&2; exit 1; }
step() { echo; echo "▸ $1"; }
version() { python3 -c 'import json; print(json.load(open("apps/desktop/src-tauri/tauri.conf.json"))["version"])'; }

# Grava a versão nos três lugares (Cargo do workspace, Tauri e package.json); o runtime embutido muda de versão
# junto, e é isso que faz o app trocar o serviço em segundo plano depois de atualizar.
set_version() {
  python3 - "$1" <<'EOF'
import json, re, sys
cur = json.load(open("apps/desktop/src-tauri/tauri.conf.json"))["version"]
arg = sys.argv[1]
ma, mi, pa = (int(x) for x in cur.split("-")[0].split("."))
new = {"patch": f"{ma}.{mi}.{pa + 1}", "minor": f"{ma}.{mi + 1}.0", "major": f"{ma + 1}.0.0"}.get(arg, arg)
if not re.fullmatch(r"\d+\.\d+\.\d+", new):
    sys.exit(f"versão inválida: {arg}")
for path in ("apps/desktop/src-tauri/tauri.conf.json", "apps/desktop/package.json"):
    text = open(path).read()
    text = re.sub(r'("version":\s*")[^"]+(")', lambda m: m.group(1) + new + m.group(2), text, count=1)
    open(path, "w").write(text)
cargo = open("Cargo.toml").read()
cargo = re.sub(r'(\[workspace\.package\][^\[]*?\nversion\s*=\s*")[^"]+(")', lambda m: m.group(1) + new + m.group(2), cargo, count=1)
open("Cargo.toml", "w").write(cargo)
EOF
}

build_app() { # $1: identidade de assinatura ("-" = ad-hoc); $2: pacotes do Tauri
  step "Compilando o serviço e o ajudante de ditado (arm64 + x86_64)"
  scripts/build-sidecar.sh universal
  step "Compilando o app"
  (cd apps/desktop && APPLE_SIGNING_IDENTITY="$1" ./node_modules/.bin/tauri build --target "$TARGET" --bundles "$2" \
    --config '{"build":{"beforeBuildCommand":"npx vite build"}}')
}

notarize() { # $1: arquivo (.zip ou .dmg)
  local out
  out="$(xcrun notarytool submit "$1" --keychain-profile "$PROFILE" --wait --output-format json 2>&1)" \
    || fail "Falha ao enviar para a notarização: $out"
  echo "$out" | python3 -c 'import json,sys; sys.exit(0 if json.load(sys.stdin).get("status") == "Accepted" else 1)' \
    || fail "A Apple recusou a notarização: $out
Veja o motivo com: xcrun notarytool log <id> --keychain-profile $PROFILE"
}

BUNDLE="$ROOT/target/$TARGET/release/bundle"

if [ "$BUMP" = "build" ]; then
  [ "$(uname)" = "Darwin" ] || fail "O instalador do macOS só pode ser gerado em um Mac."
  build_app "-" "app,dmg"   # "-": assinatura ad-hoc (roda neste Mac; para outros Macs use o make update)
  echo
  echo "✓ Instalador local (assinatura ad-hoc, sem notarizar): $(ls "$BUNDLE"/dmg/*.dmg)"
  exit 0
fi

step "Conferindo o ambiente"
[ "$(uname)" = "Darwin" ] || fail "A versão do macOS só pode ser gerada em um Mac."
command -v gh >/dev/null || fail "GitHub CLI não encontrado (brew install gh)."
gh auth status >/dev/null 2>&1 || fail "GitHub CLI sem login (gh auth login)."
command -v node >/dev/null || fail "Node não encontrado (nvm)."
rustup target list --installed | grep -q x86_64-apple-darwin || fail "Falta o alvo Intel do Rust (rustup target add x86_64-apple-darwin)."
security find-identity -v -p codesigning | grep -qF "$IDENTITY" || fail "Certificado \"$IDENTITY\" não encontrado no keychain."
NOTARY_CHECK="$(xcrun notarytool history --keychain-profile "$PROFILE" 2>&1 >/dev/null)" \
  || fail "Notarização indisponível (credencial \"$PROFILE\", Mac bloqueado ou acordo pendente na Apple; ver RELEASE.md):
$NOTARY_CHECK"
[ -f "$UPDATER_KEY" ] || fail "Chave que assina as atualizações não encontrada em $UPDATER_KEY (ver RELEASE.md)."
[ "$(git branch --show-current)" = "main" ] || fail "Publique a partir da branch main."
[ -z "$(git status --porcelain)" ] || fail "Há alterações não commitadas. Faça o commit antes de publicar."
if git ls-remote --exit-code --heads origin main >/dev/null 2>&1; then
  git fetch --quiet origin main
  [ -z "$(git rev-list HEAD..origin/main)" ] || fail "A main local está atrás do servidor. Faça o pull antes."
fi

# se algo falhar antes do commit da versão, o número volta ao que era
BUMPED=0
revert_version() { local st=$?; if [ "$st" -ne 0 ] && [ "$BUMPED" = 1 ]; then git checkout --quiet -- "${VERSION_FILES[@]}"; echo "Versão revertida." >&2; fi; }
trap revert_version EXIT
if [ "$BUMP" != "current" ]; then
  set_version "$BUMP"
  BUMPED=1
fi
VERSION="$(version)"
TAG="v$VERSION"
if gh release view "$TAG" -R "$REPO" --json isDraft -q .isDraft 2>/dev/null | grep -q false; then
  fail "A versão $VERSION já foi publicada."
fi
echo "Versão: $VERSION"

# texto do release: o que entrou desde a versão anterior (dá para editar depois no GitHub)
PREV="$(git describe --tags --abbrev=0 2>/dev/null || true)"
NOTES="$(mktemp)"
git log --no-merges --format='- %s' ${PREV:+"$PREV"..}HEAD | grep -v '^- chore: versão' > "$NOTES" || true
[ -s "$NOTES" ] || echo "- Melhorias e correções" > "$NOTES"

step "Testes"
cargo test --workspace
(cd apps/desktop && ./node_modules/.bin/vitest run)

build_app "$IDENTITY" "app"
APP="$BUNDLE/macos/DOTS.app"
OUT="$ROOT/dist/release/$VERSION"
rm -rf "$OUT" && mkdir -p "$OUT"
DMG="DOTS_${VERSION}_universal.dmg"
TGZ="DOTS_${VERSION}_universal.app.tar.gz"

step "Notarizando o app (leva alguns minutos)"
ditto -c -k --keepParent "$APP" "$OUT/DOTS.zip"
notarize "$OUT/DOTS.zip"
xcrun stapler staple -q "$APP"
rm "$OUT/DOTS.zip"

step "Gerando o instalador (DMG)"
STAGE="$(mktemp -d)"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -quiet -volname "DOTS" -srcfolder "$STAGE" -ov -format UDZO "$OUT/$DMG"
rm -rf "$STAGE"
codesign --force --sign "$IDENTITY" --timestamp "$OUT/$DMG"
notarize "$OUT/$DMG"
xcrun stapler staple -q "$OUT/$DMG"

step "Pacote da atualização automática"
COPYFILE_DISABLE=1 tar -czf "$OUT/$TGZ" -C "$(dirname "$APP")" DOTS.app
(cd apps/desktop && ./node_modules/.bin/tauri signer sign -f "$UPDATER_KEY" -p "" --app-version "$VERSION" "$OUT/$TGZ" >/dev/null)
python3 - "$VERSION" "$OUT/$TGZ.sig" "https://github.com/$REPO/releases/download/$TAG/$TGZ" "$NOTES" "$OUT/latest.json" <<'EOF'
import datetime, json, sys
version, sig, url, notes, dest = sys.argv[1:]
entry = {"signature": open(sig).read().strip(), "url": url}
json.dump({
    "version": version,
    "notes": open(notes).read().strip(),
    "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    # o mesmo pacote universal serve aos dois tipos de Mac
    "platforms": {"darwin-aarch64": entry, "darwin-x86_64": entry},
}, open(dest, "w"), indent=2, ensure_ascii=False)
EOF

step "Enviando para o GitHub (rascunho)"
gh release view "$TAG" -R "$REPO" >/dev/null 2>&1 \
  || gh release create "$TAG" -R "$REPO" --draft --title "$VERSION" --notes-file "$NOTES" >/dev/null
gh release upload "$TAG" -R "$REPO" --clobber "$OUT/$DMG" "$OUT/$TGZ" "$OUT/latest.json"

step "Gravando a versão no Git"
if [ "$BUMP" != "current" ]; then
  git commit --quiet -m "chore: versão $VERSION" -- "${VERSION_FILES[@]}"
  BUMPED=0
fi
git tag "$TAG"
git push --quiet origin main "$TAG"

step "Liberando para os usuários"
gh release edit "$TAG" -R "$REPO" --draft=false --latest --notes-file "$NOTES" >/dev/null

echo
echo "✓ DOTS $VERSION publicado: https://github.com/$REPO/releases/tag/$TAG"
echo "  Instalador: $OUT/$DMG"
