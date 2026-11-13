# FastFile · `ff`

Ferramenta desktop nativa para Windows que junta o ciclo de trabalho de um dev com arquivos:
**achar → olhar → comparar → entender o espaço em disco**. Core em **Rust**, interface em **egui**.

| Faz o que… | …como o |
|---|---|
| Busca por nome instantânea (MFT + USN Journal) | Everything |
| Uso de disco e treemap a partir do mesmo índice | WizTree |
| Busca em conteúdo com pré-visualização e ocorrências destacadas | Agent Ransack |
| **Ctrl+clique em dois resultados → diff lado a lado** | WinMerge |
| **CLI e servidor MCP para agentes de IA** (JSON, só leitura, em milissegundos) | — (novo) |

## Documentos

| Documento | Para quê |
|---|---|
| [`docs/ARQUITETURA.md`](docs/ARQUITETURA.md) | Arquitetura, gate da Fase 0 e o porquê de cada decisão |
| [`AGENTS.md`](AGENTS.md) | Regras "sempre/nunca" para qualquer agente que contribuir |
| [`docs/onboarding.md`](docs/onboarding.md) | Prompts para integrar DSH, Antigravity e Hermes ao ai-memory |

## Status

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

### Fase 0 · Gate
- [#3](https://github.com/fabiomotta0311/fastfile/issues/3) Ler a MFT — `ff index` (A, D, E)
- [#4](https://github.com/fabiomotta0311/fastfile/issues/4) Busca por nome — `ff find` (B)
- [#5](https://github.com/fabiomotta0311/fastfile/issues/5) USN Journal — `ff watch` (C)
- [#6](https://github.com/fabiomotta0311/fastfile/issues/6) Fallback por varredura (F)
- [#7](https://github.com/fabiomotta0311/fastfile/issues/7) Benchmark `ff bench` + comparação com o Everything
- **[#8](https://github.com/fabiomotta0311/fastfile/issues/8) Matriz Go/No-Go → decisão**

### Fase 1 · Núcleo
- [#9](https://github.com/fabiomotta0311/fastfile/issues/9) Busca em conteúdo (ripgrep) pré-filtrada pelo índice
- [#10](https://github.com/fabiomotta0311/fastfile/issues/10) Tamanho por pasta
- [#11](https://github.com/fabiomotta0311/fastfile/issues/11) Indexador elevado + IPC
- [#12](https://github.com/fabiomotta0311/fastfile/issues/12) Snapshot + retomada pelo USN
- [#20](https://github.com/fabiomotta0311/fastfile/issues/20) **CLI para agentes** (`ff du/find/stat/recent/grep/diff --json`)
- [#21](https://github.com/fabiomotta0311/fastfile/issues/21) **Servidor MCP** (`ff mcp`)

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
