# FastFile · `ff`

### Busca instantânea de arquivos para humanos e agentes de IA.
*Instant file search for humans and AI agents. Read-only, token-aware, secret-safe.*

> 🚧 **Em construção.** Os números de desempenho abaixo serão preenchidos pelo
> [benchmark de tarefas de agente](https://github.com/fabiomotta0311/fastfile/issues/24). Até lá,
> são metas, não promessas.

Todo dev vive o mesmo ciclo com arquivos: **achar → olhar → comparar → entender o espaço em disco**.
Hoje isso exige quatro programas. E os agentes de IA, que fazem esse ciclo o tempo todo, não usam
nenhum deles: percorrem pastas pelo shell, gastam tokens com `node_modules/` e podem ler um `.env` sem
perceber.

O FastFile é **um índice instantâneo com duas portas de entrada**: uma janela para você e um CLI/MCP
para os seus agentes.

## Para humanos

| Faz o que… | …como o |
|---|---|
| Busca por nome instantânea (MFT + USN Journal) | Everything |
| Uso de disco e treemap a partir do mesmo índice | WizTree |
| Busca em conteúdo com pré-visualização e ocorrências destacadas | Agent Ransack |
| **Ctrl+clique em dois resultados → diff lado a lado** | WinMerge |

Nativo, em Rust, abrindo em milissegundos — e tudo numa janela só.

## Para agentes

```
fastfile du C:\Users\voce --top 15 --json        # o que ocupa espaço
fastfile find "*.toml" --budget 1500 --json      # cabe no orçamento de tokens
fastfile grep "TODO" --type rust --files --json  # paridade com o grep que agentes já usam
fastfile sym AuthenticatedExecutor --json        # onde está definido
fastfile mcp                                     # o mesmo, como ferramentas MCP
```

### Feito para o orçamento de tokens
- **`--budget`:** a resposta cabe no tamanho que o agente pode pagar, com os melhores resultados primeiro
  e o aviso do que ficou de fora.
- **Resumo antes do detalhe**, paginação por `cursor` em vez de repetir a busca, e **várias consultas
  numa chamada**.
- **Ignora pastas de build** (`target/`, `node_modules/`, `.git/`) por padrão dentro de projetos — onde
  os agentes mais desperdiçam contexto.

### Seguro para dar a um agente autônomo
- **Só leitura.** O `fastfile` não apaga, não move e não altera nada. Buscar não exige dar ao agente um
  shell que pode fazer tudo.
- **Reduz o risco de vazar segredos para o modelo.** Tudo que um agente lê vai para o provedor do
  modelo. Por padrão, o `fastfile` não exibe `.env`, chaves privadas, `*.jwt` e `*.har`, e mascara
  trechos com cara de token. (É detecção por padrões: diminui o risco, não é garantia absoluta.)
- **Sem rede, sem telemetria.** O binário não fala com a internet. Uso remoto é pelo SSH que você já
  tem: `ssh servidor fastfile ...`, ou MCP sobre SSH.
- **Menor privilégio.** O agente não precisa de administrador; só o indexador local é elevado.
- **Seu índice é seu.** A lista de arquivos da máquina fica nela, protegida, e nunca sai.

### O mesmo contrato em todo lugar
O mesmo JSON versionado (`ff/1`) no Windows e no Linux, local ou remoto, e o mesmo servidor MCP para
Claude Code, Antigravity, DSH, Hermes ou qualquer harness compatível.

## Números (a preencher)

| Tarefa de agente | Sem FastFile | Com FastFile |
|---|---|---|
| Diagnóstico de espaço em disco | _—_ | _—_ |
| Achar todos os `config.toml` da máquina | _—_ | _—_ |
| "Onde está definido X" | _—_ | _—_ |
| Grep num repositório grande | _—_ | _—_ |

Medidos dentro do mesmo harness e com o mesmo modelo: tempo total, chamadas de ferramenta, tokens e
acerto. Metodologia em [`docs/ARQUITETURA.md` §3.6](docs/ARQUITETURA.md).

**Comando:** `ff` para uso no dia a dia; o mesmo binário também se chama **`fastfile`**, o nome usado
por agentes e no servidor MCP (evita conflito com aliases como `ff=firefox`).

## Documentos

| Documento | Para quê |
|---|---|
| [`docs/ARQUITETURA.md`](docs/ARQUITETURA.md) | Arquitetura, gate da Fase 0 e o porquê de cada decisão |
| [`AGENTS.md`](AGENTS.md) | Regras "sempre/nunca" para qualquer agente que contribuir |
| [`docs/onboarding.md`](docs/onboarding.md) | Prompts para integrar DSH, Antigravity e Hermes ao ai-memory |

## Status

**Próxima entrega: quick win do CLI para agentes**, logo após a base (#1, #2). Em paralelo,
**Fase 0 — Gate (em aberto).** Antes de qualquer interface, é preciso provar que o índice pela MFT
indexa o volume em segundos, busca em milissegundos e se atualiza sozinho pelo USN
(matriz em `docs/ARQUITETURA.md` §2.6).

## Roadmap (por fase → Issues)

Cada Issue é autocontida (premissas, escopo, critérios de aceite, validação e dependências). Labels
úteis: [`ai-ready`](https://github.com/fabiomotta0311/fastfile/issues?q=is%3Aissue+is%3Aopen+label%3Aai-ready),
[`portavel`](https://github.com/fabiomotta0311/fastfile/issues?q=is%3Aissue+is%3Aopen+label%3Aportavel)
(roda em Linux/VPS) e
[`windows-only`](https://github.com/fabiomotta0311/fastfile/issues?q=is%3Aissue+is%3Aopen+label%3Awindows-only)
(exige NTFS/admin).

### Base
- [#1](https://github.com/fabiomotta0311/fastfile/issues/1) Esqueleto do workspace + CI
- [#2](https://github.com/fabiomotta0311/fastfile/issues/2) Estrutura do índice (struct-of-arrays) + fonte sintética

### ⚡ Quick win · CLI para agentes (logo após a base, sem esperar o gate)
Útil já a curto prazo sobre a varredura paralela (sem admin); fica instantâneo sozinho quando a MFT e o
indexador chegarem — mesmo contrato `ff/1`.
- [#6](https://github.com/fabiomotta0311/fastfile/issues/6) Varredura paralela (fonte da v0)
- [#20](https://github.com/fabiomotta0311/fastfile/issues/20) **CLI para agentes** (`ff du/find/stat/recent --json`)
- [#22](https://github.com/fabiomotta0311/fastfile/issues/22) `ff grep` e `ff diff --json`
- [#21](https://github.com/fabiomotta0311/fastfile/issues/21) **Servidor MCP** (`fastfile mcp`)
- [#23](https://github.com/fabiomotta0311/fastfile/issues/23) Binário Linux + uso remoto (`ssh host fastfile …`, MCP sobre SSH)
- [#24](https://github.com/fabiomotta0311/fastfile/issues/24) **Benchmark de tarefas de agente** (4 harnesses, com e sem fastfile)

### Fase 0 · Gate
- [#3](https://github.com/fabiomotta0311/fastfile/issues/3) Ler a MFT — `ff index` (A, D, E)
- [#4](https://github.com/fabiomotta0311/fastfile/issues/4) Busca por nome — `ff find` (B)
- [#5](https://github.com/fabiomotta0311/fastfile/issues/5) USN Journal — `ff watch` (C)
- [#7](https://github.com/fabiomotta0311/fastfile/issues/7) Benchmark `ff bench` + comparação com o Everything
- **[#8](https://github.com/fabiomotta0311/fastfile/issues/8) Matriz Go/No-Go → decisão**

### Fase 1 · Núcleo
- [#9](https://github.com/fabiomotta0311/fastfile/issues/9) Busca em conteúdo (ripgrep) pré-filtrada pelo índice
- [#10](https://github.com/fabiomotta0311/fastfile/issues/10) Tamanho por pasta
- [#11](https://github.com/fabiomotta0311/fastfile/issues/11) Indexador elevado + IPC
- [#12](https://github.com/fabiomotta0311/fastfile/issues/12) Snapshot + retomada pelo USN

### Agentes+ · Busca avançada para agentes
- [#25](https://github.com/fabiomotta0311/fastfile/issues/25) `ff sym` — onde está definido X
- [#26](https://github.com/fabiomotta0311/fastfile/issues/26) `ff map` — visão compacta de um repositório
- [#27](https://github.com/fabiomotta0311/fastfile/issues/27) Busca aproximada por nome
- [#28](https://github.com/fabiomotta0311/fastfile/issues/28) `ff dupes` — duplicados
- [#29](https://github.com/fabiomotta0311/fastfile/issues/29) Índice de trigramas (grep instantâneo)
- [#30](https://github.com/fabiomotta0311/fastfile/issues/30) Índice ao vivo no Linux

### Fase 2 · Interface (egui)
- [#13](https://github.com/fabiomotta0311/fastfile/issues/13) Busca enquanto digita + lista virtualizada
- [#14](https://github.com/fabiomotta0311/fastfile/issues/14) Pré-visualização com realce
- [#19](https://github.com/fabiomotta0311/fastfile/issues/19) Diff lado a lado (Ctrl+clique)
- [#15](https://github.com/fabiomotta0311/fastfile/issues/15) Treemap

### Fase 3 · Extras
- [#16](https://github.com/fabiomotta0311/fastfile/issues/16) Busca em PDF/DOCX
- [#17](https://github.com/fabiomotta0311/fastfile/issues/17) Integração com o Explorer + atalho global
- [#18](https://github.com/fabiomotta0311/fastfile/issues/18) Instalador + serviço

## Privacidade

O índice de nomes de arquivos é dado pessoal: fica só na máquina, sem telemetria. Nunca commite
índices, snapshots ou saídas de benchmark com caminhos reais.
