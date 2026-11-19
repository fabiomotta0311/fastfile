# Esquema JSON do FastFile CLI (`ff/2`, com `ff/1` sob pedido)

Este documento especifica o contrato JSON versionado do CLI do FastFile (`ff` / `fastfile`), projetado como interface de primeira classe para agentes de inteligência artificial (`docs/ARQUITETURA.md` §3.1 e §3.2). A versão atual é **`ff/2`** (padrão desde 2026-11-18); a anterior, `ff/1`, continua disponível.

### Experimental: plano de validação Rust

Na raiz de um checkout Git, use `fastfile validate-plan --experimental` ou delimite a coleta com
`--path crates/ff-cli`. A partir de outro diretório, informe `--root <raiz-do-checkout>`.
`--path` é literal e relativo à raiz. Com `--base main` (ou outro ref simples/SHA local), a
primeira camada passa de `staged` (HEAD → índice) a `base_to_index` (base → índice, incluindo
commits). `unstaged` (índice → arquivos locais) e `untracked` não ignorado permanecem separados.

A resposta `ff/validate-plan/experimental-2` mantém `executed: false` e
`evidence_status: "not_executed"`: o FastFile sugere, o harness decide e executa, e **nenhuma
evidência de validação existe antes disso**. O plano traz:

| Campo | Conteúdo |
|---|---|
| `coverage` | camada de cada comparação e `complete` — só é `true` quando todo o inventário Git foi lido e nenhuma mudança ficou de fora |
| `changes` | camada (`staged`, `base_to_index`, `unstaged`, `untracked`), status, `ranges` e `symbols` aproximados por posição de declaração; `generated: true` marca arquivo que se declara gerado |
| `changes_omitted` | mudanças não exibidas por orçamento (só no MCP) |
| `impacted_packages` | crates afetados, incluindo dependentes locais transitivos |
| `first_check` | a checagem a rodar primeiro: `scope` (`first_test`, `package` ou `workspace`), `argv` estruturado e `evidence` |
| `additional` | suítes mais amplas, da mais estreita para a do workspace |
| `broadening_reasons` | por que a seleção foi ampliada: `partial_diff`, `removed_or_renamed_path`, `generated_code_change`, `excluded_path_change`, `configuration_or_non_rust_change`, `unmapped_rust_path`, `unmapped_symbols`, `ambiguous_package`, `no_root_manifest`, `manifest_unavailable`, `unresolved_path_dependency`, `evidence_limit`, `test_evidence_unavailable` (e `budget_truncated` no MCP) |
| `truncated` | se o orçamento do MCP cortou o plano |

`first_check` e `additional` trazem `argv` e evidências; execute somente após revisar e autorizar o
comando, a partir de `root`. Os caminhos de saída seguem `PATH-CWD-001`. O plano usa manifestos e
evidência de teste da working tree; nomes de símbolos são aproximações pelas posições das
declarações, e referências em testes de integração são heurísticas. Um candidato não comprova
cobertura. Alterações em configuração, renomeações, remoções, código gerado, arquivo excluído do
escopo e evidência ambígua favorecem a suíte do workspace: nesses casos `first_check` já é a suíte
ampla e `additional` fica vazio.

Somente leitura: o comando não invoca filtros, helpers de diff, Cargo, nem comandos do workflow.
Renomeações exatas e não ambíguas no índice são agrupadas; as demais aparecem como remoção e
adição em suas camadas. A comparação local normaliza CRLF e não cobre mudanças só de modo ou
transformações de filtros Git. Arquivos sensíveis, artefatos e não rastreados ignorados ficam fora
do plano. Limites de inventário (2 MiB por consulta Git), caminhos (1.024) e conteúdo (128 KiB
por arquivo / 16 MiB no total), conflitos e arquivos não acessíveis podem impedir a análise;
consulte `coverage.complete` e `broadening_reasons`.

O mesmo plano é entregue pelo MCP em `validate_plan` (`docs/mcp.md` §4.12), com opt-in explícito
(`experimental: true`) e truncamento por `budget` que **amplia** a sugestão em vez de estreitá-la.

## 0. `ff/2` × `ff/1`

