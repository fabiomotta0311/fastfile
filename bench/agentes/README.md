# Benchmark de Tarefas de Agente (`bench/agentes/`)

Metodologia, tarefas e resultados comparativos de eficiência e adoção do **FastFile** por agentes de IA
(`docs/ARQUITETURA.md` §3.6 e Issue #24).

---

## 1. Metodologia e Princípios (§3.6)

A tese "o `fastfile` torna os agentes mais rápidos, econômicos e precisos" é aferida com dados empíricos estruturados:

1. **Dois Níveis de Medição:**
   - **Nível 1 — Ferramenta Isolada:** Compara a velocidade bruta e o consumo de tokens das chamadas de busca (`fastfile`
     vs `Get-ChildItem` / `find` / `Select-String` / `ripgrep`), medindo cache frio e quente.
   - **Nível 2 — Tarefa de Agente End-to-End:** Mede a resolução completa de tarefas realistas de desenvolvimento e
     diagnóstico dentro dos harnesses de agente.

2. **Modelo Nivelado:**
   - **Par DeepSeek:** DSH (Windows local) e Hermes (Linux VPS `vps-hermes`).
   - **Par Opus:** Claude Code e Antigravity.
   - **Rigor:** Comparações feitas estritamente **dentro do mesmo harness** (com vs sem `fastfile`). Jamais são feitas
     comparações cruzadas entre modelos diferentes para avaliar a ferramenta.

3. **Três Condições Experimentais por Harness:**
   - **`sem fastfile`:** Ferramentas nativas padrão (PowerShell, CLI de arquivos tradicional).
   - **`fastfile disponível`:** Servidor MCP `fastfile` configurado e ativo. O agente escolhe livremente se adota.
   - **`fastfile obrigatório`:** O prompt instrui explicitamente o agente a utilizar `fastfile`.

4. **Protocolo de Repetições e Métricas:**
   - 5 repetições por célula com cálculo de **mediana**.
   - Sessão nova a cada rodada (sem contaminação de contexto).
   - Métricas:
     - **Tempo total de resolução (s)**
     - **Nº de tool calls (chamadas de ferramenta)**
     - **Consumo aproximado de tokens de entrada/saída**
     - **Taxa de acerto (%)** (verificada contra o gabarito fixo de cada tarefa)

---

## 2. Corpus congelado (CPST B01, issue #85)

Tudo que define "resposta correta" é derivado de dados congelados e reproduzível offline.

- **Fixture:** [`fixtures/ws/`](fixtures/ws) (workspace Rust sintético) + [`fixtures/meta.json`](fixtures/meta.json)
  (relógio de referência, mtimes como "segundos antes do relógio" e arquivos virtuais grandes, como o de >1 GiB).
  `fixtures/.gitattributes` fixa `-text` para os bytes não mudarem com `autocrlf`. Sem arquivos pessoais e sem rede.
- **Gabaritos:** [`gabaritos.json`](gabaritos.json), gerado por `corpus.py` a partir do fixture, com o `fixture_digest`.
  Regras explícitas: janela recente `0 <= agora - mtime < 3600 s`; "maior que 1 GB" é `> 2**30` estrito; `target/`,
  `.git/`, `node_modules/` e segredos (`.env`, `*.jwt`, `*.har`) ficam fora de buscas; workspace Cargo ≠ checkout
  (o `examples/standalone` não é membro).
- **Correção:** `corpus.corrigir(cenario, resposta)` exige conjunto exato; arquivo faltando ou sobrando reprova
  (e a ordem importa no diagnóstico de disco). O formato esperado está em `formato_resposta` de cada tarefa.

| Arquivo | Cenário | Gabarito (resumo) |
|---|---|---|
| [`01_diagnostico_disco.json`](tarefas/01_diagnostico_disco.json) | 5 maiores pastas | tamanhos exatos, em ordem |
| [`02_todos_config_toml.json`](tarefas/02_todos_config_toml.json) | `Cargo.toml` do workspace | raiz + membros (não o standalone) |
| [`03_onde_definido_x.json`](tarefas/03_onde_definido_x.json) | definição de `parse_header` | arquivo, linha e assinatura |
| [`04_arquivos_alterados_1h.json`](tarefas/04_arquivos_alterados_1h.json) | alterados na última hora | borda 3599/3600 s, mtime futuro fora |
| [`05_arquivos_maior_1gb.json`](tarefas/05_arquivos_maior_1gb.json) | maiores que 1 GiB | só o de 2^30 + 1 bytes |
| [`06_grep_repo_grande.json`](tarefas/06_grep_repo_grande.json) | ocorrências de `MAX_LINE` | arquivo:linha, sem `target/` nem `.env` |

### Piloto de correções (10 tarefas) e holdout

As tarefas ficam em [`avaliador/correcoes/`](avaliador/correcoes) (**uso exclusivo do harness**: contêm a mutação e o
teste discriminante). Cada uma tem revisão (`estado_inicial_digest`), teste discriminante, regressões e escopo esperado,
e cobre homônimos (P01/P02), teste em outro módulo (P03, P06–P10) e impacto entre crates (P04/P05).

- O agente recebe só `corpus.visao_agente(tarefa)` e um checkout de `corpus.materializar(tarefa, destino)`: cópia
  limpa **fora do repositório**, sem `.git`, sem `avaliador/`, sem gabaritos e sem o teste discriminante.
  O harness nunca executa o agente dentro deste repositório.
- **Holdout por família:** a unidade de separação é a família (`normalize`, `header`, `money`, `range`, `texto`).
  `corpus.divisao_por_familia` ordena por `sha256("cpst-b01:" + família)` e reserva as primeiras 40% (arredondando
  para cima). Nenhuma família aparece nos dois lados; não se ajusta prompt ou ferramenta olhando o holdout.
- **Conjunto maior para promoção:** proposta, sem tamanho fixado. Rodar o piloto em A/B/C (#87), estimar a variância
  da diferença pareada por tarefa e só então dimensionar o conjunto (novas famílias, mesma regra de divisão).

### Workspace `ledger` e tarefas L01–L12 (issue #120)

O piloto da #88 mostrou que P01–P10 se resolvem em 2–5 chamadas nativas: o workspace `ws` (212 linhas) é pequeno
demais para exploração longa ou saídas grandes, e nenhum gate consegue mostrar ganho de busca/leitura em lote ali.
[`fixtures/ledger/`](fixtures/ledger) é um razão financeiro sintético em Rust com 8 crates (`lg-core`, `lg-parse`,
`lg-rules`, `lg-tax`, `lg-fmt`, `lg-store`, `lg-report`, `lg-cli`), cerca de 5,5 mil linhas de Rust em 57 arquivos, tabelas grandes
(ISO 4217, regiões fiscais sintéticas, 164 regras padrão), documentação e testes de integração. Sem dependências
externas e sem dados pessoais; o `Cargo.lock` fica em `avaliador/correcoes/ledger.lock`, fora do checkout.

- **Tarefas:** `L01`–`L12`, campo `"workspace": "ledger"`. O prompt descreve o **sintoma** na CLI ou no relatório
  e a causa fica em outro crate ou numa tabela grande (`sintoma_distante`, `tabela_grande`, `impacto_entre_crates`).
  O escopo aceito é o arquivo da causa; corrigir no sintoma é `scope_violation`, como na P05.
- **Seis famílias, duas tarefas cada:** `moeda`, `categorias`, `importacao`, `configuracao`, `apresentacao`,
  `consulta`. A divisão é **por workspace** (`corpus.divisao`): incluir o `ledger` não muda a de P01–P10.
  Piloto do ledger: `apresentacao`, `consulta`, `importacao` (L05, L06, L08, L09, L10, L12), que atende o mínimo de
  6 tarefas pareadas do protocolo. Holdout: `categorias`, `configuracao`, `moeda`.
- **Verificação:** `FF_BENCH_CARGO=1` roda `verificar_correcoes.py` (com o defeito só o discriminante falha; com a
  correção a suíte inteira passa) e `FF_BENCH_DOCKER_IMAGE` roda o mesmo no avaliador isolado.

```bash
# Só as tarefas do ledger no piloto
python bench/agentes/run_agent_tasks.py planejar --tarefas L05,L06,L08,L09,L10,L12 --bracos A,B
```

### Avaliador independente de patches (CPST B02, issue #86)

Execute [`grade_patch.py`](grade_patch.py) no **harness confiável**, nunca na sessão ou no checkout
editável do agente. Passe apenas a tarefa B01 e um diff Git de texto relativo à raiz do fixture. O
avaliador verifica o SHA-256 da árvore inicial, aplica o diff em uma cópia própria e permite somente
alterações em fontes existentes autorizadas para a tarefa. Preserva os testes unitários inline congelados.
O teste discriminante só é adicionado **depois** da política de patch; o agente nunca recebe a cópia que o contém.

Pré-requisitos: Python 3.11+, Git e Docker Linux local (Docker Desktop com contêineres Linux no Windows).
Provisione separadamente uma imagem Rust confiável, sem segredos nem volumes declarados, e fixe seu ID
`sha256:…`. O workspace B01 não usa dependências externas. Em um shell POSIX:

```bash
# Imagem já provisionada; este comando não baixa nada.
export FF_BENCH_DOCKER_IMAGE=$(docker image inspect rust:1-slim --format '{{.Id}}')
python bench/agentes/grade_patch.py P01 candidato.diff --image "$FF_BENCH_DOCKER_IMAGE" --timeout 30
python -m unittest discover -s bench/agentes/tests
```

A suíte real com Docker é opt-in via `FF_BENCH_DOCKER_IMAGE`; os testes de patch que não executam
código candidato também rodam sem Docker. O teste de corpus legado `FF_BENCH_CARGO=1` é separado e roda
apenas os fixtures confiáveis com Cargo local, sem testar o isolamento.

Cada suíte roda em um contêiner novo: baseline (só a falha esperada), referência (verde), candidato
(mesmo inventário de testes, todos verdes). O diff não é comparado ao da referência. O executor usa
imagem local com `--pull=never`, Cargo `--offline --locked`, rede desabilitada no Docker, ambiente
limpo, usuário sem privilégios, diretório de entrada só leitura e saída de build descartável em tmpfs.
O timeout vale por suíte. Apenas os contêineres e as cópias criados pela avaliação são removidos.

`fastfile-grade/1` em stdout registra a tarefa, SHA-256 do baseline e do patch, ID da imagem, fase,
motivo e contagens observadas, sem reproduzir logs, código ou caminhos locais. O `stderr` do Docker/Cargo
não é mesclado ao protocolo de resultados em `stdout`, evitando que diagnósticos interrompam a leitura
das linhas `test ...` e do resumo Cargo; os testes de regressão também cobrem stderr volumoso acima do
limite de saída. Códigos: `0` aprovado, `1` reprovado, `2` timeout da suíte, `3` configuração/infraestrutura
ou baseline/referência inválidos. Se o ambiente isolado falhar, não há fallback que execute o candidato no host.

O isolamento protege o host e os arquivos do avaliador; o veredito mede o comportamento testado,
não prova correção geral nem resistência a código nativo escrito para fraudar o próprio processo de
teste. Para patches de origem hostil, não use esse veredito como aprovação automática sem revisão
independente.

### Braços A/B/C e uso por tentativa (CPST B03, issue #87)

[`run_agent_tasks.py`](run_agent_tasks.py) roda as tarefas de correção nos três braços e grava **uma linha
JSONL por tentativa** (`fastfile-attempt/1`), inclusive timeouts, erros de harness e retentativas.

| Braço | O agente vê | Validação antes de gastar |
|---|---|---|
| A | só as ferramentas nativas do harness | nenhum binário do FastFile no `PATH` do agente |
| B | + servidor MCP `fastfile` (catálogo padrão) | `tools/list` sem `repair_context`/`validate_plan` |
| C | + `FASTFILE_EXPERIMENTAL=context,validate_plan` | `tools/list` com as duas experimentais |

Entre os braços só muda o catálogo MCP: mesmo prompt (versionado), modelo, permissões, orçamento por
tentativa e checkout novo materializado da revisão congelada. O FastFile nunca fica no `PATH` do agente,
em nenhum braço (o uso é só via MCP); se o harness dividir diretório com o FastFile, a execução é recusada.
O ambiente do harness não vaza para o braço B: `FASTFILE_EXPERIMENTAL` é removido antes.

- **Ordem balanceada:** em cada repetição as tarefas são embaralhadas (semente registrada) e a ordem dos
  braços gira, para nenhum braço ser sempre o primeiro.
- **Holdout protegido:** sem `--incluir-holdout` só as famílias do piloto rodam (a avaliação final é #100).
- **Sessão limpa:** adaptador `claude-code` usa `claude -p --output-format stream-json`,
  `--no-session-persistence`, `--strict-mcp-config`, `--setting-sources project` e `--max-budget-usd`.
  Variáveis de runtime de uma sessão do Claude Code (`CLAUDECODE`, `CLAUDE_CODE_*`, …) nunca chegam ao
  agente, e o harness real é recusado se o runner for iniciado dentro de uma sessão: rode-o num
  terminal comum (o `claude` recusa sessões aninhadas e elas compartilhariam runtime).
- **Contaminação** invalida a tentativa (`valida: false`, não avaliada): sessão repetida, acesso a
  gabaritos/avaliador, FastFile visível no braço A ou ausente em B/C.
- **Orçamento:** harness real exige `--max-usd` (teto do lote). O lote para antes de uma tentativa que
  possa estourar o teto, e para também se o harness deixar de relatar custo (`custo_inverificavel`):
  nunca se converte bytes em custo.
- **Registro:** status, sessão, modelo, tokens por categoria (`uso`, ou `uso_status: indisponivel`),
  custo relatado, ferramentas por nome, adoção do FastFile (B/C), catálogo validado, SHA-256 do binário
  `ff` e do prompt, patch (só o SHA-256), veredito do `grade_patch.py`, duração (setup, agente,
  avaliação, total), CPU/RSS amostrados com `psutil` quando disponível e disco da área. Sem caminhos
  locais, código ou logs do agente.

```bash
# Ordem das tentativas (sem executar nada)
python bench/agentes/run_agent_tasks.py planejar --repeticoes 5

# Smoke offline: adaptador simulado, sem modelo e sem custo
python bench/agentes/run_agent_tasks.py executar --adaptador simulado --ff-bin target/release/ff \
  --area /tmp/ff-bench-area --saida /tmp/smoke.jsonl --imagem "$FF_BENCH_DOCKER_IMAGE"

# Prova de captura no harness real (1 chamada paga, teto explícito)
python bench/agentes/run_agent_tasks.py sondar --adaptador claude-code --max-usd-tentativa 0.10

# Rodada real: teto do lote e por tentativa autorizados antes
python bench/agentes/run_agent_tasks.py executar --adaptador claude-code --modelo <modelo> \
  --ff-bin target/release/ff --area <fora-do-repo> --saida <arquivo.jsonl> \
  --max-usd <teto> --max-usd-tentativa <teto-por-tentativa> --repeticoes 5 --imagem "$FF_BENCH_DOCKER_IMAGE"
```

Use o binário `ff` recém-compilado: o runner recusa o lote se o catálogo do braço não bater (um
`fastfile` antigo sem as experimentais, por exemplo). Sem `--imagem`, as tentativas ficam `nao_avaliado`.

### Relatório de CPST e protocolo de promoção (CPST B04, issue #88)

[`report_cpst.py`](report_cpst.py) agrega as linhas `fastfile-attempt/1` num relatório
`fastfile-cpst-report/1` (JSON) e numa tabela Markdown reconstruída só a partir desse JSON.

- **CPST** = custo de todas as tentativas válidas (falhas, timeouts e retentativas incluídas) ÷ sucessos.
  Com zero sucessos o CPST é `indefinido`, e o relatório mostra o gasto e 0/N.
- **Custo:** usa o `custo_usd` relatado pelo harness. Sem ele, usa uso × preço **declarado** nas
  premissas. Sem nenhum dos dois, o custo fica `indisponivel`, o total é marcado como incompleto e a
  alegação de CPST é bloqueada. CPU monetizada e amortização de setup entram só se declaradas
  (`premissas/*.json`).
- **Série única:** recusa misturar modelo, versão do harness ou prompt num mesmo relatório.
- **Comparações pareadas por tarefa** (B×A, C×B, C×A): diferença de sucesso e razão de CPST com IC 95%
  por bootstrap sobre tarefas (semente fixa). Tentativas contaminadas ficam fora e tornam o gate
  inconclusivo.
- **Gate:** [`protocolo_promocao.json`](protocolo_promocao.json) fixa margem de não inferioridade,
  redução mínima de CPST, piora máxima do p95 e mínimo de tarefas. Saída: `aprovado`, `reprovado` ou
  `inconclusivo`. O protocolo é fixado **antes** da rodada e só muda com nova versão; o holdout fica para
  a #100.

```bash
python bench/agentes/report_cpst.py tentativas.jsonl --premissas bench/agentes/premissas/claude-code-relatado.json \
  --protocolo bench/agentes/protocolo_promocao.json --json cpst.json --md cpst.md
```

Os resultados históricos do PR #39 ficam em [`RESULTADOS.md`](RESULTADOS.md), marcados como não
reproduzidos, com o motivo de cada série.

### Volume devolvido, releituras e braço de diagnóstico (issue #120)

O runner v2 casa cada `tool_result` do `stream-json` com a chamada que o gerou e grava em `volume`:

| campo | significado |
|---|---|
| `bytes_total`, `bytes_por_ferramenta`, `bytes_fastfile` | bytes UTF-8 devolvidos ao modelo |
| `bytes_ponderado` | `bytes × (chamadas restantes + resposta final)`: cada resposta é relida em todos os turnos seguintes |
| `respostas_grandes` | respostas acima de 8 KiB |
| `arquivos_lidos`, `releituras` | arquivos lidos por `Read` ou `read_files`, e quantas leituras repetiram um arquivo |
| `chamadas_repetidas` | chamadas idênticas (mesmo nome e entrada) |

- **Braço D (diagnóstico):** catálogo do B com `Glob` e `Grep` nativos desligados (`--disallowedTools`). Mostra o
  efeito possível quando o agente usa o FastFile; o B mostra se ele usa. O `Read` continua, porque o `Edit` do
  Claude Code exige ler o arquivo antes. `D_vs_B` aparece no relatório com decisão `diagnostico` e nunca passa pelo
  gate.
- **Relatório:** com linhas v2, `report_cpst.py` acrescenta o volume por braço, a razão pareada do volume ponderado
  (IC 95%) e a tabela tarefa × braço com denominadores, CPST, adoção, volume, releituras e retentativas. Linhas v1 (a
  baseline da #88) geram exatamente o relatório de antes.
- **Ganho só conta com qualidade:** uma redução de volume com mais falhas ou mais releituras não conta como ganho
  (#120). A tabela por tarefa existe para essa leitura.

- **Carregamento do catálogo MCP (`--mcp-carregamento`):**
  - `adiado` é o padrão do Claude Code: o modelo vê só os nomes das ferramentas MCP.
  - `completo` define `ENABLE_TOOL_SEARCH=false` para carregar descrição e esquema logo de início.
  - `toolsearch` libera o `ToolSearch`.
  - O modo vale para todos os braços do lote, entra na linha (`mcp_carregamento`), e o relatório recusa misturar modos.
  - A linha também registra `catalogo_sessao`: quantas ferramentas do FastFile e se o `ToolSearch` foram anunciados à sessão.
- **Sonda do catálogo:** `sondar --ff-bin ...` roda uma chamada no braço B e pergunta ao agente o que ele vê do FastFile.
  O relatório traz `catalogo_ff_sessao`, `toolsearch_no_catalogo`, `cache_creation_input_tokens` e a resposta. É só
  para o operador e não grava JSONL.

```bash
python bench/agentes/run_agent_tasks.py executar --tarefas L05,L06,L08,L09,L10,L12 --bracos A,B,D ...
python bench/agentes/run_agent_tasks.py sondar --adaptador claude-code --modelo sonnet --max-usd-tentativa 0.10 \
  --ff-bin ./target/release/ff.exe --mcp-carregamento completo
```

---

## 3. Execução dos Scripts

- **Benchmark Nível 1 (Ferramentas):**
  ```bash
  python bench/agentes/run_bench_tools.py
  ```
- **Resultados Consolidados:**
  Consulte [`RESULTADOS.md`](RESULTADOS.md).

- **Corpus e gabaritos (offline, sem custo):**
  ```bash
  python -m unittest discover -s bench/agentes/tests
  python bench/agentes/corpus.py verificar   # gabaritos.json confere com o fixture
  FF_BENCH_CARGO=1 python -m unittest discover -s bench/agentes/tests   # confirma os testes discriminantes com cargo
  ```
