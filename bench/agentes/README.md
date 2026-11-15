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

## 2. Catálogo de Tarefas Versionadas (`tarefas/`)

As tarefas contam com prompts imutáveis e resposta correta (ground truth) definida previamente:

| Arquivo | Tarefa | Resposta Esperada (Ground Truth) |
|---|---|---|
| [`01_diagnostico_disco.json`](tarefas/01_diagnostico_disco.json) | Diagnóstico de espaço em disco | Top 5 pastas ordenadas por tamanho e totais exatos |
| [`02_todos_config_toml.json`](tarefas/02_todos_config_toml.json) | Todos os `Cargo.toml` do workspace | Todos os 7 arquivos de configuração das crates e raiz |
| [`03_onde_definido_x.json`](tarefas/03_onde_definido_x.json) | Onde está definido X (`run_mcp_server`) | `crates/ff-cli/src/mcp.rs` e assinatura da função |
| [`04_arquivos_alterados_1h.json`](tarefas/04_arquivos_alterados_1h.json) | Arquivos alterados recentemente | Arquivos com timestamp recente filtrados por duração |
| [`05_arquivos_maior_1gb.json`](tarefas/05_arquivos_maior_1gb.json) | Arquivos maiores que 1 GB | Resposta negativa imediata sem varredura completa |
| [`06_grep_repo_grande.json`](tarefas/06_grep_repo_grande.json) | Grep textual no projeto (`SCHEMA_VERSION`) | Ocorrências em `schema.rs` e `mcp.rs` |

---

## 3. Execução dos Scripts

- **Benchmark Nível 1 (Ferramentas):**
  ```bash
  python bench/agentes/run_bench_tools.py
  ```
- **Resultados Consolidados:**
  Consulte [`RESULTADOS.md`](RESULTADOS.md).
