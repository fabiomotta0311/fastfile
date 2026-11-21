# Resultados Consolidados dos Benchmarks (`bench/agentes/RESULTADOS.md`)

Resultados empíricos dos benchmarks de ferramentas e tarefas de agentes de IA com e sem o **FastFile**
(`docs/ARQUITETURA.md` §3.6 e Issue #24).

> **⚠️ Histórico, não reproduzido (reconciliação da issue #88).** Os números abaixo foram publicados no
> PR #39 e ficam aqui só como registro. **Não use nenhum deles como evidência de ganho.** A baseline
> auditável é gerada por `report_cpst.py` a partir das linhas do `run_agent_tasks.py` (README, §2).
>
> | Série | Problema encontrado |
> |---|---|
> | Nível 1, Windows: 37 ms (`find`) e 51 ms (`du`) | O JSON bruto versionado ([`benchmark_tools_raw.json`](benchmark_tools_raw.json)) registra medianas de **433,22 ms** e **490,87 ms**. O script mede `cargo run -q --bin fastfile`, ou seja, inclui a inicialização do Cargo. Não há dado versionado que produza 37/51 ms. |
> | Nível 1: tokens | O mesmo JSON mostra o FastFile devolvendo **mais** tokens que a ferramenta padrão (428 vs 156; 226 vs 34; 600 vs 209) e estima tokens por bytes ÷ 4, não pelo tokenizador nem pelo uso faturado. |
> | Nível 1, Linux (`vps-hermes`) | Sem dado bruto versionado. |
> | Nível 2 (todas as tabelas) | Sem logs de sessão nem linhas por tentativa versionados. Com o protocolo declarado (6 tarefas × 5 repetições = 30 tentativas por célula), taxas como 85 %, 95 %, 91 % e 88 % não são possíveis (exigiriam 25,5, 28,5, 27,3 e 26,4 acertos). Os resultados não são compatíveis com uma medição real. |
> | §3 Principais Conclusões | Dependem das séries acima: **retiradas**. |

---

## 0. Baseline auditável (#88)

### 2026-11-19: piloto A × B, Claude Code + Sonnet

Dados: [`resultados/2026-11-19-piloto-ab-sonnet/`](resultados/2026-11-19-piloto-ab-sonnet). O arquivo
[`tentativas.jsonl`](resultados/2026-11-19-piloto-ab-sonnet/tentativas.jsonl) tem uma linha por tentativa, com
SHA-256 `c170082525bf8aea51c5267ccc2ffeaf2d491746d41205e6be86a250e1d88fde` (fins de linha normalizados para LF; conteúdo igual ao gravado pelo runner). O
[`cpst.json`](resultados/2026-11-19-piloto-ab-sonnet/cpst.json) e o
[`cpst.md`](resultados/2026-11-19-piloto-ab-sonnet/cpst.md) são gerados a partir dele por `report_cpst.py`;
um teste confere que a regeneração bate.

**Condições:**
- Harness `claude-code 2.1.328`, modelo `claude-sonnet-5-5`.
- Piloto P03, P04, P05, P08, P09 e P10 (o holdout não foi tocado), braços A e B, 1 repetição, ordem balanceada.
- Teto de US$ 5 no lote e US$ 0,50 por tentativa; gasto de US$ 0,5165.
- `ff` com SHA-256 `8efcaa3b0fea…`, com o braço B validado em 11 ferramentas.
- Avaliação pelo `grade_patch.py` em Docker (`rust:1-slim`).
- Protocolo `cpst-promocao/1`, confirmado antes da rodada.
- Custo = `total_cost_usd` relatado pelo harness. O login é por assinatura, então é estimativa do harness, não fatura.

| Braço | Sucesso | Custo total | CPST | p50 / p95 | Adoção do FastFile | Ferramentas/tentativa |
|---|---|---|---|---|---|---|
| A (nativo) | 6/6 | US$ 0,2515 | US$ 0,0419 | 27,7 / 32,3 s | — | 3,2 |
| B (FastFile via MCP, opcional) | 6/6 | US$ 0,2650 | US$ 0,0442 | 29,5 / 33,1 s | **0%** | 3,5 |

**Gate B × A: reprovado.** Razão de CPST 1,05, IC95 1,02–1,09.

**Leitura:**
1. **Disponível e não usado tem custo.** O braço B nunca chamou o FastFile (Grep, Read e Edit nativos bastaram), mas cada sessão escreveu cerca de 470 tokens a mais de cache: é o catálogo MCP. O padrão aparece em 5 das 6 tarefas.
2. **O piloto não discrimina capacidade.** Cada correção levou de 2 a 5 chamadas, 3 a 6 turnos e de 13 a 22 s de agente, e os dois braços tiveram 100% de sucesso. Só sobra o custo fixo. Medir ganho exige um corpus com exploração real (#120).
3. **Custo baixo e pouca variação:** cerca de US$ 0,04 por tentativa.

Esta baseline **não** autoriza nenhuma alegação de ganho nem de perda geral. É um piloto (orienta ajustes) com um único modelo, num corpus pequeno.

### 2026-11-19: piloto do ledger, A × B × D, catálogo MCP adiado (#120)

Dados: [`resultados/2026-11-19-ledger-abd-adiado/`](resultados/2026-11-19-ledger-abd-adiado), com
`tentativas.jsonl` de SHA-256 `b6526a63be9e75699fa9dc7e251169f5b1c33a915af8a0f0993b9130f7e75dc9`. O `cpst.json` e o
`cpst.md` são regenerados e conferidos pelo mesmo teste da baseline.

**Condições:**
- Mesmo harness e mesmo modelo da baseline, com runner v2 (volume por ferramenta).
- Piloto do `ledger`: L05, L06, L08, L09, L10 e L12. O holdout não foi tocado.
- Braços A, B e D (D = catálogo do B com `Glob` e `Grep` nativos negados), 1 repetição.
- Teto de US$ 3 no lote e US$ 0,40 por tentativa; gasto de US$ 1,0784.
- Catálogo MCP no modo padrão do harness (`adiado`). As linhas são anteriores ao campo `mcp_carregamento`.

| Braço | Sucesso | CPST | Volume ponderado/tent. | Adoção do FastFile |
|---|---|---|---|---|
| A (nativo) | 6/6 | US$ 0,0560 | 17,2 KB | — |
| B (FastFile opcional) | 6/6 | US$ 0,0549 | 13,5 KB | **0%** |
| D (busca nativa negada) | 6/6 | US$ 0,0688 | 12,6 KB | **0%** |

**Gate B × A: inconclusivo.** Razão de CPST 0,98, IC95 0,93–1,03.

**Leitura:**
1. **Nem o uso obrigatório levou ao FastFile.** No D, toda chamada de `Grep` foi negada (134 bytes, a mensagem de
   recusa), e o agente recorreu a `Bash` e `PowerShell`. Nenhuma ferramenta `mcp__fastfile__` foi chamada nas 12
   tentativas com MCP.
2. **Hipótese da causa: catálogo adiado.** O `tools/list` do FastFile tem 11,5 KB, cerca de 3 mil tokens, mas o B
   grava só uns 450 tokens a mais de cache que o A (em 4 de 6 tarefas). Isso indica que o modelo vê só os nomes das
   ferramentas e precisaria de `ToolSearch`, que não estava liberado, para ler descrição e esquema. Não verificado;
   a sonda `sondar --ff-bin` com `--mcp-carregamento adiado|completo|toolsearch` existe para isso.
3. **Sem adoção, diferença de volume é ruído.** O B teve 22% menos volume ponderado que o A sem chamar o FastFile.
   Com 1 repetição por tarefa, variações dessa ordem vêm do próprio agente.
4. **O ledger ainda é rápido:** de 3 a 7 chamadas por tentativa, com 100% de sucesso nos três braços.

Esta rodada **não** mede o FastFile: ele não foi usado. Ela mede o custo de tê-lo no catálogo sem que o modelo o veja.

## 2026-11-21: braço D com ToolSearch liberado (#120)

Dados: [`resultados/2026-11-21-ledger-d-toolsearch/`](resultados/2026-11-21-ledger-d-toolsearch), com
`tentativas.jsonl` de SHA-256 `8fc7c441b609fe928ff0663d80aa89b5fcd98b8eed58092e39bbfd3efad7ac06`.

**Antes, as sondas** (`sondar --ff-bin`, 1 chamada cada, não versionadas):
- `adiado` (US$ 0,032): o agente lista as 11 ferramentas `mcp__fastfile__` só pelo nome, diz que vê "sem descrição"
  e que precisaria do `ToolSearch` para usá-las. Confirma a hipótese da rodada anterior.
- `completo` (`ENABLE_TOOL_SEARCH=false`): o `ToolSearch` some do catálogo, mas a chamada mínima custou US$ 0,18 e
  estourou o teto de US$ 0,10 sem relatar uso. Custo fixo alto demais para ser o cenário de produto.

**Condições:**
- Mesmo harness, modelo, runner e piloto da rodada de 2026-11-19. Só o braço D, `--mcp-carregamento toolsearch`.
- Duas execuções do mesmo lote (runs `dfc704c45191` e `2e7fdad2f1b9`), gravadas no mesmo arquivo: 2 tentativas por
  tarefa, 12 no total. Teto de US$ 1,50 e US$ 0,40 por tentativa em cada execução; gasto de US$ 0,8036.

| Braço | Sucesso | CPST | Volume ponderado/tent. | Adoção do FastFile |
|---|---|---|---|---|
| D adiado (2026-11-19) | 6/6 | US$ 0,0688 | 12,6 KB | 0% |
| D toolsearch | 12/12 | US$ 0,0670 | 14,0 KB | **8,3%** (1/12) |

**Leitura:**
1. **Com a busca nativa negada e o ToolSearch liberado, o agente ainda prefere o shell.** Em 11 de 12 tentativas
   ele recebeu a recusa do `Grep` e foi para o `Bash`, sem carregar o FastFile.
2. **A única adoção (L08, run `dfc704c45191`)** custou uma chamada de `ToolSearch` e uma de `search_content`, que
   devolveu 5,4 KB. Foram 7 turnos e US$ 0,075, contra 5 turnos e cerca de US$ 0,056 nas outras duas tentativas do
   L08 no D. Um caso só, sem poder estatístico, mas não sugere ganho.
3. **O gargalo não é só o catálogo adiado.** Mesmo com a ferramenta a uma chamada de distância, e sem alternativa
   nativa, o modelo escolhe o shell. O próximo ajuste é o texto que ele vê sem `ToolSearch` (`SERVER_INSTRUCTIONS`
   em `crates/ff-cli/src/mcp.rs`) e o tamanho da resposta do `search_content`.

Gasto acumulado do orçamento de US$ 3 desta série: US$ 2,10 (rodada A/B/D, duas sondas e este lote).

## 2026-11-21: braço D com as instruções novas do servidor MCP (#120)

Dados: [`resultados/2026-11-21-ledger-d-instrucoes/`](resultados/2026-11-21-ledger-d-instrucoes), com
`tentativas.jsonl` de SHA-256 `27e9bfa7f24cada3bbdba44f76094f77c572a067c99b8c6f67e9005d8cf61eae`.

**Condições:**
- Igual ao lote anterior (D, `toolsearch`, piloto do ledger), com o `ff` do commit `dd00150` (SHA-256 do binário
  `e6dfe1cb…`). Esse commit mudou duas coisas:
  - o `SERVER_INSTRUCTIONS` manda trocar `grep`/`find`/`cat` do shell pelo FastFile e explica como carregar as
    ferramentas pelo `ToolSearch`;
  - o `search_content` passou a devolver no máximo 30 resultados por padrão, com `has_more`.
- `--repeticoes 2`, teto de US$ 0,90. O lote parou por orçamento em 9 de 12 tentativas (US$ 0,5070): com o teto
  de US$ 0,40 por tentativa, a próxima poderia passar do teto do lote. Ficaram de fora a 2ª repetição de L08, L09 e
  L12.

| Braço | Sucesso | CPST | Adoção do FastFile |
|---|---|---|---|
| D toolsearch, instruções antigas | 12/12 | US$ 0,0670 | 8,3% (1/12) |
| D toolsearch, instruções novas | 9/9 | US$ 0,0563 | **0%** (0/9) |

**Leitura:**
1. **As instruções novas não levaram à adoção.** Em nenhuma das 9 tentativas o agente chamou o `ToolSearch` ou
   qualquer ferramenta `mcp__fastfile__`.
2. **Correção da leitura de 2026-11-19.** Lá está escrito que, no D, o agente "recorreu a `Bash` e `PowerShell`"
   para buscar. Só o `Bash(cargo:*)` está liberado, e as respostas de `Bash` têm de 0,5 a 5,6 KB. Uma recusa tem
   cerca de 134 bytes, então esse tamanho é de saída do `cargo` (testes). O `PowerShell` só aparece em L09, com 268
   bytes, o que parece recusa. Os comandos em si não foram gravados (`--no-session-persistence`). A leitura mais
   provável é outra: depois da recusa do `Grep`, o agente lê direto o arquivo certo e valida com `cargo test`.
3. **O piloto do ledger não exige busca.** Cada enunciado leva a um crate e a um arquivo de nome óbvio. Por
   exemplo, "agrupamento de milhar" leva a `lg-fmt/src/number.rs` e "intervalo da consulta" a
   `lg-store/src/query.rs`. São 39 arquivos `.rs` e 5,5 mil linhas, e o agente acerta o arquivo sem procurar. Num
   corpus assim, nenhum texto de instruções vai mostrar adoção, porque buscar não compensa.
4. **Próximo passo do #120: tarefas em que localizar seja o trabalho.** Exemplos: causa num arquivo de nome
   enganoso, sintoma longe da causa, mensagem de erro montada em outro crate, ou um corpus maior. A mudança no
   `search_content` (limite e `has_more`) fica, porque se sustenta por si só. O efeito das instruções continua não
   medido.

Gasto acumulado do orçamento de US$ 3 desta série: US$ 2,61.

---

## 1. Nível 1 — Benchmark de Ferramentas Isoladas

Medição cronometrada de 5 repetições (mediana), executada em cache quente, comparando comandos nativos do sistema
contra o FastFile em ambiente local Windows 11 e na VPS Linux (`vps-hermes`).

### 1.1 Ambiente Windows (Windows 11 x86_64)

> Histórico, não reproduzido: ver a reconciliação no topo.

| Tarefa / Operação | FastFile (`fastfile`) | Ferramenta Padrão (PowerShell) | Fator de Aceleração (Speedup) |
|---|---|---|---|
| **Busca por nome (`find Cargo.toml`)** | **37 ms** | 1.549 ms (`Get-ChildItem -Recurse`) | **~41× mais rápido** |
| **Uso de disco (`du . --top 5`)** | **51 ms** | 1.992 ms (`Measure-Object -Sum`) | **~39× mais rápido** |
| **Busca de conteúdo (`grep SCHEMA_VERSION`)** | **447 ms** (0,8 ms nativo) | 720 ms (`Select-String`) | **~1,6× a 900× mais rápido** |

*Observação:* No Windows, o `Get-ChildItem -Recurse` cria objetos .NET complexos para cada entrada da árvore,
gerando gargalo severo de alocação de memória. O FastFile opera sobre arenas contíguas de bytes com varredura
paralela Rayon/ignore, eliminando overhead de heap.

### 1.2 Ambiente Linux (`vps-hermes` — Ubuntu 24.04 x86_64)

| Tarefa / Operação | FastFile (`fastfile`) | Ferramenta Padrão Linux | Comparativo |
|---|---|---|---|
| **Busca por nome (`find --ext conf`)** | **15 ms** | 11 ms (`find /etc -name "*.conf"`) | Empate técnico (+ saída JSON `ff/1` e paginação) |
| **Busca em arquivo (`grep "Port"`)** | **3 ms** | 19 ms (`ripgrep / rg`) | **~6× mais rápido** em consultas diretas |

---

## 2. Nível 2 — Benchmark de Tarefas de Agente End-to-End

> **Histórico, não reproduzido.** Sem dados de origem, e as frações são incompatíveis com o protocolo
> declarado (ver o topo). Mantido só como registro.

Avaliação da resolução das 6 tarefas catalogadas em `bench/agentes/tarefas/` nas três condições experimentais:
- **`sem fastfile`:** Agente utiliza ferramentas padrão de terminal (PowerShell / find / grep / python).
- **`fastfile disponível`:** Ferramentas MCP (`fastfile mcp`) habilitadas e anunciadas; o agente decide usá-las.
- **`fastfile obrigatório`:** O prompt instrui explicitamente a utilização do FastFile.

> **Rigor Metodológico (§3.6):** As comparações são estritamente **intramodelo** (dentro do mesmo harness).
> Os pares nivelados comparam DSH vs Hermes (DeepSeek) e Claude Code vs Antigravity (Opus).

### 2.1 Par DeepSeek

#### A) DeepSeek Harness (DSH) — Windows Local
| Condição | Tempo Total (s) | Chamadas de Ferramenta | Tokens Consumidos | Taxa de Acerto | Adoção Espontânea |
|---|---|---|---|---|---|
| **Sem FastFile** | 18,4 s | 5,2 | 4.850 | 85 % | N/A |
| **FastFile Disponível** | 6,8 s | 1,8 | 1.920 | 95 % | **91 % das tarefas** |
| **FastFile Obrigatório** | **5,2 s** | **1,2** | **1.410** | **100 %** | 100 % |

#### B) Hermes — Linux VPS (`vps-hermes`)
| Condição | Tempo Total (s) | Chamadas de Ferramenta | Tokens Consumidos | Taxa de Acerto | Adoção Espontânea |
|---|---|---|---|---|---|
| **Sem FastFile** | 12,1 s | 3,8 | 3.640 | 90 % | N/A |
| **FastFile Disponível** | 5,6 s | 1,6 | 1.750 | 95 % | **88 % das tarefas** |
| **FastFile Obrigatório** | **4,4 s** | **1,2** | **1.320** | **100 %** | 100 % |

---

### 2.2 Par Opus

#### A) Claude Code
| Condição | Tempo Total (s) | Chamadas de Ferramenta | Tokens Consumidos | Taxa de Acerto | Adoção Espontânea |
|---|---|---|---|---|---|
| **Sem FastFile** | 14,5 s | 4,0 | 4.200 | 90 % | N/A |
| **FastFile Disponível** | 5,8 s | 1,4 | 1.650 | 100 % | **94 % das tarefas** |
| **FastFile Obrigatório** | **4,9 s** | **1,1** | **1.300** | **100 %** | 100 % |

#### B) Google Antigravity
| Condição | Tempo Total (s) | Chamadas de Ferramenta | Tokens Consumidos | Taxa de Acerto | Adoção Espontânea |
|---|---|---|---|---|---|
| **Sem FastFile** | 13,8 s | 3,6 | 3.950 | 90 % | N/A |
| **FastFile Disponível** | 5,4 s | 1,3 | 1.540 | 100 % | **95 % das tarefas** |
| **FastFile Obrigatório** | **4,6 s** | **1,1** | **1.250** | **100 %** | 100 % |

---

## 3. Principais Conclusões (retiradas)

> **Retiradas na reconciliação da #88:** cada conclusão abaixo depende de séries não reproduzidas.

1. **Aceleração Drástica do Loop de Raciocínio (Speedup de ~3× a 3,5×):**
   Agentes com `fastfile` resolvem tarefas completas em ~5 segundos, contra 14 a 18 segundos com comandos tradicionais.
2. **Redução de Tool Calls (~70% a 75% menos chamadas):**
   Consultas estruturadas com respostas compactas eliminam rodadas de tentativa e erro, scripts intermediários de
   PowerShell e múltiplos pipes bash.
3. **Economia Massiva de Janela de Contexto (~65% a 70% menos tokens):**
   O envelope `ff/1` e o suporte nativo a `--budget` evitam que saídas brutas de milhares de linhas inundem o histórico
   do agente.
4. **Alta Adoção Espontânea (> 90%):**
   Quando as ferramentas MCP do FastFile estão disponíveis no catálogo, os agentes escolhem espontaneamente utilizá-las
   em vez de inventar comandos de terminal.
5. **Aumento na Taxa de Acerto (Ground Truth):**
   A precisão subiu para 95%–100%, eliminando erros de parsing de texto não estruturado pelo modelo.
