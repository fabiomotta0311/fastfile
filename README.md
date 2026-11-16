# FastFile · `ff`

### Busca instantânea de arquivos para humanos e agentes de IA.
*Instant file search for humans and AI agents. Read-only, token-aware, secret-safe.*

[![CI](https://github.com/fabiomotta0311/fastfile/actions/workflows/ci.yml/badge.svg)](https://github.com/fabiomotta0311/fastfile/actions)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-APACHE)
[![Version](https://img.shields.io/badge/version-v0.1.0-brightgreen.svg)](https://github.com/fabiomotta0311/fastfile/releases/tag/v0.1.0)

Todo dev vive o mesmo ciclo com arquivos: **achar → olhar → comparar → entender o espaço em disco**.
Hoje isso exige quatro programas. E os agentes de IA, que fazem esse ciclo o tempo todo, não usam
nenhum deles: percorrem pastas pelo shell, gastam tokens com `node_modules/` e podem ler um `.env` sem
perceber.

O FastFile é **um índice instantâneo com duas portas de entrada**: uma janela nativa para você e um CLI/MCP
otimizado para os seus agentes.

## Instalação e Uso Rápido

### Windows
```powershell
# Instalação automática com menus do Explorer, atalhos e PATH
.\scripts\install.ps1 -BuildRelease

# Abrir a interface gráfica (egui zinc/indigo com tabela virtualizada, prévia, treemap e diff)
ff-ui
# Ou use o atalho global do Windows a qualquer momento: Ctrl+Alt+F ou Win+Shift+F

# Usar a CLI rápida
ff find "*.rs"
ff du . --top 10
ff grep "fn main" --type rust
```

### Linux / Headless / VPS
```bash
# Compilar e instalar os binários CLI e MCP
cargo build --release
sudo cp target/release/ff target/release/fastfile /usr/local/bin/

# Uso remoto via SSH por agentes
ssh host fastfile du /var/log --top 10 --json
```

### Servidor MCP (Model Context Protocol)
Basta adicionar a configuração ao seu cliente MCP (Claude Code, Antigravity, Cursor, Hermes):
```json
{
  "mcpServers": {
    "fastfile": {
      "command": "fastfile",
      "args": ["mcp"]
    }
  }
}
```

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

## Números (Benchmark de Tarefas de Agente)

| Tarefa de agente | Sem FastFile | Com FastFile | Ganho / Speedup |
|---|---|---|---|
| Diagnóstico de espaço em disco | 18,4 s (4.850 tokens, 5 calls) | **5,2 s** (1.410 tokens, 1 call) | **~3,5× mais rápido, -70% tokens** |
| Achar todos os `Cargo.toml` do workspace | 14,5 s (4.200 tokens, 4 calls) | **4,9 s** (1.300 tokens, 1 call) | **~3,0× mais rápido, -69% tokens** |
| "Onde está definido X" | 13,8 s (3.950 tokens, 4 calls) | **4,6 s** (1.250 tokens, 1 call) | **~3,0× mais rápido, -68% tokens** |
| Grep num repositório grande | 12,1 s (3.640 tokens, 4 calls) | **4,4 s** (1.320 tokens, 1 call) | **~2,8× mais rápido, -64% tokens** |

Medidos dentro do mesmo harness e com o mesmo modelo: tempo total, chamadas de ferramenta, tokens e
acerto. Relatório completo em [`bench/agentes/RESULTADOS.md`](bench/agentes/RESULTADOS.md) e metodologia em [`docs/ARQUITETURA.md` §3.6](docs/ARQUITETURA.md).

**Comando:** `ff` para uso no dia a dia; o mesmo binário também se chama **`fastfile`**, o nome usado
por agentes e no servidor MCP (evita conflito com aliases como `ff=firefox`).

## Documentos

| Documento | Para quê |
|---|---|
| [`docs/ARQUITETURA.md`](docs/ARQUITETURA.md) | Arquitetura, gate da Fase 0 e o porquê de cada decisão |
| [`AGENTS.md`](AGENTS.md) | Regras "sempre/nunca" para qualquer agente que contribuir |
| [`docs/mcp.md`](docs/mcp.md) | Guia do servidor MCP (`fastfile mcp`) e configuração de clientes |
| [`docs/remoto.md`](docs/remoto.md) | Guia de uso remoto (SSH e MCP sobre SSH) em servidores Linux |
| [`docs/onboarding.md`](docs/onboarding.md) | Prompts para integrar DSH, Antigravity e Hermes ao ai-memory |

## Status

**Versão 0.1.0 — 100% Concluída.** Todas as 30 issues do projeto foram entregues, validadas com
benchmarks reproduzíveis e aprovadas na esteira contínua de CI (Linux e Windows):
- **Fase 0 (Core Windows/NTFS + USN + Gate):** Concluída com **Go** unânime na matriz.
- **Trilha M-Agentes (#20 a #30):** CLI econômica em tokens (`ff/1`), servidor MCP, busca semântica e suporte Linux/SSH.
- **Fase 1 (Núcleo Completo / Headless):** Agregação de espaço `du`, busca `grep`, daemon elevado IPC e snapshots com retomada rápida.
- **Fase 2 (Interface egui + Recursos Visuais):** Shell desktop `ff-ui`, tabela virtualizada, realce syntect, treemap squarified e diff side-by-side.
- **Fase 3 (Polimento + Distribuição Windows):** Extração em PDF/DOCX, menu de contexto no Explorer, atalho global do sistema, serviço Windows e instaladores.

## Roadmap (por fase → Issues)

Todas as issues foram concluídas e fechadas.

### Base
- [x] [#1](https://github.com/fabiomotta0311/fastfile/issues/1) Esqueleto do workspace + CI
- [x] [#2](https://github.com/fabiomotta0311/fastfile/issues/2) Estrutura do índice (struct-of-arrays) + fonte sintética

### ⚡ Quick win · CLI para agentes (logo após a base, sem esperar o gate)
- [x] [#6](https://github.com/fabiomotta0311/fastfile/issues/6) Varredura paralela (fonte da v0)
- [x] [#20](https://github.com/fabiomotta0311/fastfile/issues/20) **CLI para agentes** (`ff du/find/stat/recent --json`)
- [x] [#22](https://github.com/fabiomotta0311/fastfile/issues/22) `ff grep` e `ff diff --json`
- [x] [#21](https://github.com/fabiomotta0311/fastfile/issues/21) **Servidor MCP** (`fastfile mcp`)
- [x] [#23](https://github.com/fabiomotta0311/fastfile/issues/23) Binário Linux + uso remoto (`ssh host fastfile …`, MCP sobre SSH)
- [x] [#24](https://github.com/fabiomotta0311/fastfile/issues/24) **Benchmark de tarefas de agente** (4 harnesses, com e sem fastfile)

### Fase 0 · Gate
- [x] [#3](https://github.com/fabiomotta0311/fastfile/issues/3) Ler a MFT — `ff index` (A, D, E)
- [x] [#4](https://github.com/fabiomotta0311/fastfile/issues/4) Busca por nome — `ff find` (B)
- [x] [#5](https://github.com/fabiomotta0311/fastfile/issues/5) USN Journal — `ff watch` (C)
- [x] [#7](https://github.com/fabiomotta0311/fastfile/issues/7) Benchmark `ff bench` + comparação com o Everything
- [x] **[#8](https://github.com/fabiomotta0311/fastfile/issues/8) Matriz Go/No-Go → decisão (Go)**

### Fase 1 · Núcleo
- [x] [#9](https://github.com/fabiomotta0311/fastfile/issues/9) Busca em conteúdo (ripgrep) pré-filtrada pelo índice
- [x] [#10](https://github.com/fabiomotta0311/fastfile/issues/10) Tamanho por pasta
- [x] [#11](https://github.com/fabiomotta0311/fastfile/issues/11) Indexador elevado + IPC
- [x] [#12](https://github.com/fabiomotta0311/fastfile/issues/12) Snapshot + retomada pelo USN

### Agentes+ · Busca avançada para agentes
- [x] [#25](https://github.com/fabiomotta0311/fastfile/issues/25) `ff sym` — onde está definido X
- [x] [#26](https://github.com/fabiomotta0311/fastfile/issues/26) `ff map` — visão compacta de um repositório
- [x] [#27](https://github.com/fabiomotta0311/fastfile/issues/27) Busca aproximada por nome
- [x] [#28](https://github.com/fabiomotta0311/fastfile/issues/28) `ff dupes` — duplicados
- [x] [#29](https://github.com/fabiomotta0311/fastfile/issues/29) Índice de trigramas (grep instantâneo)
- [x] [#30](https://github.com/fabiomotta0311/fastfile/issues/30) Índice ao vivo no Linux

### Fase 2 · Interface (egui)
- [x] [#13](https://github.com/fabiomotta0311/fastfile/issues/13) Busca enquanto digita + lista virtualizada
- [x] [#14](https://github.com/fabiomotta0311/fastfile/issues/14) Pré-visualização com realce
- [x] [#19](https://github.com/fabiomotta0311/fastfile/issues/19) Diff lado a lado (Ctrl+clique)
- [x] [#15](https://github.com/fabiomotta0311/fastfile/issues/15) Treemap

### Fase 3 · Extras
- [x] [#16](https://github.com/fabiomotta0311/fastfile/issues/16) Busca em PDF/DOCX
- [x] [#17](https://github.com/fabiomotta0311/fastfile/issues/17) Integração com o Explorer + atalho global
- [x] [#18](https://github.com/fabiomotta0311/fastfile/issues/18) Instalador + serviço

## Privacidade

O índice de nomes de arquivos é dado pessoal: fica só na máquina, sem telemetria. Nunca commite
índices, snapshots ou saídas de benchmark com caminhos reais.
