# Avaliação do FastFile para agentes de IA e harnesses

**Data:** 2026-11-18 · **Branch avaliada:** `chore/profissionalizacao` (`a5927bf`) · **Binário:** `target\release\ff.exe` 0.2.0
**Escopo:** só o que um agente ou harness (Claude Code, Codex, Cursor, Hermes, DSH, Antigravity) usa:
CLI `ff`/`fastfile`, esquema `ff/1`, servidor MCP e os recursos de agente de `ARQUITETURA.md` §3.1–§3.6.

**Objetivo do dono:** que o FastFile seja incorporado a todos os harnesses por ser a melhor ferramenta do segmento.

> **Resumo em uma frase.** A base é boa e já é mais rápida que o ripgrep em grep não indexado (~2× nesta
> máquina), mas hoje um agente **não pode confiar** no FastFile: há três defeitos que dão resposta
> silenciosamente errada ou incompleta (`batch` no CLI, índice de trigramas, segredos dentro do conteúdo), e o
> diferencial do produto (o índice residente da $MFT) **não é usado no caminho padrão dos agentes**. A
> recomendação é corrigir a confiança primeiro (P0, ~1 semana), depois fechar o ciclo
> `achar → ler` com `ff read` (P1), e só então investir em distribuição para harnesses.

---

## Situação após as correções de 2026-11-18

Implementado na mesma branch (sem commit), com testes em `crates/ff-cli/tests/agent_trust.rs`,
`crates/ff-cli/src/secrets.rs` e `crates/ff-content/src/lib.rs`:

| Item | O que mudou | Evidência |
|---|---|---|
| #1 `batch` | O CLI usa o mesmo executor do MCP; também aceita `du`, `map`, `dupes`, `recent` e `mode`/`case_sensitive` no grep | `cli_batch_returns_real_results` |
| #2 trigramas | O índice só descarta arquivos que representa fielmente; o resto é lido. Os trigramas da consulta saem da HIR do regex (`TRIGRAM-QUERY-001`, `TRIGRAM-COVERAGE-001`) | Em `Projects`: 168 = 168 arquivos (antes 40 de 167); `fn main`, `Result<`, `unwrap\(\)`, `colou?r` idênticos com e sem índice; **1,45 s × 2,5 s** sem índice (−42%) |
| #3 segredos no conteúdo | `«redacted:<tipo>»` em `grep`, `diff` e assinaturas do `sym`; `summary.secrets_masked` | `content_masking_*`, `grep_masks_secret_values_by_default` |
| #4 fallback e tempo | Aviso quando `find --all` cai na varredura ou quando ela passa de 2 s; `--timeout`/`FASTFILE_TIMEOUT` (CLI, código 6) e `FASTFILE_MCP_TIMEOUT` (MCP, padrão 45 s) | `find --all` em `C:\Users\fabio` com `--timeout 2`: `TIMEOUT` em 2,1 s (antes 63 s) |
| #5 erros de uso | Envelope `ff/1` com `USAGE_ERROR` e código 64 quando a saída é para máquina | `usage_errors_are_structured` |
| #6 `ff read` | `ff read` / MCP `read_files`: vários trechos por chamada, `next`, `--outline`, PDF/DOCX, UTF-16, binários, segredos bloqueados e mascarados | Contra texto numerado no estilo do Read dos harnesses: −13% (`lib.rs:1-400`), −8% e −6% em outros dois trechos. `--outline`: 11 KB × 121 KB do `lib.rs` (−91%) e 2,2 KB × 16 KB do `read.rs` (−87%) |
| #7 caminhos + `summary` duplicado | Esquema **`ff/2`** (padrão): todo `path` abre da pasta atual, sempre com `/`; grep sem `result.summary`. `ff/1` com `--schema 1`/`FASTFILE_SCHEMA=1` | `ff2_paths_open_from_cwd_and_schema_1_is_kept`; bytes `ff/2` × `ff/1`: grep −33%, find −3%, recent −2% |
| #9 (parte) / #10 (grep) | `ff grep -F`, `-w`, `--hidden` (e `fixed_strings`, `word`, `hidden` no MCP) | `grep_fixed_word_and_hidden_flags` |
| #8 (parte) / #12 / #13 | MCP com JSON compacto, `readOnlyHint`, negociação de versão, `instructions`, erros tipados, `repo_map` com budget padrão de 2 mil tokens | `repo_map` padrão: 36,8 KB → 4,5 KB. `tools/list` subiu de 8,9 KB para 10,3 KB (anotações) |

