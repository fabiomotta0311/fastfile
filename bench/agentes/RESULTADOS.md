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
