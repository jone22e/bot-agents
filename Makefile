# Publicação de versões do DOTS (ver RELEASE.md)
.PHONY: help update update-minor update-major release build-mac

help:
	@echo "make update         sobe a versão (0.4.0 -> 0.4.1) e publica"
	@echo "make update-minor   sobe a versão (0.4.3 -> 0.5.0) e publica"
	@echo "make update-major   sobe a versão (0.5.1 -> 1.0.0) e publica"
	@echo "make update VERSION=1.4.0   publica com um número escolhido"
	@echo "make release        publica a versão que já está nos arquivos"
	@echo "make build-mac      gera o instalador local, sem assinar nem publicar"

update:
	@scripts/release.sh $(or $(VERSION),patch)

update-minor:
	@scripts/release.sh minor

update-major:
	@scripts/release.sh major

release:
	@scripts/release.sh current

build-mac:
	@scripts/release.sh build
