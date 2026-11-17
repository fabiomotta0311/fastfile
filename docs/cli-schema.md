# Esquema JSON do FastFile CLI (`ff/1`)

Este documento especifica o contrato JSON versionado (`schema: "ff/1"`) do CLI do FastFile (`ff` / `fastfile`), projetado como interface de primeira classe para agentes de inteligência artificial (`docs/ARQUITETURA.md` §3.1 e §3.2).

## 1. Princípios do Contrato

1. **Versionamento Estrito:** Todas as respostas JSON contêm `"schema": "ff/1"`. Mudanças futuras no formato manterão retrocompatibilidade ou adotarão nova versão (`ff/2`).
2. **Ativação Automática:** A saída JSON é ativada explicitamente via `--json` ou **automaticamente** quando o stdout do processo não for um terminal interativo (ex.: redirecionamento, pipe ou processo filho invocado por agentes).
3. **Resumo Primeiro (Token-Econômico):** As respostas trazem metadados e contagens consolidadas no objeto `"summary"`, evitando que agentes precisem ler arrays extensos quando apenas o total importa.
4. **Proteção de Segredos por Padrão:** Arquivos com nomes sensíveis (`.env`, `.env.*`, `*.pem`, `*.key`, `*.jwt`, `*.har`, `id_rsa`, `credentials.json`, etc.) são ocultados e contabilizados em `"secrets_hidden"`. O usuário/agente deve passar `--include-secrets` explicitamente para desativar a proteção.
5. **Paginação Estável e Orçamento de Tokens Mensurável:** Paginação via `"cursor"` (numérico sequencial) e controle estrito de envelope via `--budget <N>` (tokens estimados, onde 1 token ≈ 4 bytes UTF-8 sobre todo o JSON emitido). Quando o orçamento é insuficiente até para o envelope mínimo, o CLI preserva o formato estruturado com `results: []`, marca `truncated: true` e emite aviso explicativo.
6. **Modo Compacto (`--compact` / `-c`):** Emissão minificada em linha única sem espaços extras, omitindo campos redundantes (`name`, `mtime_iso`, `total_size_formatted`) e relativizando caminhos automaticamente para consumo de contexto ultrabaixo.
7. **Somente Leitura:** Nenhum comando altera, move ou apaga arquivos no disco.

---

## 2. Envelope Padrão de Resposta

Toda resposta de sucesso segue o envelope abaixo:

```json
{
  "schema": "ff/1",
  "source": "walk",
  "command": "find",
  "warnings": [
    "1 segredo(s) ocultado(s); use --include-secrets para exibir"
  ],
  "summary": {
    "target": ".",
    "total_matches": 42,
    "returned_matches": 10,
    "secrets_hidden": 1,
    "elapsed_ms": 2.45
  },
  "results": [ ... ],
  "cursor": "10",
  "has_more": true,
  "truncated": false
}
```

### Campos do Envelope

| Campo | Tipo | Descrição |
|---|---|---|
| `schema` | `string` | Sempre `"ff/1"` para esta versão. |
| `source` | `string` | Fonte dos dados: `"walk"` (varredura paralela v0), `"mft"` (MFT one-shot v1) ou `"indexer"` (serviço residente v2). |
| `command` | `string` | Subcomando executado (`"du"`, `"find"`, `"stat"`, `"recent"`, `"batch"`). |
| `warnings` | `array<string>` | Avisos não-fatais (ex.: segredos ocultados, limite de tokens atingido, pastas sem permissão). Omitido se vazio. |
| `summary` | `object` | Resumo consolidado da operação (totais, contagens e tempo decorrido). |
| `results` | `array` | Lista de itens resultantes (para comandos que retornam múltiplos itens). |
| `result` | `object` | Item individual (usado por `stat`). |
| `cursor` | `string?` | Identificador para buscar a próxima página com `--cursor <valor>`. |
| `has_more` | `bool?` | Indica se existem mais itens além dos retornados nesta página. |
| `truncated` | `bool?` | `true` quando a resposta foi truncada pelo `--budget` de tokens. |

---

## 3. Envelope de Erro

Quando um erro ocorre no modo JSON, a saída é emitida com código de saída diferente de 0 no formato estruturado:

```json
{
  "schema": "ff/1",
  "error": {
    "code": "TARGET_NOT_FOUND",
    "message": "alvo não encontrado: 'diretorio_inexistente'"
  }
}
```

### Códigos de Erro e Saída

| Código do Erro | Código de Saída | Significado |
|---|---|---|
| `NOT_IMPLEMENTED` | 1 | Subcomando ou recurso ainda não implementado (stub). |
| `TARGET_NOT_FOUND` | 2 | Arquivo, diretório ou volume alvo não existe. |
| `INVALID_ARGUMENT` | 3 | Parâmetro inválido (ex.: formato de tamanho ou duração incorreto). |
| `IO_ERROR` | 4 | Falha de E/S ao ler o sistema de arquivos ou stdin. |
| `EXECUTION_ERROR` | 5 | Erro geral de execução. |

---

## 4. Esquemas por Subcomando

### 4.1 `ff du` (Disk Usage)

