# Uso Remoto e MCP sobre SSH (`docs/remoto.md`)

Guia de operação e integração do FastFile em servidores Linux remotos (VPS, servidores de desenvolvimento e nuvem)
conforme especificado em `docs/ARQUITETURA.md` §3.3, §3.4 e na Issue #23.

---

## 1. Princípios: Sem Rede Dentro do Binário

Seguindo a política estrita de privacidade e segurança do projeto (`AGENTS.md` e `docs/ARQUITETURA.md` §6):

- **O FastFile não implementa clientes SSH, sockets de rede, telemetria nem HTTP.**
- O acesso a máquinas remotas é realizado por **composição pura com o SSH já instalado no sistema operacional**.
- No Linux, como não há MFT nem USN Journal do NTFS, o FastFile opera através da **varredura paralela otimizada**
  (`WalkSource` com `ignore`), preservando exatamente o mesmo contrato `ff/1`, a proteção de segredos e os limites de tokens.

---

## 2. Nomes de Comando: `fastfile` e `ff` (§3.3)

Em servidores remotos e configurações de agentes, use sempre o binário **`fastfile`**:
- Evita conflito com aliases comuns (`ff` para Firefox, ffmpeg, fastfetch).
- O instalador (`scripts/install.sh`) e o `cargo install` criam simultaneamente:
  - `/usr/local/bin/fastfile` (para agentes, scripts e MCP)
  - `/usr/local/bin/ff` (para uso interativo humano)

---

## 3. Uso do CLI via SSH

Qualquer comando FastFile pode ser disparado remotamente via SSH, devolvendo o JSON estruturado diretamente no stdout:

### 3.1 Análise de Uso de Disco (`du`)
```bash
ssh vps-hermes fastfile du /etc --top 5 --json
```

Exemplo real de resposta:
```json
{
  "schema": "ff/1",
  "source": "walk",
  "command": "du",
  "summary": {
    "elapsed_ms": 264.68,
    "secrets_hidden": 0,
    "target": "/etc",
    "top_folders_returned": 3,
    "total_dirs": 260,
    "total_files": 1496,
    "total_size": 2401620,
    "total_size_formatted": "2.29 MB"
  },
  "results": [
    { "path": "etc", "size": 2401620, "files": 1496, "dirs": 260, "depth": 0 },
    { "path": "etc/ssh", "size": 633493, "files": 14, "dirs": 4, "depth": 1 }
  ]
}
```

### 3.2 Busca de Arquivos por Nome (`find`)
```bash
ssh vps-hermes fastfile find --target /etc --ext conf --limit 10 --json
```

### 3.3 Busca de Conteúdo (`grep`)
```bash
ssh vps-hermes fastfile grep "Port" /etc/ssh/sshd_config --json
```

---

## 4. MCP sobre SSH (Integração com Agentes de IA)

O servidor MCP do FastFile (`fastfile mcp`) roda nativamente sobre `stdio`. Para conectar agentes locais a servidores
remotos, basta configurar o cliente MCP para iniciar o processo via SSH. O túnel SSH encaminha o fluxo bidirecional de
mensagens JSON-RPC de forma transparente e segura.

### 4.1 Claude Code (`claude.json`)

```json
{
  "mcpServers": {
    "fastfile-vps": {
      "command": "ssh",
      "args": [
        "vps-hermes",
        "fastfile",
        "mcp"
      ]
    }
  }
}
```

### 4.2 DeepSeek Harness (DSH) (`mcp_config.json`)

```json
{
  "mcpServers": {
    "fastfile-vps": {
      "command": "ssh",
      "args": [
        "vps-hermes",
        "fastfile",
        "mcp"
      ]
    }
  }
}
```

### 4.3 Google Antigravity (`mcp_settings.json`)

```json
{
  "fastfile-vps": {
    "command": "ssh",
    "args": [
      "vps-hermes",
      "fastfile",
      "mcp"
    ]
  }
}
```

### 4.4 Cursor (`.cursor/mcp.json`)

```json
{
  "mcpServers": {
    "fastfile-vps": {
      "command": "ssh",
      "args": [
        "vps-hermes",
        "fastfile",
        "mcp"
      ]
    }
  }
}
```

---

## 5. Instalação em Servidores Linux

### Método 1: Script de Instalação (Recomendado)
No servidor alvo:
```bash
curl -fsSL https://raw.githubusercontent.com/fabiomotta0311/fastfile/main/scripts/install.sh | bash
```

### Método 2: Via Cargo
```bash
cargo install --git https://github.com/fabiomotta0311/fastfile.git ff-cli
```

### Método 3: Binário Pré-compilado (GitHub Releases)
Baixe o arquivo `fastfile-linux-x86_64.tar.gz` da página de Releases, descompacte e mova para `/usr/local/bin`:
```bash
tar -xzf fastfile-linux-x86_64.tar.gz
sudo mv ff fastfile /usr/local/bin/
```

---

## 6. Evidência de Validação na VPS (`vps-hermes`)

Durante a entrega da Issue #23, os seguintes comandos foram validados a partir de máquina cliente Windows contra a VPS:

1. `ssh vps-hermes 'fastfile du /etc --top 3 --json'` -> Sucesso (2.29 MB analisados em 264ms).
2. `ssh vps-hermes 'fastfile find --target /etc --ext conf --limit 3 --json'` -> Sucesso (167 matches encontrados em 14.9ms).
3. `ssh vps-hermes 'fastfile grep "Port" /etc/ssh/sshd_config --json'` -> Sucesso (3 ocorrências localizadas em 0.79ms).
4. `echo '{"jsonrpc":"2.0","id":1,"method":"initialize"}' | ssh vps-hermes fastfile mcp` -> Sucesso (resposta JSON-RPC `fastfile 0.1.0` recebida).
