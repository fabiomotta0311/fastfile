# Esquema JSON do FastFile CLI (`ff/1`)

Este documento especifica o contrato JSON versionado (`schema: "ff/1"`) do CLI do FastFile (`ff` / `fastfile`), projetado como interface de primeira classe para agentes de inteligência artificial (`docs/ARQUITETURA.md` §3.1 e §3.2).

## 1. Princípios do Contrato

1. **Versionamento Estrito:** Todas as respostas JSON contêm `"schema": "ff/1"`. Mudanças futuras no formato manterão retrocompatibilidade ou adotarão nova versão (`ff/2`).
2. **Ativação Automática:** A saída JSON é ativada explicitamente via `--json` ou **automaticamente** quando o stdout do processo não for um terminal interativo (ex.: redirecionamento, pipe ou processo filho invocado por agentes).
3. **Resumo Primeiro (Token-Econômico):** As respostas trazem metadados e contagens consolidadas no objeto `"summary"`, evitando que agentes precisem ler arrays extensos quando apenas o total importa.
4. **Proteção de Segredos por Padrão:** Arquivos com nomes sensíveis (`.env`, `.env.*`, `*.pem`, `*.key`, `*.jwt`, `*.har`, `id_rsa`, `credentials.json`, etc.) são ocultados e contabilizados em `"secrets_hidden"`. O usuário/agente deve passar `--include-secrets` explicitamente para desativar a proteção.
5. **Paginação Estável e Orçamento de Tokens:** Paginação via `"cursor"` (numérico sequencial) e limitação por tokens aproximados via `--budget <N>` (truncamento seguro sem quebrar o formato JSON).
6. **Somente Leitura:** Nenhum comando altera, move ou apaga arquivos no disco.

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
