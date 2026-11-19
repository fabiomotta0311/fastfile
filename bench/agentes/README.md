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