| | `ff/2` (padrão) | `ff/1` (legado) |
|---|---|---|
| Como pedir | padrão; `--schema 2` ou `FASTFILE_SCHEMA=2` | `--schema 1` ou `FASTFILE_SCHEMA=1` (vale também para `fastfile mcp`) |
| Campo `path` (find, recent, grep, sym, map, dupes, stat, read) | **abre direto da pasta atual**: relativo a ela quando o arquivo está dentro dela, absoluto quando não está (contrato `PATH-CWD-001`) | relativo ao alvo pedido (`ff grep x crates` devolvia `ff-cli\src\lib.rs`) ou absoluto, conforme `--relative`/`--compact` |
| Separador | sempre `/`, sem o prefixo `\\?\` (inclusive `map.root`) | o do sistema (`\\` no JSON do Windows) |
| `grep`: sumário | só no `summary` do envelope (com `indexed`, `candidates_count`, `files_skipped_by_index`, `files_outside_index` quando o índice de trigramas é usado) | repetido em `result.summary` |
| `--relative` | sem efeito (a regra acima vale sempre) | define a base dos caminhos |

Os demais campos são iguais nas duas versões. `summary.target` continua sendo o alvo exatamente como foi pedido.

**Migração:** quem juntava o alvo ao caminho devolvido (`join(target, path)`) deve passar a usar o `path` direto, ou fixar `FASTFILE_SCHEMA=1` enquanto migra.

## 1. Princípios do Contrato

1. **Versionamento Estrito:** Todas as respostas JSON contêm `"schema"` (`"ff/2"` por padrão, `"ff/1"` sob pedido). Mudanças incompatíveis adotam nova versão; os exemplos abaixo mostram o envelope, que é igual nas duas versões salvo o descrito no §0.
2. **Ativação Automática:** A saída JSON é ativada explicitamente via `--json` ou **automaticamente** quando o stdout do processo não for um terminal interativo (ex.: redirecionamento, pipe ou processo filho invocado por agentes).
3. **Resumo Primeiro (Token-Econômico):** As respostas trazem metadados e contagens consolidadas no objeto `"summary"`, evitando que agentes precisem ler arrays extensos quando apenas o total importa.
4. **Proteção de Segredos por Padrão:** Arquivos com nomes sensíveis (`.env`, `.env.*`, `*.pem`, `*.key`, `*.jwt`, `*.har`, `id_rsa`, `credentials.json`, etc.) são ocultados e contabilizados em `"secrets_hidden"`. O usuário/agente deve passar `--include-secrets` explicitamente para desativar a proteção. Além disso, **valores com cara de segredo dentro do conteúdo** (tokens do GitHub/Slack/AWS/Stripe/Google, chaves `sk-…`, JWT, cabeçalhos de chave privada, senha em URL de conexão e atribuições como `password = …`) são substituídos por `«redacted:<tipo>»` em `grep`, `diff` e assinaturas do `sym`; a contagem sai em `summary.secrets_masked` (contrato `SECRETS-CONTENT-001`).
5. **Paginação Estável e Orçamento de Tokens Mensurável:** Paginação via `"cursor"` (numérico sequencial) e controle estrito de envelope via `--budget <N>` (tokens estimados, onde 1 token ≈ 4 bytes UTF-8 sobre todo o JSON emitido). Quando o orçamento é insuficiente até para o envelope mínimo, o CLI preserva o formato estruturado com `results: []`, marca `truncated: true` e emite aviso explicativo.
6. **Modo Compacto (`--compact` / `-c`):** Emissão minificada em linha única sem espaços extras, omitindo campos redundantes (`name`, `mtime_iso`, `total_size_formatted`). No `ff/1`, também relativiza caminhos ao alvo.
7. **Somente Leitura:** Nenhum comando altera, move ou apaga arquivos no disco.

---

## 2. Envelope Padrão de Resposta

As operações estáveis seguem o envelope abaixo. Os comandos opt-in `checkpoint` e
`changes` têm um [contrato experimental separado](checkpoints.md).

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
| `TIMEOUT` | 6 | A consulta excedeu `--timeout <segundos>` (opção global) ou `FASTFILE_TIMEOUT`. |
| `USAGE_ERROR` | 64 | Uso inválido da linha de comando (flag desconhecida, argumento ausente). Emitido em JSON quando a saída não é um terminal ou com `--json`/`-c`. |

**Avisos de fallback.** Quando a resposta vem da varredura em vez do índice residente (ex.: `find --all`
com o `ff-indexer` parado) ou a varredura passa de 2 s, `warnings` traz o tempo gasto e a dica para iniciar o
indexador. Agentes devem preferir `--timeout` (ou `FASTFILE_TIMEOUT`) a deixar o harness esperar.

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

Recebe um array de consultas no JSON de entrada e responde todas em uma única operação. Comandos aceitos: `stat`, `find`, `du`, `grep`
(com `mode` e `case_sensitive`), `sym`, `diff`, `map`, `dupes` e `recent`. Cada item de `results` é o
envelope `ff/1` completo da consulta correspondente; consultas inválidas viram
`{"command", "status": "error", "error"}` sem interromper as demais.

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
      "schema": "ff/1",
      "command": "stat",
      "summary": { "target": "Cargo.toml", "type": "file", "size": 512, "mtime": 1700001234 },
      "result": { "path": "Cargo.toml", "is_dir": false, "size": 512, "...": "..." }
    },
    {
      "schema": "ff/1",
      "command": "find",
      "summary": { "pattern": "*.rs", "total_matches": 83, "returned_matches": 20, "...": "..." },
      "results": [ { "path": "crates/ff-cli/src/lib.rs", "...": "..." } ],
      "cursor": "20",
      "has_more": true
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

#### Opções compatíveis com o ripgrep

`-s/--case-sensitive` (o padrão do `ff grep` **ignora** maiúsculas e minúsculas, ao contrário do `rg`),
`-F/--fixed-strings` (padrão literal), `-w/--word` (palavra inteira) e `--hidden` (inclui pastas ocultas
como `.github/`, mantendo o `.gitignore`; `--all` desliga também o `.gitignore`). No MCP:
`case_sensitive`, `fixed_strings`, `word` e `hidden`.

#### Índice de trigramas (`--build-index`)

O índice só **descarta** arquivos que representa fielmente (mesmo mtime e tamanho, com trigramas
extraídos). Arquivos novos, alterados, acima dos tetos do índice (5 MB por arquivo, 200 MB ou 50 mil
arquivos no total) e documentos PDF/DOCX são sempre lidos, então o resultado é idêntico ao da busca
sem índice (contrato `TRIGRAM-COVERAGE-001`). Os trigramas da consulta saem da HIR do regex e só usam
literais obrigatórios (`TRIGRAM-QUERY-001`). Com índice, `result.summary` traz `indexed`,
`candidates_count`, `files_skipped_by_index` e `files_outside_index`.

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

### 4.8 `ff read` (Leitura de Arquivos e Trechos)

Lê vários arquivos ou trechos numa chamada, em fluxo (nunca carrega o arquivo inteiro para devolver um
trecho). Especificações: `caminho` (do início, até `--max-lines`, padrão 400), `caminho:N` (linha N com
`--around` linhas de contexto, padrão 10), `caminho:A-B` e `caminho:A-` (de A até `--max-lines`).
`--outline` devolve só o esqueleto de símbolos (rust, python, js, ts, go, cs). PDF/DOCX saem como texto;
binários vêm com `binary: true`; arquivos de segredo vêm com erro `SECRET_FILE`; valores com cara de
segredo são mascarados. `--budget` limita o conteúdo somado de todos os arquivos.

```bash
fastfile read crates/ff-cli/src/lib.rs:1210-1260 Cargo.toml:3 --around 2
fastfile read crates/ff-cli/src/read.rs --outline
```

```json
{
  "schema": "ff/2",
  "command": "read",
  "summary": { "files": 2, "lines_returned": 56, "secrets_hidden": 0, "secrets_masked": 0, "elapsed_ms": 3.6 },
  "results": [
    { "path": "crates/ff-cli/src/lib.rs", "start_line": 1210, "end_line": 1260, "total_lines": 3592, "content": "fn run_find(...
..." },
    { "path": "Cargo.toml", "start_line": 1, "end_line": 5, "focus_line": 3, "total_lines": 58, "content": "[workspace]
..." }
  ]
}
```

Campos opcionais por arquivo: `focus_line` (pedido `caminho:N`), `encoding` (`utf-16le`, `utf-16be`,
`lossy`, `extracted` para PDF/DOCX; ausente = UTF-8), `long_lines_cut` (linhas acima de 2000 caracteres
cortadas), `truncated` + `next` (a especificação que continua a leitura, ex.: `"lib.rs:401-"`), `binary`,
`outline` (lista de `{line, kind, name, signature}`) e `error` (`TARGET_NOT_FOUND`, `IS_DIRECTORY`,
`SECRET_FILE`, `IO_ERROR`, `TOO_LARGE`, `UNSUPPORTED_LANGUAGE`). Um arquivo com erro não interrompe os demais;
com um único arquivo inexistente, o processo sai com `TARGET_NOT_FOUND` (código 2).

---

### 4.9 `fastfile context` (experimental, CLI e MCP `repair_context`)

O comando é experimental e opt-in por invocação explícita. A saída de sucesso usa
`ff-context/1`, independente de `--schema`; erros usam o envelope padrão. No MCP a
mesma operação é a ferramenta `repair_context`, só listada com
`FASTFILE_EXPERIMENTAL=context` (ver [`mcp.md`](mcp.md) §4.11); o JSON é o mesmo do CLI,
descontado o transporte JSON-RPC. `ff/1`, `ff/2` e o catálogo padrão não mudam.

Na raiz de um checkout, forneça exatamente uma entrada:

```text
fastfile context --root . --location crates/example/src/math.rs:2
fastfile context --root . --diagnostic "crates/example/src/math.rs:2|assertion failed"
fastfile context --root . --symbol total
```

O diagnóstico usa o primeiro `|` como delimitador; a mensagem é obrigatória,
mas não é interpretada nem devolvida. Use caminho direto, absoluto (inclusive
`C:\projeto\src\lib.rs:12`) ou relativo à pasta atual, e linha de base 1.
`anchor.status` é `observed` só se a linha existir no snapshot autorizado,
`unverified` se não for confirmável e `query` para uma busca por símbolo. Na
entrada por localização, o menor nó de definição sintática sobre a linha fornece
o nome candidato, se houver. Os caminhos devolvidos seguem `PATH-CWD-001`.

- `symbol_status: ambiguous` significa duas ou mais definições observadas para o
  nome, mesmo em módulos diferentes. `one_candidate` não equivale a resolução
  semântica; `not_observed` se limita ao escopo percorrido.
- Cada trecho tem `path`, `start_line`, `end_line` inclusive, `content` mascarado e
  `evidence` com `kind`, `line`, `status` e `reasons`. Localizações e declarações
  são observadas; testes `#[test]` e referências textuais são candidatos. Motivos
  incluem `same_module`, `textual_reference` e `adjacent_test` (até 24 linhas no
  mesmo arquivo). Comentários também podem casar; não há resolução de chamadas,
  avaliação de `cfg` ou expansão de macros.
- A seleção prioriza localização, definições, testes e referências nessa ordem;
  empates por caminho e linha. Faixas sobrepostas do mesmo arquivo são reunidas.
  A descoberta lê fontes Rust elegíveis; só as faixas selecionadas são extraídas
  e devolvidas, do mesmo snapshot usado para a descoberta.
- Escopo: diretório explícito, UTF-8 `.rs`, regras `.gitignore` e `.ignore` dentro
  da raiz, exclusão de ocultos, diretórios de build, segredos e links/junctions.
  Cada arquivo é lido uma vez e revalidado (tamanho, mtime, identidade e caminho);
  se mudou durante a leitura é descartado com `file_changed_during_read` (escopo
  parcial). A revalidação é de melhor esforço: uma troca que preserve tamanho e
  mtime no mesmo tick não é detectável. Não execute em raízes remotas.
- `coverage: partial` e `warnings` sinalizam lacunas de leitura, sintaxe ou
  limites: 20.000 entradas retornadas pela varredura, 512 arquivos Rust, 256 KiB
  por arquivo, 16 MiB de fontes, 128 referências por arquivo e 32 evidências.
  Uma saída parcial vazia não prova ausência.

Contrato `ff-context/1` (campos novos em relação ao protótipo, sem remover os antigos):

| Campo | Significado |
|---|---|
| `root` | Raiz autorizada, no formato de `PATH-CWD-001` (`restricted` se o caminho for sensível) |
| `anchor` | Âncora; `unverified` traz `reason`: `not_in_scope`, `line_out_of_range` ou `ambiguous_path` |
| `coverage`, `warnings`, `files_skipped` | `scanned_scope` ou `partial`; códigos do que não foi lido |
| `truncated`, `budget` | `budget.tokens` pedido (1 a 50000; padrão 4000; 1 token ≈ 4 bytes), `used_tokens` e se houve corte |
| `omitted_count`, `omitted` | Faixas que não couberam, com `path`, `start_line`, `end_line`, `status` e `read` (`caminho:A-B`, para reler com `read`/`read_files`) |
| `empty_means_absent` | `true` só com escopo completo, sem corte e âncora verificada; nunca interprete vazio como ausência quando `false` |

Avisos novos: `budget_truncated`, `deadline_exceeded`, `link_skipped`, `file_changed_during_read`.
O orçamento mantém, por prioridade, localização e definições (observadas), depois testes e
referências textuais (candidatos). Se nem o envelope couber, a resposta mínima é
`{"schema","command","coverage":"partial","truncated":true,"empty_means_absent":false,"warnings":["budget_too_small"]}`
(cerca de 35 tokens). `--deadline-ms` (MCP: `timeout_ms`) é um prazo cooperativo: ao expirar,
devolve o que foi lido com `deadline_exceeded`; `0` expira de imediato. Links e junctions nunca são
seguidos e geram `link_skipped`; arquivo ilegível gera `unreadable_source`; ambos tornam o escopo parcial.
Não há modo irrestrito: segredos são sempre mascarados, a mensagem do diagnóstico nunca é
devolvida e os erros não ecoam a entrada.

O caso sintético (três entradas, teste em outro arquivo, homônimos e deduplicação)
é executável com `cargo test -p ff-cli --test context_contract --locked`; o contrato de orçamento,
cobertura, escopo e equivalência CLI/MCP, com `cargo test -p ff-cli --test context_mcp --locked`.

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