Agregação de uso de disco por pasta, com ordenação por tamanho e controle de profundidade.

#### Exemplo de Invocação
```bash
fastfile du . --top 10 --depth 1 --relative --json
```

#### Estrutura do JSON
```json
{
  "schema": "ff/1",
  "source": "walk",
  "command": "du",
  "summary": {
    "target": ".",
    "total_size": 154203810,
    "total_size_formatted": "147.06 MB",
    "total_files": 2450,
    "total_dirs": 320,
    "top_folders_returned": 2,
    "secrets_hidden": 0,
    "elapsed_ms": 12.34
  },
  "results": [
    {
      "path": "target",
      "size": 140500000,
      "files": 2100,
      "dirs": 250,
      "depth": 1
    },
    {
      "path": "crates",
      "size": 12500000,
      "files": 310,
      "dirs": 50,
      "depth": 1
    }
  ]
}
```

---

### 4.2 `ff find` (Busca de Arquivos)

Busca no índice com suporte a padrões, filtros por extensão, tamanho (`--size ">1GB"`), tipo (`--type dir` ou `file`) e recência.

#### Exemplo de Invocação
```bash
fastfile find "*.vhdx" --size ">1GB" --json
fastfile find --name target --type dir --relative --json
```

#### Estrutura do JSON
```json
{
  "schema": "ff/1",
  "source": "walk",
  "command": "find",
  "summary": {
    "target": ".",
    "pattern": "*.vhdx",
    "total_matches": 1,
    "returned_matches": 1,
    "secrets_hidden": 0,
    "total_scanned": 1540,
    "elapsed_ms": 1.15
  },
  "results": [
    {
      "path": "C:\\Discos\\dev.vhdx",
      "name": "dev.vhdx",
      "is_dir": false,
      "size": 15032385536,
      "mtime": 1700001234,
      "mtime_iso": "2023-11-14T22:33:54Z"
    }
  ]
}
```

---

### 4.3 `ff stat` (Metadados do Arquivo ou Diretório)

Exibe informações completas de tamanho, datas e atributos. Para diretórios, computa recursivamente tamanho total e contagem.

#### Exemplo de Invocação
```bash
fastfile stat Cargo.toml --json
```

#### Estrutura do JSON
```json
{
  "schema": "ff/1",
  "source": "walk",
  "command": "stat",
  "summary": {
    "target": "Cargo.toml",
    "type": "file",
    "size": 512,
    "mtime": 1700001234
  },
  "result": {
    "path": "Cargo.toml",
    "name": "Cargo.toml",
    "is_dir": false,
    "is_file": true,
    "size": 512,
    "mtime": 1700001234,
    "mtime_iso": "2023-11-14T22:33:54Z",
    "readonly": false,
    "hidden": false,
    "system": false,
    "total_size": null,
    "total_files": null,
    "total_dirs": null
  }
}
```

---

### 4.4 `ff recent` (Modificados Recentemente)

Lista arquivos alterados nos últimos instantes (ex.: `--since 1h`, `30m`, `24h`, `7d`), ordenados por data decrescente.

#### Exemplo de Invocação
```bash
fastfile recent --since 1h --limit 20 --json
```

#### Estrutura do JSON
```json
{
  "schema": "ff/1",
  "source": "walk",
  "command": "recent",
  "summary": {
    "target": ".",
    "since": "1h",
    "min_mtime": 1700000000,
    "total_matches": 3,
    "returned_matches": 3,
    "secrets_hidden": 0,
    "elapsed_ms": 2.10
  },
  "results": [
    {
      "path": "crates/ff-cli/src/lib.rs",
      "name": "lib.rs",
      "is_dir": false,
      "size": 25400,
      "mtime": 1700002100,
      "mtime_iso": "2023-11-14T22:48:20Z"
    }
  ]
}
```

---

### 4.5 `ff batch` (Consultas em Lote)

Recebe um array de consultas no JSON de entrada e responde todas em uma única operação.

#### Exemplo de Entrada (`queries.json` ou stdin)
```json
{
  "queries": [
    { "command": "stat", "target": "Cargo.toml" },
    { "command": "find", "pattern": "*.rs" }
  ]
}
```

#### Estrutura da Resposta
```json
{
  "schema": "ff/1",
  "source": "walk",
  "command": "batch",
  "summary": {
    "total_queries": 2,
    "processed": 2
  },
  "results": [
    {
      "command": "stat",
      "target": "Cargo.toml",
      "status": "ok"
    },
    {
      "command": "find",
      "target": ".",
      "pattern": "*.rs",
      "status": "ok"
    }
  ]
}
```

---

### 4.6 `ff grep` (Busca em Conteúdo via ripgrep)

Busca expressões regulares em arquivos sem carregar tudo na memória, pulando arquivos binários e respeitando segredos.

#### Exemplo de Invocação
```bash
fastfile grep "TODO" --ext rs --json
fastfile grep "pub fn" src/ --mode files --json
fastfile grep "version" Cargo.toml --mode count --json
```

