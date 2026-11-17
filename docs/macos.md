# FastFile no macOS (Apple Silicon & Intel)

O FastFile roda nativamente no macOS para arquiteturas **Apple Silicon (M1/M2/M3/M4, `aarch64-apple-darwin`)** e **Intel (`x86_64-apple-darwin`)**.

Ele oferece paridade completa com as versões Windows e Linux:
1. **Interface Gráfica Desktop (`ff-ui`)**: Construída em `egui` e renderizada nativamente com Metal/Cocoa, com atalhos padrão Mac (`Cmd+F`, seleção com `Cmd+clique`).
2. **CLI de Alta Performance (`fastfile` e `ff`)**: Consultas em milissegundos formatadas para humanos e agentes de IA.
3. **Servidor MCP Integrado (`fastfile mcp`)**: Compatível nativamente com Claude Code, Cursor, Antigravity, Windsurf e Hermes no macOS.
4. **Localização Nativa**: Detecta automaticamente o idioma padrão do macOS através das variáveis de ambiente (`$LANG` / `$LC_ALL`).

---

## 1. Instalação no macOS

### Opção A: Via Homebrew (Recomendado)

```bash
# Adicionar o repositório tap ou baixar a fórmula
brew install fabiomotta0311/fastfile/fastfile
```

### Opção B: Download do Pacote Pré-compilado

Baixe o pacote correspondente à sua arquitetura na [página de Releases](https://github.com/fabiomotta0311/fastfile/releases):

- **Apple Silicon (M1/M2/M3/M4):** `fastfile-macos-arm64.tar.gz`
- **Intel Mac:** `fastfile-macos-x86_64.tar.gz`

Extraia e mova para o seu diretório de binários:

```bash
tar -xzf fastfile-macos-arm64.tar.gz
sudo mv fastfile ff ff-ui /usr/local/bin/
```

### Opção C: Compilação a partir do Código-Fonte

```bash
# Clone e entre no repositório
git clone https://github.com/fabiomotta0311/fastfile.git
cd fastfile

# Execute o script de instalação automática
./scripts/install.sh
```

---

## 2. Utilização no macOS

### Interface Gráfica
Para abrir a janela do FastFile no macOS:
```bash
fastfile --ui
# ou
ff-ui
```
- Use **`Cmd + F`** para focar a barra de busca.
- Selecione arquivos com **`Cmd + clique`** para abrir o comparador Diff Lado a Lado.

### Terminal & Scripts
```bash
# Buscar arquivos no seu disco
ff find "*.rs"
ff find "*.pdf" --type doc

# Análise de espaço em disco no diretório Home
ff du ~ --top 10

# Busca ultrarrápida de conteúdo
ff grep "fn main" --type rust

# Encontrar definição de símbolos via Tree-sitter
ff sym AuthenticatedUser
```

---

## 3. Servidor MCP para Agentes de IA no macOS

Para integrar com **Claude Code**, **Cursor**, **Antigravity** ou qualquer harness de agentes no macOS, adicione à sua configuração de MCP (`~/Library/Application Support/...` ou `~/.config/...`):

```json
{
  "mcpServers": {
    "fastfile": {
      "command": "/usr/local/bin/fastfile",
      "args": ["mcp"]
    }
  }
}
```

---

## 4. Serviço em Segundo Plano via Launchd (LaunchAgent)

No macOS, o indexador residente pode ser gerenciado nativamente pelo **launchd**:

1. Gere o arquivo de configuração do LaunchAgent:
   ```bash
   ff-indexer --install
   ```
2. Salve o arquivo em `~/Library/LaunchAgents/io.github.fabiomotta0311.fastfile.plist`
3. Ative o serviço:
   ```bash
   launchctl load ~/Library/LaunchAgents/io.github.fabiomotta0311.fastfile.plist
   ```
O serviço iniciará automaticamente no login do usuário e manterá a comunicação IPC via Unix Domain Socket em `/tmp/fastfile.sock`.
