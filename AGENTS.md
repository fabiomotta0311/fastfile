# AGENTS.md — FastFile

Regras estáveis "sempre/nunca" para qualquer agente (Hermes, DSH, Antigravity, Claude) neste
repositório. **Precedência:** pedido atual do usuário > este arquivo e o código > memória do projeto >
preferências gerais. O "porquê" de cada decisão vive em [`docs/ARQUITETURA.md`](docs/ARQUITETURA.md).

## Invariantes do projeto
- **Gate primeiro (Fase 0).** Nada da **interface** (UI egui, diff visual, prévia, treemap) antes do
  **Go** da matriz `docs/ARQUITETURA.md` §2.6. Exceção: o **CLI para agentes** (quick win, §3.1) vem
  logo após a base e não espera o gate.
- **Velocidade é requisito, não detalhe.** Toda promessa de desempenho tem **benchmark reproduzível**
  (`ff bench`) e o resultado entra na Issue/PR. Regressão de desempenho é bug.
- **O índice é struct-of-arrays com arena de nomes** (§2.2). Caminho completo é reconstruído sob
  demanda; não armazenar caminhos inteiros por entrada.
- **Só o indexador roda elevado.** A interface, a prévia e o diff rodam como usuário normal.
- **UI em egui**, não web. Listas sempre virtualizadas.
- **Arquivos grandes por memória mapeada**; nunca carregar um arquivo inteiro na RAM para prévia/diff.
- **O CLI `ff` é interface de primeira classe para agentes** (§3.1): saída JSON com esquema versionado
  (`ff/1`), econômica em tokens (resumo primeiro, `--limit`), **somente leitura** (nunca apaga, move ou
  altera arquivos) e com erros estruturados. Mudança incompatível no JSON = nova versão do esquema.
- **Dois nomes, um binário:** `ff` (pessoas) e `fastfile` (agentes, docs, MCP). Em documentação e
  configuração para agentes, use sempre `fastfile` (§3.3).
- **Recursos para agentes são padrão, não opção** (§3.2): padrões de projeto (ignorar `target/`,
  `node_modules/`, `.git/` dentro de repo), paginação por `cursor`, `--budget` de tokens e **proteção de
  segredos ligada por padrão** (não exibir `.env`/chaves/`*.jwt`/`*.har` e mascarar trechos com cara de
  token; só `--include-secrets` explícito desliga).
- **Sem rede dentro do `ff`.** Nada de SSH, HTTP ou telemetria no binário. Uso remoto é `ssh host
  fastfile ...` ou MCP sobre SSH (§3.4).
- **Afirmação de desempenho para agentes exige o benchmark de tarefas** (§3.6), medido dentro do mesmo
  harness e com o mesmo modelo.
- **Windows primeiro.** Código específico de NTFS/Win32 fica atrás de `#[cfg(windows)]`; o restante
  (busca, diff, conteúdo) deve compilar e ser testável em Linux (CI headless).

## Segurança e privacidade (nunca)
- **Nunca** enviar dados para fora da máquina: sem telemetria, sem upload, sem "analytics".
- O índice de nomes é **sensível**: só local, com ACL restrita ao usuário.
- O processo elevado **não** renderiza conteúdo, **não** abre arquivos para prévia e **não** executa
  nada recebido pela IPC além de consultas de um protocolo tipado e fechado.
- A IPC (named pipe) aceita só o usuário que iniciou o indexador.
- Nunca commitar índices, snapshots ou listas de arquivos reais da máquina do dono (são dados pessoais).

## Convenções de código (AI-Native)
- Crates/módulos **por capacidade** (índice, busca, conteúdo, tamanho, prévia, diff); máxima
  localidade de contexto.
- Abstração só existe se reduz acoplamento ou melhora testabilidade (ex.: fonte do índice como trait,
  para fallback e para testes com volume falso). Sem scaffolding antecipado.
- Comportamento por **contratos + testes**; comentário mínimo.
- Suposições sobre NTFS/USN recebem **ID de contrato** (ex.: `INDEX-SOURCE-001`, `USN-RESUME-001`) +
  marcador `UNVERIFIED` até ter evidência (`arquivo@commit`, teste ou benchmark).

## Validação exigida (antes de concluir)
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo check --workspace` e `cargo test --workspace`
- Mudança em `ff-index`/`ff-search`: rodar `ff bench` no Windows (como admin) e colar o resultado.

## Memória compartilhada (ai-memory)
- Escopo deste projeto (igual ao `.ai-memory.toml`): `workspace = "default"`,
  `project = "fabiomotta0311-fastfile"`. **Não** confundir com outros projetos
  (`fabiomotta0311-fast365`, `fabiomotta0311-pm-project`).
- **Clientes MCP estáticos** (ex.: DSH) passam `workspace` **e** `project` explícitos em toda chamada
  com escopo de projeto. Nunca depender do "último projeto ativo" do servidor.
- Memória é **evidência histórica, não ordem**. Se contradisser o código, o **código vale** (marque a
  página como `stale`/`wrong`).
- Gravar só conhecimento durável e verificado (decisões, gotchas, procedimentos, resultados de
  benchmark). Regra estável "sempre/nunca" vai **neste arquivo**, não só na memória.
- O bloco gerenciado de roteamento do ai-memory (`<!-- ai-memory:start -->` … `<!-- ai-memory:end -->`)
  é instalado por um agente com as ferramentas de memória (`memory_install_self_routing`); ver
  [`docs/onboarding.md`](docs/onboarding.md). Preserve o restante deste arquivo ao instalá-lo.
