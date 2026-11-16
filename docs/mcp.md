# Servidor MCP FastFile (`fastfile mcp`)

Documentação e guia de integração do servidor **Model Context Protocol (MCP)** do FastFile
(`docs/ARQUITETURA.md` §3.1, §3.3 e Issue #21).

O FastFile disponibiliza um servidor MCP nativo sobre `stdio` para agentes de IA (Claude Code, DSH,
Cursor, Antigravity e outros). O servidor expõe as capacidades de busca instantânea, inspeção e diff
do projeto mantendo as garantias de segurança, eficiência e economia de contexto.

---

## 1. Princípios e Garantias do Servidor

1. **Protocolo Padrão:** Compatível com a especificação Model Context Protocol (MCP) sobre JSON-RPC 2.0 via `stdio`.
2. **Dois nomes, um binário (§3.3):** O servidor deve ser configurado preferencialmente com o comando `fastfile mcp`
   (ou `ff mcp`).
3. **Somente Leitura (§3.1):** Nenhuma ferramenta cria, altera, move ou apaga arquivos no sistema.
4. **Proteção de Segredos Ativa por Padrão (§3.2):** Arquivos sensíveis (`.env`, chaves privadas, certificados, `*.jwt`,
   `*.har`) são automaticamente omitidos dos resultados e pesquisas, a menos que `include_secrets: true` seja solicitado
   explicitamente.
5. **Orçamento de Tokens (`budget`):** Ferramentas que produzem grandes volumes de dados aceitam o parâmetro `budget`
   para truncar os resultados com segurança antes de estourar a janela de contexto do agente.
6. **Formato Unificado `ff/1`:** Todas as ferramentas retornam respostas de texto contendo o envelope JSON tipado `ff/1`
   com metadados de execução (`duration_ms`, `total_matched`, `returned`, `truncated`, `warnings`).
7. **Sem Acesso à Rede (§6, §3.4):** O servidor FastFile não se comunica com a rede. Para servidores remotos ou VPS, a
   comunicação ocorre via MCP sobre SSH.

---

## 2. Configuração de Clientes MCP

### 2.1 Claude Code

No arquivo `claude.json` (ou via comando `claude mcp add`):

```json
{
  "mcpServers": {
    "fastfile": {
      "command": "fastfile",
      "args": ["mcp"]
    }
  }
}
```

### 2.2 DeepSeek Harness (DSH)

No arquivo de configuração de MCP do DSH (`mcp_config.json`):

```json
{
  "mcpServers": {
    "fastfile": {
      "command": "fastfile",
      "args": ["mcp"]
    }
  }
}
```

### 2.3 Cursor

Em `.cursor/mcp.json` ou nas configurações globais de MCP:

```json
{
  "mcpServers": {
    "fastfile": {
      "command": "fastfile",
      "args": ["mcp"]
    }
  }
}
```

### 2.4 Google Antigravity

Em sua configuração local de MCP (`mcp_settings.json` ou manifesto de servidor):

```json
{
  "fastfile": {
    "command": "fastfile",
    "args": ["mcp"]
  }
}
```

### 2.5 Servidores Remotos e VPS (MCP sobre SSH — §3.4)

O FastFile não possui cliente nem servidor de rede próprio. O uso em máquinas remotas (Linux VPS, servidores)
ocorre através de túnel SSH:

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

## 3. Catálogo de Ferramentas Expostas

O servidor MCP expõe 7 ferramentas:

| Ferramenta | Descrição | Parâmetros Principais |
|---|---|---|
| `disk_usage` | Agrega uso de disco por pasta (estilo `du` / WizTree) | `target`, `top`, `depth`, `budget`, `relative`, `all` |
| `find_files` | Busca arquivos por nome, curingas (`*.rs`), tamanho, tipo e data | `pattern`, `name`, `target`, `size`, `type`, `limit`, `cursor`, `budget` |
| `file_stat` | Metadados completos de arquivo/pasta com contagem recursiva | `target`, `include_secrets` |
| `recent_changes` | Arquivos modificados recentemente | `target`, `since`, `limit`, `cursor`, `budget`, `relative` |
| `search_content` | Busca regex/texto em conteúdo de arquivos (estilo ripgrep) | `pattern`, `path`, `ext`, `type`, `mode`, `limit`, `budget` |
| `diff_files` | Diff estruturado de arquivos em blocos com linhas unificadas | `file_a`, `file_b`, `context`, `budget` |
| `batch` | Executa múltiplas consultas agregadas em uma única chamada | `queries` (lista de consultas `find`, `stat`, `grep`, `diff`, `sym`) |
| `find_symbol` | Localiza onde uma função, struct, classe, trait ou tipo está definida (Tree-Sitter) | `name`, `path`, `kind`, `lang`, `limit`, `cursor`, `budget`, `relative` |

---

## 4. Detalhes das Ferramentas e Exemplos de Parâmetros

### 4.1 `disk_usage`
Calcula o consumo de disco no diretório ou volume alvo:
```json
{
  "target": ".",
  "top": 10,
  "depth": 1,
  "budget": 2000,
  "relative": true
}
```

### 4.2 `find_files`
Localiza arquivos com filtros instantâneos:
```json
{
  "pattern": "*.rs",
  "target": "crates",
  "size": ">10KB",
  "type": "file",
  "limit": 20,
  "relative": true
}
```

### 4.3 `file_stat`
Inspeciona atributos e metadados de uma entrada:
```json
{
  "target": "Cargo.toml"
}
```

### 4.4 `recent_changes`
Lista alterações recentes (ex.: na última hora ou no último dia):
```json
{
  "target": ".",
  "since": "2h",
  "limit": 25,
  "relative": true
}
```

### 4.5 `search_content`
Busca no conteúdo de arquivos sem indexar binários:
```json
{
  "pattern": "fn run_mcp_server",
  "path": "crates",
  "ext": "rs",
  "limit": 10
}
```

### 4.6 `diff_files`
Compara dois arquivos gerando formato estruturado:
```json
{
  "file_a": "Cargo.lock.old",
  "file_b": "Cargo.lock",
  "context": 3,
  "budget": 1500
}
```

### 4.7 `batch`
Executa várias pesquisas em lote:
```json
{
  "queries": [
    { "type": "stat", "target": "Cargo.toml" },
    { "type": "find", "pattern": "main.rs", "target": "crates" },
    { "type": "sym", "name": "run_mcp_server" }
  ]
}
```

### 4.8 `find_symbol`
Localiza declarações de símbolos via análise sintática AST (Tree-Sitter):
```json
{
  "name": "AuthenticatedExecutor",
  "path": ".",
  "kind": "struct",
  "lang": "rust",
  "limit": 10
}
```

### 4.9 `repo_map`
Gera visão compacta de um repositório (estrutura de pastas, linguagens, contagens, tamanhos e principais símbolos por arquivo via Tree-Sitter):
```json
{
  "path": ".",
  "depth": 3,
  "budget": 1500
}
```

### 4.10 `find_duplicates`
Localiza arquivos duplicados (mesmo tamanho e mesmo hash) para diagnóstico de desperdício de espaço em disco (somente leitura):
```json
{
  "path": ".",
  "min_size": "10KB",
  "limit": 20
}
```

---

## 5. Formato de Resposta

Toda execução de ferramenta MCP devolve o resultado envelopado no padrão `ff/1`:
```json
{
  "schema": "ff/1",
  "source": "walk",
  "duration_ms": 2.4,
  "total_matched": 1,
  "returned": 1,
  "truncated": false,
  "results": [...]
}
```

Em caso de falha controlada (ex.: arquivo não encontrado), a ferramenta retorna erro com `isError: true` no protocolo MCP
ou um envelope com código de erro estruturado:
```json
{
  "schema": "ff/1",
  "error": {
    "code": "TARGET_NOT_FOUND",
    "message": "alvo não encontrado: 'inexistente.txt'"
  }
}
```
