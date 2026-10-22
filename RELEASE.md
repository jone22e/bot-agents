# Publicar uma versão do DOTS

O app se atualiza sozinho, como o Ovseer. Ele consulta os releases de `jone22e/bot-agents` no GitHub 10 segundos
depois de abrir, a cada 30 minutos e quando a janela volta ao primeiro plano (nesse caso, só se a última consulta
foi há mais de 10 minutos). Baixa a versão nova em segundo plano e mostra a barra "DOTS X está pronto para
instalar". A instalação acontece em "Reiniciar e atualizar" ou quando o usuário fecha o app. Na abertura seguinte
o app troca sozinho o serviço em segundo plano (`dotsd`) pela versão nova.

Publicar uma versão é rodar um comando:

```bash
make update
```

## Comandos

| Comando | O que faz |
|---|---|
| `make update` | Sobe a versão de correção (0.4.0 → 0.4.1) e publica macOS e Linux x86_64 |
| `make update-minor` | Sobe a versão de funcionalidade (0.4.3 → 0.5.0) e publica |
| `make update-major` | Sobe a versão principal (0.5.1 → 1.0.0) e publica |
| `make update VERSION=1.4.0` | Publica com o número informado |
| `make release` | Publica a versão que já está nos arquivos, sem subir o número |
| `make build-mac` | Gera o instalador local em `target/universal-apple-darwin/release/bundle/dmg/`, sem assinar e sem publicar |
| `make build-linux` | Gera o `.deb` x86_64 em `tmp/linux/` com Docker, sem publicar |

## O que o `make update` faz

1. Confere o ambiente: certificado, credencial de notarização, chave das atualizações, login no GitHub, branch
   `main` sem alterações pendentes e em dia com o servidor.
2. Sobe o número da versão em `Cargo.toml` (workspace), `apps/desktop/src-tauri/tauri.conf.json` e
   `apps/desktop/package.json`.
3. Roda os testes (`cargo test --workspace` e os testes do frontend).
4. Compila o app universal do macOS e o pacote `.deb` para Linux x86_64 em um contêiner Debian.
5. Assina o app macOS com o Developer ID e envia para a Apple notarizar. Gera o DMG e os pacotes de atualização
   (`.app.tar.gz` e `.deb`, assinados com a chave das atualizações), inclui as três plataformas no `latest.json`
   e sobe os quatro arquivos para um release em rascunho no GitHub.
6. Faz o commit `chore: versão X`, cria a tag `vX` e envia para o servidor.
7. Tira o release do rascunho. A partir daí os apps instalados enxergam a versão.

Se algo falhar antes do passo 6, o número da versão volta ao que era e nada fica visível para os usuários.
Os arquivos gerados ficam em `dist/release/<versão>/`.

## Antes de publicar

Faça o commit e o push do que vai entrar na versão. O comando se recusa a rodar com alterações pendentes.

## Preparar uma máquina nova (uma vez)

1. **Certificado e notarização.** São os mesmos do Ovseer (ver `RELEASE.md` do ovrgit): o certificado
   `Developer ID Application: C2S Business Ltda (3UG973ND38)` e a credencial `ovseer-notary` no keychain.
   Para usar outra credencial: `APPLE_KEYCHAIN_PROFILE=nome make update`.

2. **Chave das atualizações.** Os apps instalados só aceitam uma atualização assinada com esta chave (a parte
   pública está em `plugins.updater.pubkey` do `tauri.conf.json`). Ela fica em `~/.tauri/dots-updater.key`, fora
   do repositório. Para publicar de outra máquina, copie esse arquivo para o mesmo lugar.

   **Não perca esta chave.** Sem ela, os apps já instalados não aceitam mais nenhuma atualização e precisam ser
   reinstalados à mão com um DMG novo.

3. **GitHub.** `gh auth login` com uma conta que tenha permissão de escrita no repositório.

4. **Ferramentas.** Rust com o alvo Intel (`rustup target add x86_64-apple-darwin`), as ferramentas de linha de
   comando do Xcode (`xcode-select --install`, para o ajudante de ditado em Swift), Node (nvm) e Docker em execução.

O pacote Linux usa o Secret Service do ambiente gráfico para guardar credenciais. O ditado por voz continua
exclusivo do macOS.

## Conferir se a atualização chegou

Abra o app instalado em uma versão anterior. Em cerca de 10 segundos ele baixa a nova e aparece a barra "DOTS X
está pronto para instalar". Em Configurações → Geral → Atualização dá para ver a versão instalada e forçar a
checagem.

## Texto do release

O `make update` preenche o texto com os commits que entraram desde a versão anterior. Para reescrever:

```bash
gh release edit v0.4.1 -R jone22e/bot-agents --notes "Texto novo"
```

## Problemas comuns

| Sintoma | Causa |
|---|---|
| `Certificado "Developer ID Application…" não encontrado` | Falta o certificado Developer ID Application nesta máquina |
| `Notarização indisponível` | A credencial `ovseer-notary` não existe nesta máquina, o Mac está bloqueado ou a Apple tem um acordo pendente em developer.apple.com |
| `Chave que assina as atualizações não encontrada` | Falta copiar `~/.tauri/dots-updater.key` para esta máquina |
| O app não acha a versão nova | O release ainda está em rascunho, ou o número não é maior que o instalado |
| "Disponível só no app instalado" nas Configurações | O app está rodando em modo de desenvolvimento (`tauri dev`) |
| A notarização falha | Veja o motivo com `xcrun notarytool log <id> --keychain-profile ovseer-notary` |

## Onde está o código

| Arquivo | Papel |
|---|---|
| `apps/desktop/src-tauri/src/updater.rs` | Checagem, download e instalação |
| `apps/desktop/src/stores/updater.ts` | Estado na tela (barra e Configurações) |
| `scripts/release.sh` | Passos do `make update` |
| `scripts/build-sidecar.sh` | Serviço e ajudante de ditado (`universal` para o instalador) |
| `scripts/build-linux.sh` e `scripts/Dockerfile.linux` | Build isolado do pacote `.deb` x86_64 |
| `apps/desktop/src-tauri/tauri.conf.json` → `plugins.updater` | Chave pública e endereço do `latest.json` |
| `apps/desktop/src-tauri/Entitlements.plist` | Permissões do app assinado (microfone do ditado) |
