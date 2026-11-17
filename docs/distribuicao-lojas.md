# Guia de Distribuição em Lojas e Gerenciadores de Pacotes

Este guia descreve o procedimento para disponibilizar o **FastFile** nas principais lojas e gerenciadores de pacotes para Windows, macOS e Linux, garantindo integridade e rastreabilidade total.

---

## 0. Pipeline Unificada de Release e Rastreabilidade

O FastFile adota uma pipeline única de CI/CD via GitHub Actions (`.github/workflows/release.yml`) disparada por tags (`v*`) ou manual (`workflow_dispatch`). A pipeline é estruturada para eliminar divergências de hash e garantir que nenhum artefato quebrado seja publicado:

1. **Construção Multiplataforma:**
   - **Windows x64:** Compila `ff.exe`, `fastfile.exe`, `ff-ui.exe` e `ff-indexer.exe`. Monta o pacote ZIP contendo todos os binários, ícone oficial `fastfile.ico`, os scripts de instalação `scripts/install.ps1`, `scripts/uninstall.ps1` (e wrappers na raiz `install.ps1`/`uninstall.ps1`) e documentação de licença/README. Compila também o instalador Inno Setup `FastFile-Setup.exe`.
   - **Linux x64:** Compila os binários completos, empacotando com `scripts/install.sh`, atalho `.desktop`, ícone e documentação em `fastfile-linux-x86_64.tar.gz`.
   - **macOS (Apple Silicon & Intel):** Compila binários nativos e empacota em `tar.gz` com `scripts/install.sh`.
2. **Validação em Máquina Limpa (Smoke Test):**
   - Antes de liberar a publicação, jobs descartáveis (`test-clean-install-windows` e `test-clean-install-linux`) baixam os pacotes gerados em runners limpos, validam a presença de `scripts/install.ps1`, instalam os binários, executam consultas via CLI (`ff --version`, `fastfile find . --compact`) validando o esquema `ff/1` e executam o desinstalador (`uninstall.ps1`) confirmando a remoção limpa.
3. **Geração Única de Manifestos e Checksums:**
   - O utilitário `scripts/generate_release_manifests.py` calcula os hashes SHA-256 reais diretamente dos artefatos produzidos no mesmo commit.
   - Emite `SHA256SUMS.txt`.
   - Atualiza atomicamente o manifesto Scoop (`packaging/scoop/fastfile.json`), WinGet (`packaging/winget/fastfile.yaml`) e Homebrew (`packaging/homebrew/fastfile.rb`), eliminando qualquer marcador pendente (`PENDING_*`).
   - Disponibiliza todos os pacotes, instaladores e manifestos validados na release do GitHub.

---

## 1. Windows: WinGet (Windows Package Manager)

O WinGet é a loja oficial de linha de comando da Microsoft, pré-instalada em todas as versões do Windows 10 e 11. O catálogo alimenta também as pesquisas da interface da Microsoft Store.

### Procedimento com `wingetcreate` (Oficial da Microsoft)
1. Instale a ferramenta oficial no Windows:
   ```powershell
   winget install Microsoft.WingetCreate
   ```
2. Submeta a nova versão apontando para a release do GitHub:
   ```powershell
   wingetcreate submit https://github.com/fabiomotta0311/fastfile/releases/download/v0.1.0/FastFile-Setup.exe
   ```
