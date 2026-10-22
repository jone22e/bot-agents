# Instalação do DOTS no Ubuntu 24.04+ / Debian 12+ (x86_64)

Baixe o arquivo `DOTS_<versão>_amd64.deb` do release e instale com:

```bash
sudo apt install ./DOTS_*_amd64.deb
```

Abra **DOTS** pelo menu de aplicativos. O app copia o serviço `dotsd` para o diretório de dados do usuário e o
inicia quando abre. No Linux, o serviço não é registrado para iniciar automaticamente ao entrar na sessão.

O ditado por voz é exclusivo do macOS. Para guardar credenciais de conexão, o ambiente gráfico precisa oferecer
um serviço Secret Service (por exemplo, GNOME Keyring).

As versões seguintes são distribuídas pelo mesmo release do macOS. O atualizador do app usa o pacote `.deb` e
pode pedir autenticação do sistema para instalá-lo.