Continuam abertos (ver §3): paridade de grep
(#9: falta `-A/-B/-C`, `--glob`, multilinha e `cursor`), `--hidden` no `find` (#10), flags uniformes (#11)
e todo o P2/P3.

---

## 0. Como a avaliação foi feita

- Leitura de `crates/ff-cli/src/{lib.rs,mcp.rs,secrets.rs}`, `crates/ff-content/src/trigram.rs`,
  `docs/cli-schema.md`, `docs/mcp.md` e `ARQUITETURA.md` §3.
- Execução real do binário release em dois alvos: o repositório `fastfile` (~160 arquivos) e
  `C:\Users\fabio\Projects` (15 repositórios, ~65 mil arquivos pesquisáveis).
- Comparação com **ripgrep 15.0.0** (o motor das ferramentas Grep/Glob do Claude Code, do Cursor e do Cline).
  `fd` não está instalado nesta máquina e ficou sem medição.
- Tempos: melhor de 3 execuções com cache quente, cronometrados pelo shell (inclui a partida do processo). O
  indexador (`ff-indexer`) **não** estava rodando, que é o caso comum de um agente numa máquina recém-ligada.
- "Tokens" = bytes ÷ 4, a mesma heurística do `--budget`.

Estes números são **evidência exploratória**, não o benchmark de tarefas do §3.6. Para afirmar desempenho em
público, ainda é preciso rodar o §3.6.

---

## 1. O que existe hoje

### 1.1 Inventário verificado

| Capacidade | CLI | MCP | Estado real |
|---|---|---|---|
| Busca por nome (`find`) com glob, regex, fuzzy, ext, tamanho, tipo, `--since` | ✅ | `find_files` | Funciona. Sem `--all` usa **varredura local** (não o índice). |
| Uso de disco (`du`) | ✅ | `disk_usage` | Funciona; rápido. |
| Metadados (`stat`) | ✅ | `file_stat` | Funciona. |
| Recentes (`recent`) | ✅ | `recent_changes` | Funciona (varredura por mtime, não USN). |
| Conteúdo (`grep`) modos content/files/count, `--type`, `--ext` | ✅ | `search_content` | Funciona e é rápido; **sem contexto, multilinha, glob, paginação**. |
| Índice de trigramas para grep | `--build-index` | `build_index` | **Resultados incompletos sem aviso** (§2.1). |
| Diff estruturado (`diff`) | ✅ | `diff_files` | Funciona. |
| Símbolos (`sym`, tree-sitter: rust, python, js, ts, go, cs) | ✅ | `find_symbol` | Funciona; só definições. |
| Mapa do repo (`map`) | ✅ | `repo_map` | Funciona; **saída padrão grande** (21–37 KB). |
| Duplicados (`dupes`) | ✅ | `find_duplicates` | Funciona. |
| Lote (`batch`) | ⚠️ stub | `batch` | **CLI devolve `"status":"ok"` sem resultados**; o MCP funciona. |
| Ler arquivo / trecho | ❌ | ❌ | Não existe. |
| `watch` (USN) | ✅ (humano) | ❌ | Não exposto a agentes. |
| `--budget`, `--compact`, `cursor` | ✅ | ✅ | `cursor` só em find/recent/sym; `grep` não pagina. |
| Proteção de segredos | por **nome de arquivo** | idem | **Mascaramento de conteúdo não existe** (§2.1). |
| Erros estruturados | ✅ (erros de execução) | `TOOL_ERROR` genérico | Erros de **uso** (clap) saem em texto, com o código de saída 2, o mesmo de `TARGET_NOT_FOUND`. |
| Uso remoto via SSH | ✅ por composição | ✅ | Conforme §3.4. |
| Benchmark de tarefas (§3.6) | — | — | Não implementado. |

### 1.2 Comparação com o que os agentes já usam

| Necessidade do agente | Ferramenta atual dos harnesses | FastFile hoje | Veredito |
|---|---|---|---|
| Grep em repo | Grep do harness (ripgrep): modos, `-A/-B/-C`, multilinha, glob, `head_limit`/`offset` | Mais rápido, JSON e segredos, mas sem contexto, glob nem paginação | **Mais rápido, porém incompleto** |
| Achar arquivo por nome no repo | Glob do harness (ordenado por mtime) | `find` equivalente, com tamanho e mtime | **Empate**, com metadados a mais |
| Achar arquivo na **máquina** | `Get-ChildItem -Recurse`, `find /`, `fd` | Com o indexador: ms. **Sem ele: 63 s em `C:\Users\fabio`, sem aviso** | **Diferencial real, mas só com o indexador** |
| Uso de disco | `du`/PowerShell (minutos) | `du`, ms a s | **FastFile vence com folga** |
| "Onde está definido X?" | grep + leitura | `sym` (tree-sitter) | **FastFile vence** (1 chamada, ~550 B) |
| Visão do repo | `ls`/`tree` + leituras | `map` | **FastFile vence**, mas o padrão é caro |
| Ler trecho de arquivo | Read do harness (offset/limit, numerado) | — | **Lacuna** |
| Ler PDF/DOCX | Plugins variados | O código existe (`ff-content/extractors.rs`), mas não é exposto | **Lacuna barata de fechar** |
| MCP de filesystem (`@modelcontextprotocol/server-filesystem`) | `read_text_file`, `read_multiple_files`, `directory_tree`, `search_files`, `get_file_info` (sem índice, sem grep) | Tem índice, grep, sym, du e budget, mas falta a leitura | **FastFile vence**, exceto na leitura |

### 1.3 Medições (cache quente, indexador desligado)

**Grep — FastFile × ripgrep 15, `C:\Users\fabio\Projects`, mesma semântica (`-s` no ff)**

| Consulta | rg (ms) | ff (ms) | rg (bytes) | ff `-c` (bytes) | Resultados iguais? |
|---|---:|---:|---:|---:|---|
| `TODO`, só arquivos (`-l` / `--mode files`) | 5 468 | 2 582 | 11 409 | 12 734 | ✅ 167 = 167 |
| `TODO`, contagem (`-c` / `--mode count`) | 4 920 | 2 837 | 11 763 | 15 930 | ✅ |
| `fn main --type rust`, conteúdo | 456 | 209 | 2 392 | 4 437 | ✅ 25 arquivos / 28 linhas |

- O ff é **~1,7–2,2× mais rápido** que o rg nesta máquina para o mesmo resultado.
- Em **tokens**, o JSON do ff custa **+12% a +85%** a mais que o texto do rg. A tese "econômico em tokens" ainda
  não se confirma no grep. Os motivos estão no §2.3.
- **Padrão divergente:** sem `-s`, o ff ignora maiúsculas e minúsculas. `TODO` vira 846 arquivos contra 167 no
  rg. Um agente treinado em `rg` recebe 5× mais resultado do que espera.

**Find por nome**

| Consulta | Tempo | Observação |
|---|---:|---|
| `find "*.toml"` no repo (walk) | 115 ms | 14 resultados, 1,3 KB compacto |
| `find config.toml` em `Projects` (walk) | 426 ms | rg `--files -g` levou 359 ms |
| `find config.toml -t C:\Users\fabio --all` sem indexador | **63 213 ms** | 3,36 M entradas varridas; **nenhum aviso** de fallback |
| Com indexador (sessão anterior, `ff bench C:`) | P95 22–40 ms | É aqui que está o diferencial |

**MCP**

| Item | Medida |
|---|---|
| `tools/list` (10 ferramentas) | 8 888 B ≈ **2,2 mil tokens fixos por sessão** |
| `find_files` (20 itens): pretty × compacto | 3 058 × 2 133 B (**−30%** se compactar) |
| `search_content`: pretty × compacto | 4 984 × 3 697 B (**−26%**) |
| `repo_map` padrão: pretty × compacto | **36 842** × 21 092 B (**−43%**). Sem budget padrão ≈ 9 mil tokens |

---

## 2. Lacunas e atritos encontrados

### 2.1 Defeitos de confiança (resposta errada ou incompleta sem aviso) — **P0**

1. **`ff batch` no CLI é um stub que mente.** `lib.rs:2151-2282` só ecoa
   `{"command":…,"status":"ok"}` e não executa nada. Isso contradiz `cli-schema.md` §4.5 e o §3.2. O MCP `batch`
   funciona (`execute_batch_json`), então a correção é reaproveitá-lo.
2. **Índice de trigramas devolve resultados incompletos sem aviso.** Depois de `--build-index` em `Projects`
   (65 mil arquivos), `grep TODO --mode files -s` passou a devolver **40 arquivos em vez de 167**, com
   `truncated` ausente e `source:"walk"`. A causa é que o índice tem tetos (`max_files` 50 000,
   `max_total_bytes` 200 MB, `max_file_size` 5 MB em `trigram.rs:39-41`) e a busca considera só os candidatos
   indexados. Como `use_index` é **ligado por padrão**, um único `--build-index` envenena todas as buscas
   seguintes nessa raiz. Além disso, a consulta indexada levou ~1,0 s contra ~2,6 s sem índice: o ganho existe,
   mas não compensa a resposta errada. *(O índice criado por este teste foi apagado depois da medição.)*
3. **Segredos dentro do conteúdo não são mascarados.** O `AGENTS.md` e o §3.2 prometem "mascarar trechos com
   cara de token" por padrão. Num `config.yaml` com `ghp_…`, `AKIA…` e `password: …`, `ff grep` devolveu
   **os três valores em claro**. O `secrets.rs` só classifica **nomes** de arquivo. Há também um vazamento
   pequeno: num `.env` ocultado, `total_matches: 1` com `matches: []` revela que o padrão existe no segredo.
4. **Fallback silencioso e sem teto de tempo.** Sem o indexador, `find --all` num perfil de usuário levou 63 s
   e devolveu `source:"walk"` sem nenhum `warning`. O §3.1 exige avisar. Um harness com timeout de 30–120 s
   aborta a chamada, e o agente conclui que "o ff trava".

### 2.2 Atritos de ergonomia (custam chamadas extras ou erros do agente)

5. **Caminhos relativos ao alvo, não ao diretório de trabalho.** `find lib.rs -t crates` devolve
   `ff-cli\src\lib.rs`. `grep … crates` e `sym … crates/ff-cli/src` fazem o mesmo (`lib.rs`). O agente
   precisa juntar o prefixo sozinho antes de ler, e erra. Ripgrep e o Glob do harness devolvem caminhos
   utilizáveis direto.
6. **Separador `\\` e formatos misturados.** No JSON do Windows cada `\` vira `\\` (mais tokens). O `map`
   usa `/` e devolve `root: "//?/C:/…"` (prefixo UNC vazado); o grep indexado usa `/`, o não indexado usa `\\`.
7. **Pastas ocultas somem sem opção intermediária.** `find "*.yml"` não acha `.github/workflows/*.yml`, e
   `find .cargo` não acha nada. A única saída é `--all`, que também inclui `target/` e `node_modules/` (e
   leva 63 s fora do repo). Falta `--hidden` separado de `--no-ignore`, como no rg e no fd.
8. **O índice residente é contornado no caminho padrão.** `find` sem `--all` usa varredura de propósito
   (§3.2: o IPC não conhece `.gitignore`). Na prática, o agente dentro de um repo nunca usa o diferencial do
   produto. Fora do repo precisa de `--all` e, sem o indexador, cai em 63 s.
9. **Grep sem paridade com o que o agente já sabe usar:** não tem `-A/-B/-C`, multilinha, `--glob`, `-F`
   (literal), `-w` nem paginação (`cursor`). Sob `--budget` marca `truncated: true` mas **não devolve
   cursor**, então o agente não consegue continuar.
10. **Inconsistência de flags entre subcomandos.** Em `find`, o alvo é `-t/--target` (flag); em `grep`,
    `sym`, `map` e `dupes`, é posicional. `-t` significa *target* no `find` e *type* no `grep`. Clap gera
    erro em texto, o agente tenta de novo, e cada tentativa custa uma chamada.
11. **Erros de uso fora do contrato.** `ff grep x -C 2` sai em texto do clap com código **2**, o mesmo de
    `TARGET_NOT_FOUND`, sem o envelope `ff/1`. O agente não distingue "flag não existe" de "caminho não
    existe".
12. **Ordenação de `find` por mtime e não por relevância.** Para `find config`, o nome exato deveria vir
    antes de `config_old_backup/…`. Só `--fuzzy` ranqueia.

### 2.3 Custo em tokens (a tese "econômico" ainda não se paga)

13. **`summary` duplicado no grep:** aparece no topo e de novo em `result.summary` (~180 B por chamada).
14. **MCP sempre indentado.** `mcp.rs:166` usa `to_string_pretty` mesmo com `compact: true`, o que custa
    +26% a +43% de bytes (§1.3).
15. **Campos que o agente raramente usa:** `column_number` em toda linha do grep, `is_dir:false` e `mtime`
    em toda linha do find. Não há opção de projeção (`--fields path,line`).
16. **`map` sem budget padrão:** 21 KB na CLI e 37 KB no MCP. Para "me dê uma visão do repo", 1,5–2 mil
    tokens bastam.
17. **`tools/list` com 2,2 mil tokens fixos** e descrições em português. Os harnesses e a maioria dos
    prompts estão em inglês; a taxa de escolha espontânea da ferramenta (variante "disponível" do §3.6)
    provavelmente sofre com isso. É hipótese a medir, não fato.

### 2.4 Protocolo MCP

18. Responde sempre `protocolVersion: "2024-11-05"`, sem negociar a versão pedida pelo cliente.
19. Não publica `annotations` (`readOnlyHint: true`, `openWorldHint: false`). Clientes como Claude Code e
    Cursor usam isso para dispensar confirmação. Um servidor somente leitura que não se declara assim pede
    permissão à toa.
20. Não tem `structuredContent`/`outputSchema` (o JSON vai dentro de texto), nem `instructions` no
    `initialize` (o lugar certo para dizer ao modelo *quando* preferir o FastFile ao grep nativo).
21. Erros viram `TOOL_ERROR` genérico; os códigos tipados da CLI (`TARGET_NOT_FOUND`, `INVALID_ARGUMENT`)
    se perdem.
22. A documentação diverge do código: `docs/mcp.md` diz "7 ferramentas", lista 8, e o servidor expõe 10
    (`repo_map` e `find_duplicates` não estão documentados).

### 2.5 Higiene

23. Os testes de contrato deixam ~90 arquivos `ff_contract_tri_*.idx` em `%LOCALAPPDATA%\fastfile\trigrams`
    (vistos nesta máquina) e não fazem limpeza.

---

## 3. Propostas priorizadas

Legenda: **Impacto** para agentes e harnesses (A/M/B) · **Esforço** (P = até 1 dia, M = 2–5 dias,
G = mais de 1 semana). Toda afirmação de ganho tem uma **medida** associada (`AGENTS.md`: sem benchmark, sem
promessa).

### P0 — Confiança (sem isto, nenhum harness deveria adotar)

| # | Proposta | Resolve | Imp. | Esf. | Como medir |
|---|---|---|---|---|---|
| 1 | **`batch` real no CLI**, reaproveitando `execute_batch_json` do MCP | §2.1-1 | A | P | Teste de contrato: `batch` = concatenação das chamadas individuais |
| 2 | **Trigramas corretos:** os arquivos não indexados (acima do teto ou modificados depois do índice) entram por varredura, ou a resposta marca `incomplete: true` + `coverage` + aviso. Índice só por opção explícita até ficar à prova | §2.1-2 | A | M | Teste diferencial: `grep --no-index` ≡ `grep` (indexado) em 100 padrões sobre `Projects`; latência p50/p95 de ambos |
| 3 | **Mascaramento de conteúdo ligado por padrão** no `grep`, no `diff` (e no futuro `read`): regras do tipo gitleaks (ghp_, AKIA, xox*, JWT, chaves PEM, `password=`), substituindo por `«redacted:github-token»`; `summary.secrets_masked` | §2.1-3 | A | M | Corpus de teste com N segredos sintéticos: recall ≥ 99%; taxa de falso positivo num repo real; custo de latência (< 5%) |
| 4 | **Aviso de fallback + `--timeout`** (padrão 20 s para agentes) que devolve resultado parcial com `incomplete: true` e a dica "inicie o indexador: `fastfile index --service`" | §2.1-4, §2.2-8 | A | P | `find --all` sem indexador termina em ≤ timeout com JSON válido |
| 5 | **Erros de uso no envelope `ff/1`** (`USAGE_ERROR`, código de saída 64, com sugestão "você quis dizer `--context`?") | §2.2-11 | M | P | Teste de contrato com 10 flags inválidas |

### P1 — Fechar o ciclo do agente e cortar tokens

| # | Proposta | Resolve | Imp. | Esf. | Como medir |
|---|---|---|---|---|---|
| 6 | **`ff read` / MCP `read_files`:** vários arquivos e intervalos de linha por chamada (`a.rs:10-80,b.rs:1-30`), numeração de linhas, detecção de binário e de codificação, mascaramento de segredos, **extração de PDF/DOCX** (já existe em `ff-content`), modo `--outline` (só as assinaturas de símbolos com faixas de linha) e `--around <linha>` | §1.2 (lacuna de leitura) | **A** | M | Tarefa §3.6 "entender a função X": nº de chamadas e tokens com/sem `read --outline`; o alvo é ≥ 30% menos tokens que Read de arquivo inteiro |
| 7 | **Caminhos utilizáveis:** relativos ao **cwd** (ou absolutos) e sempre com `/`; `--relative-to target` vira opção. Corrigir o `//?/` do `map` | §2.2-5/6 | A | P | Teste de contrato: todo `path` devolvido abre com `stat` sem pós-processamento; bytes −2–4% no Windows |
| 8 | **Saída enxuta para agentes:** MCP compacto (−26–43% medidos), grep sem `summary` duplicado, `--fields`, omitir campos com valor padrão (`is_dir:false`), formato agrupado por arquivo no grep (`{"path":…, "lines":[[n,"txt"],…]}`) | §2.3 | A | P–M | **Benchmark de tokens** fixo (5 consultas × 3 repos) contra `rg --json` e `rg` texto: meta é ff ≤ rg texto em bytes para o mesmo conteúdo |
| 9 | **Paridade de grep:** `-A/-B/-C`, `-F`, `-w`, `--glob`, `--multiline`, `--smart-case` (e documentar o padrão; considerar smart-case como o padrão do agente), `cursor` no grep e com `--budget` | §2.2-9 | A | M | Suíte de paridade: 30 invocações do Grep do Claude Code traduzidas, resultado idêntico ao rg |
| 10 | **`--hidden` separado de `--no-ignore`** (e mostrar `.github/` por padrão dentro de um repo git) | §2.2-7 | M | P | Teste: `find "*.yml"` acha os workflows |
| 11 | **Flags uniformes:** alvo sempre posicional e opcional em todos os subcomandos (`-t` só para *type*); aliases compatíveis com rg/fd (`-g`, `-i`, `-l`, `-c`) | §2.2-10 | M | P | Contagem de erros de uso nas transcrições do benchmark §3.6 |
| 12 | **MCP moderno:** negociar `protocolVersion`, `annotations.readOnlyHint`, `structuredContent` + `outputSchema`, `instructions` curtas ("use `fastfile` para buscas fora do repo, disco, símbolos"), erros tipados e descrições em inglês (com `FASTFILE_LANG=pt` opcional) | §2.4 | A | P–M | Variante "disponível" do §3.6: taxa de escolha espontânea do FastFile antes e depois; nº de pedidos de permissão |
| 13 | **`map` com budget padrão** (~2 mil tokens) e expansão progressiva (`--focus crates/ff-cli`) | §2.3-16 | M | P | Bytes do `map` padrão em 3 repos |

### P2 — Diferenciais que só o FastFile pode ter (o fosso competitivo)

| # | Proposta | Por que só o FastFile | Imp. | Esf. | Como medir |
|---|---|---|---|---|---|
| 14 | **Índice residente no caminho padrão:** o indexador aplica as regras de ignore (por raiz de repo, em cache e invalidadas pelo USN quando `.gitignore` muda); o CLI e o MCP usam o IPC por padrão também dentro de repos | Ninguém mais tem $MFT + USN ao vivo | **A** | G | `find`/`recent` p95 com e sem indexador em repo grande (ex.: um monorepo com 500 mil arquivos); a meta é < 50 ms no gate B |
| 15 | **`fastfile changes --since <token>`** a partir do USN: "o que mudou no disco desde o início desta sessão ou do último passo", incluindo arquivos gerados por builds e por outros agentes. Token opaco devolvido em toda resposta | rg/fd/Glob não têm journal; `git status` não vê arquivos ignorados nem fora do repo | A | M | Tarefa §3.6 "o que o build/teste gerou?": chamadas e acerto com e sem o recurso |
| 16 | **Busca por nome na máquina inteira, sem admin, com o indexador como serviço** (instalação que registra o serviço; CLI detecta e orienta) | Gate da Fase 0 | A | M | Tarefas §3.6 "achar todos os `config.toml`" e "arquivos > 1 GB" |
| 17 | **`sym --refs` e `ff outline`:** referências (tree-sitter + grep pré-filtrado pelo índice) e esqueleto do arquivo | Combina índice + tree-sitter num binário só | M | M | Tarefa "renomear X: quem usa?" (acerto × rg) |
| 18 | **Grep com escopo git:** `--changed` (só arquivos modificados ou não rastreados), `--since-commit <rev>` | Útil em revisão de código | M | P | Chamadas por tarefa de revisão |
| 19 | **Contexto para revisão:** `ff diff --git <rev>` estruturado, mascarado e com budget | Diff já existe | B | M | Bytes × `git diff` |

### P3 — Distribuição e adoção (só depois de P0/P1)

| # | Proposta | Imp. | Esf. | Como medir |
|---|---|---|---|---|
| 20 | **Benchmark de tarefas §3.6 executável** (`bench/agent-tasks/`: prompts versionados, gabarito, coleta de tokens e chamadas por harness). É o que transforma "melhor ferramenta" em afirmação defensável | **A** | G | É a própria medida |
| 21 | **`fastfile integrate <harness> --print`**: imprime a configuração MCP e o trecho de `AGENTS.md`/skill para Claude Code, Codex (`config.toml`), Cursor, Hermes e DSH. Só imprime, para manter o binário somente leitura; o instalador aplica com consentimento | A | P | Tempo até a primeira chamada bem-sucedida num harness novo |
| 22 | **Skill/trecho de instruções oficial** ("quando usar `fastfile` em vez do grep nativo"), curto e testado no benchmark | M | P | Variante "disponível" do §3.6 |
| 23 | **Publicação nos registros** (MCP Registry, winget, Homebrew, crates.io), com o `server.json` do MCP Registry | M | M | Instalações |

### Fora de escopo (de propósito)

- **Busca semântica com embeddings:** é pesada, depende de modelo e não cabe no "rápido e somente leitura" agora.
  Reavaliar depois do §3.6.
- **Escrita e edição de arquivos:** continua proibida pelo contrato. Os harnesses já têm Edit/Write próprios.

---

## 4. Recomendação: o que fazer primeiro

**Sprint 1 (≈ 1 semana), "dá para confiar":** P0 #1–#5, mais #7 (caminhos) e a parte barata do #8 (MCP
compacto e grep sem `summary` duplicado). Quase tudo é esforço P. Ao fim, rodar uma suíte diferencial
`ff` × `rg` (resultado idêntico em 100 consultas) e o benchmark de tokens, e anexar os dois ao PR.

**Sprint 2, "fecha o ciclo":** #6 `ff read`/`read_files` com `--outline` e mascaramento, #9 paridade de grep,
#12 MCP moderno. Isso põe o FastFile à frente do MCP de filesystem e no nível do Grep/Read nativos, com
vantagens próprias (segredos, PDF, budget).

**Em paralelo, desde já:** começar o #20 (benchmark de tarefas). Sem ele, "melhor do segmento" é opinião, e
ele também aponta qual diferencial do P2 (#14 índice no caminho padrão ou #15 `changes`) rende mais por hora
investida.

**Primeiro passo concreto sugerido:** corrigir o **trigrama (#2)** e o **`batch` (#1)**. São os dois casos em
que o agente recebe hoje uma resposta errada com cara de certa, o pior defeito possível para uma ferramenta
de agentes.