#### Estrutura do JSON (Modo Content)
```json
{
  "schema": "ff/1",
  "source": "walk",
  "command": "grep",
  "summary": {
    "pattern": "TODO",
    "target": "crates",
    "mode": "content",
    "total_matches": 1,
    "files_searched": 35,
    "files_matched": 1,
    "secrets_hidden": 0,
    "elapsed_ms": 3.45
  },
  "result": {
    "summary": { ... },
    "matches": [
      {
        "path": "crates/ff-cli/src/lib.rs",
        "line_number": 42,
        "line_text": "    // TODO: implementar cache"
      }
    ]
  }
}
```

---

### 4.7 `ff diff` (Diferenças Estruturadas entre Arquivos)

Gera blocos (hunks) com linhas modificadas, adicionadas e removidas.

#### Exemplo de Invocação
```bash
fastfile diff old_config.toml new_config.toml --json
```

#### Estrutura do JSON
```json
{
  "schema": "ff/1",
  "source": "walk",
  "command": "diff",
  "summary": {
    "file_a": "old_config.toml",
    "file_b": "new_config.toml",
    "additions": 2,
    "deletions": 1,
    "is_identical": false,
    "hunks_count": 1
  },
  "result": {
    "file_a": "old_config.toml",
    "file_b": "new_config.toml",
    "additions": 2,
    "deletions": 1,
    "is_identical": false,
    "hunks": [
      {
        "old_start": 1,
        "old_lines": 2,
        "new_start": 1,
        "new_lines": 3,
        "header": "@@ -1,2 +1,3 @@",
        "lines": [
          { "kind": "Delete", "old_lineno": 1, "new_lineno": null, "text": "version = \"1.0\"" },
          { "kind": "Insert", "old_lineno": null, "new_lineno": 1, "text": "version = \"1.1\"" },
          { "kind": "Equal", "old_lineno": 2, "new_lineno": 2, "text": "name = \"fastfile\"" },
          { "kind": "Insert", "old_lineno": null, "new_lineno": 3, "text": "description = \"FastFile\"" }
        ]
      }
    ]
  }
}
```

---

## 5. Orçamento de Tokens (`--budget`) e Modo Compacto (`--compact`)

O FastFile foi concebido para agentes de IA cujo contexto é um recurso escasso e mensurável. As opções `--budget` e `--compact` garantem controle rigoroso sobre os tokens consumidos pelas consultas.

### 5.1 Unidade e Definição de Medida

- **Tokens Estimados:** O argumento `--budget <N>` define um teto em **tokens estimados de modelo de linguagem**, modelados pela heurística padrão da indústria:
  $$\text{max\_bytes} = \text{budget} \times 4 \text{ bytes UTF-8}$$
- **Limite Real sobre o Envelope Inteiro:** O teto de bytes é avaliado sobre a **resposta JSON serializada final** (incluindo chaves de topo `"schema"`, `"source"`, `"command"`, objeto `"summary"`, array `"warnings"` e elementos de paginação `"cursor"`, `"has_more"`, `"truncated"`), e **não** apenas sobre a quantidade de itens no array `"results"`.

### 5.2 Otimizações do Modo Compacto (`--compact` / `-c`)

Quando `--compact` é fornecido (ou ativado por chamadas MCP):
1. **Minificação sem Espaços:** Emite JSON em linha única via `serde_json::to_string`, eliminando quebras de linha e recuos que duplicariam o consumo de tokens.
2. **Eliminação de Informações Repetidas:**
   - O campo `name` é suprimido (`null`/omitido) quando puder ser derivado do `path`.
   - O campo `mtime_iso` é omitido em favor do timestamp Unix `mtime`.
   - Campos de exibição humana redundantes (como `total_size_formatted`) são excluídos do resumo.
3. **Caminhos Relativos Automáticos:** Quando o alvo for `.` ou um caminho relativo, o FastFile normaliza os caminhos dos resultados para ficarem relativos ao alvo, evitando repetição de prefixos absolutos longos (ex.: `C:\Users\...\`).

### 5.3 Comportamento do Envelope e Truncamento

1. **Envelope Mínimo não cabe (ex.: `--budget 1`):**
   - O CLI prioriza a integridade sintática do JSON estruturado para não induzir o agente a erro de parsing.
   - Retorna o envelope base estruturado com `"results": []`.
   - Define `"truncated": true`, `"has_more": true` e `"cursor": "0"`.
   - Inclui aviso explicativo detalhado em `"warnings"`:
     `"orçamento de 1 token(s) (~4 bytes) insuficiente para incluir resultados (envelope mínimo requer ... bytes); resultados omitidos"`.
2. **Orçamento Parcial:**
   - Inclui o maior número de itens inteiros em `"results"` que mantenha a saída serializada $\le \text{max\_bytes}$.
   - Itens que excedem o teto são removidos.
   - Define `"truncated": true`, `"has_more": true`, gera o `"cursor"` para a próxima página e anexa `"limite de tokens (--budget) atingido; resultados truncados"` em `"warnings"`.
3. **Sem Orçamento:**
   - Todo o conteúdo correspondente até `--limit` (padrão 100) é retornado com `"truncated": false`.