3. A ferramenta baixará o executável, extrairá a arquitetura, calculará o hash SHA-256 e abrirá automaticamente um Pull Request contra o repositório [`microsoft/winget-pkgs`](https://github.com/microsoft/winget-pkgs).
4. Após aprovação pelo bot de validação da Microsoft (normalmente em poucas horas), qualquer usuário poderá instalar com:
   ```powershell
   winget install fastfile
   ```

---

## 2. Windows: Microsoft Store (Interface Gráfica)

1. Crie ou acerte sua conta de desenvolvedor individual no [Microsoft Partner Center](https://partner.microsoft.com/dashboard).
2. Acesse a aba **Apps e Jogos** > **Novo Produto** > **Aplicativo MSIX ou EXE**.
3. Reserve o nome do aplicativo: `FastFile`.
4. Faça o upload do instalador (`FastFile-Setup.exe`) ou gere o pacote `.msix` com a [MSIX Packaging Tool](https://learn.microsoft.com/windows/msix/packaging-tool/overview).
5. Preencha as informações da listagem:
   - Categoria: *Utilitários e Ferramentas / Ferramentas de Desenvolvedor*
   - Imagens: screenshots da tela de busca, diff lado a lado e treemap de disco
   - Link de privacidade: informe a URL do repositório (`https://github.com/fabiomotta0311/fastfile#privacidade`)
6. Submeta para a certificação do Windows App Store.

---

## 3. Linux: Snap Store (Ubuntu / Canonical)

A Snap Store é a loja oficial do Ubuntu e a mais adotada para empacotar ferramentas de desenvolvedor e utilitários em todas as distros Linux.

### Publicação Automatizada via GitHub
1. Acesse [snapcraft.io](https://snapcraft.io) e faça login com sua conta Ubuntu One.
2. Registre o nome do aplicativo:
   ```bash
   sudo snap install snapcraft --classic
   snapcraft login
   snapcraft register fastfile
   ```
3. Acesse o painel de controle web em [snapcraft.io/fastfile/builds](https://snapcraft.io) e conecte o repositório GitHub `fabiomotta0311/fastfile`.
4. O Snapcraft utilizará automaticamente o manifesto [`snap/snapcraft.yaml`](../snap/snapcraft.yaml) já configurado no repositório.
5. Cada nova tag ou push na branch `main` disparará um build na nuvem da Canonical e publicará automaticamente nos canais `edge` e `stable`.
6. Para instalar:
   ```bash
   sudo snap install fastfile --classic
   ```

---

## 4. Linux: Flathub (Flatpak)

O Flathub é o padrão de facto para distribuição de aplicativos desktop independentes de distribuição (GNOME Software e KDE Discover).

1. O manifesto Flatpak está pronto em [`packaging/flatpak/io.github.fabiomotta0311.fastfile.yaml`](../packaging/flatpak/io.github.fabiomotta0311.fastfile.yaml).
2. Faça um fork do repositório [`flathub/flathub`](https://github.com/flathub/flathub).
3. Abra uma issue/submissão de novo aplicativo seguindo o [Guia de Submissão do Flathub](https://docs.flathub.org/docs/for-app-authors/submission).
4. Uma vez aprovado, um repositório dedicado `flathub/io.github.fabiomotta0311.fastfile` é criado e compila as novas versões automaticamente.

---

## 5. Linux: Arch User Repository (AUR)

O repositório comunitário do Arch Linux e Manjaro.

1. Clone o repositório AUR com sua chave SSH do Arch:
   ```bash
   git clone ssh://aur@aur.archlinux.org/fastfile.git
   ```
2. Copie o arquivo [`packaging/aur/PKGBUILD`](../packaging/aur/PKGBUILD) para essa pasta.
3. Gere o `.SRCINFO`:
   ```bash
   makepkg --printsrcinfo > .SRCINFO
   ```
4. Faça commit e push para o AUR:
   ```bash
   git add PKGBUILD .SRCINFO
   git commit -m "chore: release 0.1.0"
   git push origin master
   ```
5. Usuários de Arch Linux e derivados poderão instalar diretamente com:
   ```bash
   yay -S fastfile
   ```

---

## 6. Rust Ecosystem: Crates.io

Para permitir que qualquer desenvolvedor com toolchain Rust instale instantaneamente com `cargo install`:

1. Gere um token de API em [crates.io/settings/tokens](https://crates.io/settings/tokens).
2. Execute o login local:
   ```bash
   cargo login
   ```
3. Publique as crates em ordem de dependência:
   ```bash
   cargo publish -p ff-index
   cargo publish -p ff-search
   cargo publish -p ff-size
   cargo publish -p ff-diff
   cargo publish -p ff-sym
   cargo publish -p ff-content
   cargo publish -p ff-indexer
   cargo publish -p ff-preview
   cargo publish -p ff-ui
   cargo publish -p ff-cli
   ```
4. A partir daí, basta executar:
   ```bash
   cargo install ff-cli
   ```
