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
6. **Formato Unificado `ff/2`:** Todas as ferramentas retornam respostas de texto contendo o envelope JSON tipado `ff/2` (caminhos abrem direto da pasta atual do servidor; `FASTFILE_SCHEMA=1` no ambiente do servidor volta ao `ff/1`, ver `docs/cli-schema.md` §0)
   com metadados de execução (`duration_ms`, `total_matched`, `returned`, `truncated`, `warnings`).
7. **Sem Acesso à Rede (§6, §3.4):** O servidor FastFile não se comunica com a rede. Para servidores remotos ou VPS, a
   comunicação ocorre via MCP sobre SSH.
8. **Protocolo:** negocia `protocolVersion` (`2025-06-18`, `2025-03-26`, `2024-11-05`; devolve a pedida pelo
   cliente quando suportada) e envia `instructions` no `initialize` dizendo ao modelo quando preferir o FastFile.
9. **Saída compacta e erros tipados:** o texto de cada resposta é JSON minificado (26–43% menos bytes que a
   versão indentada). Erros trazem `error.code` tipado: `TARGET_NOT_FOUND`, `INVALID_ARGUMENT`,
   `UNKNOWN_TOOL`, `TIMEOUT` ou `TOOL_ERROR`.
10. **Limite de tempo:** cada chamada desiste depois de `FASTFILE_MCP_TIMEOUT` segundos (padrão 45) e devolve
    `TIMEOUT` com a dica de iniciar o indexador, em vez de deixar o cliente pendurado.
11. **Experimental é opt-in explícito:** as ferramentas marcadas como experimentais aparecem em `tools/list`,
    mas qualquer chamada precisa declarar `experimental: true`. Sem isso a resposta é `INVALID_ARGUMENT` e
    nenhum dado é devolvido. Contrato e formato podem mudar entre versões menores.

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

O servidor MCP expõe 12 ferramentas, todas com `annotations.readOnlyHint: true` (e `destructiveHint: false`, `openWorldHint: false`), o que permite aos clientes dispensar confirmação:

| Ferramenta | Descrição | Parâmetros Principais |
|---|---|---|
| `disk_usage` | Agrega uso de disco por pasta (estilo `du` / WizTree) | `target`, `top`, `depth`, `budget`, `relative`, `all` |
| `find_files` | Busca arquivos por nome, curingas (`*.rs`), tamanho, tipo e data | `pattern`, `name`, `target`, `size`, `type`, `limit`, `cursor`, `budget` |
| `file_stat` | Metadados completos de arquivo/pasta com contagem recursiva | `target`, `include_secrets` |
| `recent_changes` | Arquivos modificados recentemente | `target`, `since`, `limit`, `cursor`, `budget`, `relative` |
| `search_content` | Busca regex/texto em conteúdo de arquivos (estilo ripgrep, acelerado por trigramas) | `pattern`, `path`, `ext`, `type`, `mode`, `limit`, `budget`, `use_index`, `build_index` |
| `diff_files` | Diff estruturado de arquivos em blocos com linhas unificadas | `file_a`, `file_b`, `context`, `budget` |
| `batch` | Executa múltiplas consultas agregadas em uma única chamada | `queries` (lista de consultas `find`, `stat`, `du`, `grep`, `sym`, `diff`, `map`, `dupes`, `recent`, `read`) |
| `find_symbol` | Localiza onde uma função, struct, classe, trait ou tipo está definida (Tree-Sitter) | `name`, `path`, `kind`, `lang`, `limit`, `cursor`, `budget`, `relative` |
| `repo_map` | Visão compacta do repositório: árvore, linguagens, tamanhos e principais símbolos | `path`, `depth`, `budget` (padrão 2000; `0` = sem teto), `all` |
| `find_duplicates` | Arquivos duplicados por tamanho + hash (somente leitura) | `path`, `min_size`, `limit`, `budget`, `relative`, `all` |
| `read_files` | Lê vários arquivos ou trechos numa chamada (`a.rs:10-80`, `b.rs:120`, `c.rs:300-`), com `next` para continuar, `outline` para o esqueleto de símbolos, PDF/DOCX como texto e segredos bloqueados/mascarados | `paths`, `max_lines`, `around`, `outline`, `budget`, `include_secrets` |
| `validate_plan` (**experimental**) | Sugere como validar mudanças Rust locais a partir do Git, com cobertura por camada, primeira checagem, validação adicional e motivos de ampliação; nunca executa nada | `experimental` (obrigatório `true`), `root`, `path`, `base`, `budget` |

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
Busca no conteúdo de arquivos sem indexar binários (acelerada por índice de trigramas quando disponível):
```json
{
  "pattern": "fn run_mcp_server",
  "path": "crates",
  "ext": "rs",
  "limit": 10,
  "use_index": true,
  "build_index": false
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

### 4.11 `repair_context` (experimental, opt-in)

Fora do catálogo padrão: só aparece em `tools/list` (e só é executável) com `FASTFILE_EXPERIMENTAL=context` no ambiente do servidor, por exemplo `"env": {"FASTFILE_EXPERIMENTAL": "context"}` na configuração do cliente. Sem isso o catálogo continua com 11 ferramentas e a chamada devolve `UNKNOWN_TOOL`. Equivale ao `fastfile context` (resposta `ff-context/1`, ver [`cli-schema.md`](cli-schema.md) §4.9):
```json
{
  "root": ".",
  "diagnostic": "crates/example/src/math.rs:2|assertion failed",
  "budget": 4000,
  "timeout_ms": 20000
}
```
Exatamente um de `location`, `diagnostic` ou `symbol`. Campos desconhecidos (`include_secrets`, `all`, etc.) são rejeitados com `INVALID_ARGUMENT`: não há modo irrestrito. O prazo cooperativo é no máximo 80% de `FASTFILE_MCP_TIMEOUT`, para que um escopo lento devolva uma resposta parcial (`deadline_exceeded`) em vez de `TIMEOUT`. Trechos `observed` são evidência verificada; `candidate` são testes próximos ou menções textuais, nunca chamadas resolvidas. Resposta `partial`, `truncated` ou com `empty_means_absent: false` não significa ausência: reler as faixas de `omitted` com `read_files` ou refazer a chamada com `location` numa delas.

### 4.12 `validate_plan` (experimental)
Devolve o mesmo plano de `fastfile validate-plan --experimental` (`docs/cli-schema.md`), para o harness
decidir como validar uma mudança:
```json
{
  "experimental": true,
  "root": ".",
  "path": "crates/ff-cli",
  "base": "main",
  "budget": 2000
}
```

- `root` precisa ser a raiz do checkout Git; `path` é literal e relativo a ela (`..`, caminho absoluto ou
  arquivo sensível são rejeitados). `base` aceita apenas ref simples ou SHA local.
- `executed: false` e `evidence_status: "not_executed"` são invariantes: a ferramenta sugere `argv`
  estruturado e evidência (teste, manifesto, workflow conhecido), mas **não executa Cargo, testes,
  filtros Git ou comandos de workflow**. A evidência de validação só passa a existir depois que o
  harness roda os testes.
- `first_check` é a checagem mais estreita que a evidência sustenta; `additional` traz as suítes mais
  amplas até a do workspace. Quando a cobertura está incompleta ou há ambiguidade, `first_check` já é
  a suíte do workspace, com `broadening_reasons` explicando o motivo.
- `budget` trunca o plano **ampliando** a validação: `truncated: true`, `coverage.complete: false`,
  `budget_truncated` em `broadening_reasons`, `changes` omitidas (contadas em `changes_omitted`) e
  apenas a suíte ampla preservada. Orçamento menor que o envelope mínimo devolve `first_check: null`
  e `additional: []` — nunca uma seleção estreita sem a evidência que a sustentava.

---

## 5. Formato de Resposta

Toda execução de ferramenta MCP devolve o resultado envelopado no padrão `ff/2`:
```json
{
  "schema": "ff/2",
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
  "schema": "ff/2",
  "error": {
    "code": "TARGET_NOT_FOUND",
    "message": "alvo não encontrado: 'inexistente.txt'"
  }
}
```
