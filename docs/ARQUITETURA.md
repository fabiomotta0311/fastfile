# FastFile — Plano de Arquitetura e Execução

> Ferramenta desktop nativa para Windows que junta, numa janela só, o ciclo de trabalho de um dev com
> arquivos: **achar → olhar → comparar → entender o espaço em disco**.
> Inspirações: **Everything** (busca por nome instantânea), **WizTree** (uso de disco), **Agent Ransack**
> (busca em conteúdo + pré-visualização) e **WinMerge** (diff lado a lado).
> Core em **Rust**; UI em **egui**. Mantido majoritariamente por agentes de IA (§0.2).

---

## 0. Princípio reitor — Gate de viabilidade (Fase 0)

**Nada da interface (UI, diff visual, pré-visualização, treemap) começa antes de o índice da MFT passar
no Go/No-Go.** Exceção deliberada: o **CLI para agentes** (§3.1) é um quick win entregue logo após a base,
porque não depende de tela e já é útil sobre a varredura paralela.

O risco técnico do projeto inteiro está numa coisa só: **ler a MFT do NTFS e acompanhar o USN Journal
em Rust com velocidade comparável ao Everything**. Se isso funciona, o resto (busca em conteúdo,
prévia, diff, treemap) é engenharia conhecida. Se não funciona, o produto vira "mais um buscador que
percorre pastas" e perde a razão de existir.

O entregável da Fase 0 é um CLI (`ff`) que prova, de ponta a ponta:

```
abrir volume NTFS → ler a MFT → montar índice em memória → buscar por nome
      → alterar um arquivo no disco → ver o índice atualizar sozinho (USN)
```

### 0.1 Por que egui (e não Tauri, como no Fast365)

A decisão é oposta à do Fast365, e de propósito:

- No Fast365 o conteúdo são mensagens do Teams em HTML rico → web/React renderiza nativamente.
- No FastFile o conteúdo é **texto puro** (código, logs, configs) em fonte monoespaçada, e as listas de
  resultado podem ter **milhões de linhas**.
- `egui` (via `eframe`) lida muito bem com listas virtualizadas, abre em milissegundos e usa pouca RAM —
  que é a promessa de uma ferramenta "fast". Seleção de texto e realce de sintaxe em monoespaçado são
  suficientes para prévia e diff.

**Trade-off assumido:** prévia de formatos ricos (PDF/DOCX renderizados) fica limitada a **texto
extraído**. Não é objetivo renderizar documentos.

**Decisão confirmada pelo dono (2026-11-13), após comparar com Tauri + React:** começar com egui.
A alternativa web ganharia em visual pronto e no diff do Monaco, ao custo de ~60–100 MB de RAM e de
abertura a frio mais lenta. Consequências para o projeto:

- **Visual:** o tema padrão do egui é funcional, não bonito. A interface ganha um **tema próprio**
  (fontes, paleta, espaçamento, cantos, modo escuro) desde a primeira tela — não é polimento opcional.
- **Diff e prévia são nossos:** sem Monaco, o `ff-diff` e o `ff-preview` implementam realce de sintaxe,
  destaque intra-linha e navegação. Já estava no plano; fica registrado que é esforço deliberado.
- **Porta aberta:** a interface consome o núcleo só pela API do indexador (consultas tipadas, resultados
  por janela de linhas visíveis). Se um dia a UI for trocada (Tauri/React ou Slint), o núcleo não muda.

### 0.2 Premissa transversal — AI-Native

Mesma premissa do Fast365: a arquitetura otimiza o **custo de manutenção por agentes** (Hermes, DSH,
Antigravity, Claude). Na prática:

- **Localidade de contexto:** crates e módulos por **capacidade** (índice, busca, conteúdo, prévia,
  diff), não por camada técnica.
- **Abstração só se pagar o próprio custo:** uma trait existe se reduz acoplamento ou torna a validação
  mais barata (ex.: a fonte do índice é trait para o fallback e para testes com volume falso).
- **Sem scaffolding antecipado:** crates das fases seguintes só nascem quando a fase começar.
- **Contratos + testes** em vez de narrativa; benchmarks reproduzíveis para tudo que for promessa de
  velocidade.

Regras operacionais estáveis vivem em [`AGENTS.md`](../AGENTS.md).

---

## 1. Visão de componentes

```
┌───────────────────────────────────────────────────────────────────┐
│  ff-ui (egui, usuário normal)                                     │
│  busca · lista virtualizada · prévia · diff · treemap             │
└──────────────▲────────────────────────────────────────────────────┘
               │ IPC local (named pipe com ACL do usuário)
┌──────────────┴────────────────────────────────────────────────────┐
│  ff-indexer (processo elevado, superfície mínima)                 │
│  lê MFT · acompanha USN · mantém índice · responde consultas      │
└──────────────▲────────────────────────────────────────────────────┘
               │
        volume NTFS bruto (\\.\C:)      fallback: varredura paralela
```

**Por que dois processos:** ler a MFT exige **administrador**. Rodar a interface inteira elevada é ruim
(superfície de ataque, drag-and-drop bloqueado entre níveis de integridade, abrir arquivos no editor do
usuário como admin). Então só o indexador é elevado e faz uma coisa: manter o índice e responder
consultas. Na Fase 0 tudo roda num processo só (o CLI, como admin); a separação vem na Fase 1.

### Crates (nascem por fase)

| Crate | Fase | Responsabilidade |
|---|---|---|
| `ff-index` | 0 | Fonte do índice: leitura da MFT, USN Journal, fallback por varredura; estrutura de dados do índice |
| `ff-search` | 0 | Consulta por nome sobre o índice (substring, curingas, regex, filtros de tamanho/data/extensão) |
| `ff-cli` | 0 | Binário `ff` — o gate da Fase 0 e ferramenta de benchmark |
| `ff-content` | 1 | Busca em conteúdo (motor do ripgrep), pré-filtrada pelo índice |
| `ff-size` | 1 | Agregação de tamanho por pasta (uso de disco) a partir do índice |
| `ff-indexer` | 1 | Processo elevado + IPC + snapshot do índice em disco |
| `ff-ui` | 2 | Interface egui: lista, prévia, diff, treemap |
| `ff-preview` | 2 | Prévia de texto com realce de sintaxe e das ocorrências; detecção de binário |
| `ff-diff` | 2 | Diff de linhas + intra-linha para a visão lado a lado |

---

## 2. Fase 0 — Índice pela MFT (o gate)

### 2.1 Como ler o NTFS

Duas rotas, e o PoC deve **medir as duas** antes de escolher:

1. **`FSCTL_ENUM_USN_DATA`** (API oficial): enumera todas as entradas do volume com número de
   referência, referência do pai e nome. É rápido e simples de implementar, mas **não traz tamanho nem
   datas**.
2. **Leitura bruta da `$MFT`** (abrir `\\.\C:`, `FSCTL_GET_NTFS_VOLUME_DATA`, ler os registros de 1 KB e
   interpretar os atributos `$FILE_NAME` e `$DATA`): traz **tudo** (nome, pai, tamanho, datas), é o
   método do WizTree, mas exige tratar *fixups*, registros de extensão (`$ATTRIBUTE_LIST`), hard links
   (vários `$FILE_NAME`) e arquivos fragmentados da própria `$MFT`.

Crates candidatas a avaliar (não a adotar às cegas): `ntfs`, `mft`, `windows` (FFI Win32). Decisão
registrada como contrato `INDEX-SOURCE-001` com o benchmark que a justifica.

### 2.2 Estrutura do índice (feita para velocidade de varredura)

Busca por nome no estilo Everything é uma **varredura linear muito rápida** sobre nomes compactos, não
uma árvore de busca. Portanto, *struct-of-arrays*:

```
entries:   Vec<Entry>            // id → { parent: u32, name_off: u32, name_len: u16,
                                 //        flags: u16, size: u64, mtime: i64 }
names:     Vec<u8>               // arena com todos os nomes (UTF-8)
names_lc:  Vec<u8>               // arena paralela já em minúsculas (busca sem distinção de caixa)
frn_map:   HashMap<u64, u32>     // número de referência NTFS → id (para aplicar o USN)
```

- Busca = varrer `names_lc` com `memchr`/`memmem` em paralelo (`rayon`), depois filtrar por
  tamanho/data/extensão nos `entries` encontrados.
- Caminho completo é **reconstruído sob demanda** subindo pelos `parent` (não é armazenado).
- Meta de memória: ordem de grandeza de **~100 bytes por arquivo** (a calibrar no benchmark).

### 2.3 Atualização ao vivo (USN Journal)

`FSCTL_QUERY_USN_JOURNAL` + `FSCTL_READ_USN_JOURNAL` em loop: cada registro (criado, apagado,
renomeado, alterado) vira uma operação no índice via `frn_map`. Guardar o último USN processado permite,
na Fase 1, **retomar do snapshot sem reindexar** se o journal não tiver sido truncado.

### 2.4 Fallback (sem admin, exFAT/FAT32, rede)

Varredura paralela das pastas (`ignore` ou `jwalk`) alimentando a **mesma** estrutura de índice, sem
atualização ao vivo (ou com `ReadDirectoryChangesW` depois). É lenta por natureza; o importante é que a
interface funcione igual.

### 2.5 Subcomandos do gate

```
ff index C:                 # indexa o volume, imprime nº de entradas, tempo e memória usada
ff find <padrão> [filtros]  # busca por nome, imprime resultados e latência
ff watch C:                 # mantém o índice vivo pelo USN e imprime as mudanças aplicadas
ff bench C: <padrões...>    # benchmark reproduzível: indexação + latência p50/p95 de consultas
```

### 2.6 Matriz Go/No-Go

| # | Checkpoint | Passa se… (metas a calibrar contra o Everything na mesma máquina) |
|---|---|---|
| A | Indexação | `ff index C:` indexa o volume inteiro em **segundos** (alvo < 10 s), com contagem de entradas próxima à do Everything |
| B | Busca por nome | latência **p95 < 50 ms** para substring sobre o volume inteiro |
| C | Atualização ao vivo | criar/renomear/apagar um arquivo aparece no índice em **< 1 s** via USN |
| D | Tamanhos | soma de tamanhos por pasta bate com o Explorer/WizTree (diferença explicável) |
| E | Memória | índice do volume cabe em memória razoável (alvo < 300 MB para ~2 milhões de entradas) |
| F | Fallback | sem admin, a varredura funciona e produz o mesmo formato de índice |

**Go** = A, B, C e E verdes em duas execuções. **No-Go** = A ou B ordens de grandeza piores que o
Everything sem caminho claro de otimização. D e F podem ser concluídos no início da Fase 1.

---

## 3. Fase 1 — Núcleo completo (headless)

- **`ff-content`** — busca em conteúdo com as crates do ripgrep (`grep-searcher`, `grep-regex`),
  multithread. O diferencial sobre o Agent Ransack: o índice **filtra candidatos antes de abrir
  arquivos** (nome, extensão, tamanho, data, pasta), e arquivos binários são pulados cedo.
- **`ff-size`** — agregação de tamanho por pasta a partir do índice (é só somar subindo pelos
  `parent`). Base do treemap.
- **`ff-indexer`** — processo elevado, superfície mínima, servindo consultas por **named pipe** com ACL
  restrita ao usuário que o iniciou. **Snapshot** do índice em disco (formato próprio, versionado) para
  *cold start* em milissegundos, retomando pelo último USN.

### 3.1 CLI para agentes (interface de primeira classe)

O FastFile tem **dois públicos**: a pessoa (interface egui) e os **agentes de IA** (Claude, DSH, Hermes,
Antigravity), que hoje diagnosticam disco e arquivos percorrendo pastas — lento, caro em tokens e às
vezes sem permissão. O CLI `ff` deixa de ser só ferramenta do gate e vira o jeito de um agente perguntar
coisas ao índice.

**Caso real que motivou:** diagnosticar o que ocupa espaço no disco levou mais de 7 minutos via
PowerShell (varredura recursiva, estouro de tempo, erros de permissão). Com o índice residente, as
mesmas perguntas respondem em milissegundos:

```
ff du C:\ --top 15 --json                     # maiores pastas do volume
ff du %LOCALAPPDATA% --depth 1 --json         # o que pesa dentro de uma pasta
ff find "*.vhdx" --size ">1GB" --json         # arquivos grandes de um tipo
ff find --name target --type dir --json       # todas as pastas target/ com tamanho
ff stat <caminho> --json                      # tamanho, datas, atributos, hard links
ff recent --since 1h --json                   # o que mudou recentemente
ff grep "TODO" --ext rs --json                # busca em conteúdo (ripgrep), resultado estruturado
ff diff a.toml b.toml --json                  # diferenças em blocos estruturados
```

**Contrato do CLI para agentes:**
- **Saída JSON estável e versionada** (`--json`, ou automática quando a saída não é um terminal). O
  esquema tem versão (`"schema": "ff/1"`) e muda só de forma compatível; quebra = nova versão.
- **Econômico em tokens:** resumo primeiro (totais, contagens), detalhes limitados por `--limit`
  (padrão baixo), caminhos opcionalmente relativos à raiz pedida. Nada de despejar 100 mil linhas.
- **Só leitura.** O CLI **não apaga, não move, não altera** nada. Limpeza é decisão da pessoa; o agente
  recebe os números e recomenda.
- **Sem admin para o agente:** as consultas vão ao `ff-indexer` (que é quem roda elevado). Se o
  indexador não estiver rodando, o CLI cai no fallback (varredura) e **avisa** que a resposta é lenta e
  pode estar incompleta.
- **Códigos de saída e erros estruturados** (erro também em JSON), para o agente decidir sem adivinhar.
- **Modo servidor MCP** (`ff mcp`): as mesmas consultas expostas como ferramentas MCP, para os agentes
  chamarem nativamente, sem montar linha de comando.

**Entrega antecipada (quick win, decisão do dono em 2026-11-13).** Como o CLI não depende de
interface, ele sai **logo após a base**, antes do gate, e fica mais rápido a cada etapa **sem mudar o
contrato** (`ff/1`):

| Etapa | Fonte dos dados | Velocidade esperada | Precisa de admin? |
|---|---|---|---|
| v0 — quick win | varredura paralela em Rust (fallback, §2.4) | segundos a dezenas de segundos por volume; muito mais rápido que PowerShell | não |
| v1 — após a MFT (Fase 0) | MFT lida na hora (one-shot) | segundos para o volume | sim (o `ff` detecta e usa se elevado) |
| v2 — após o indexador (Fase 1) | índice residente via IPC | milissegundos | não (quem é elevado é o indexador) |

`ff grep` (motor do ripgrep) e `ff diff --json` (`ff-diff`) **não dependem do índice** e também entram
no quick win; o pré-filtro por índice do `ff grep` chega depois, na Fase 1.

### 3.2 Recursos pensados para agentes (quick win)

Objetivo declarado: o FastFile ser o **mecanismo de busca padrão dos agentes** — não só por velocidade,
mas por gastar menos chamadas e menos tokens para a mesma resposta.

- **Padrões de projeto:** dentro de um repositório, respeitar `.gitignore`/`.ignore` e pular `target/`,
  `node_modules/`, `.git/` e similares por padrão (`--all` inclui tudo). Fora de repositório, o volume
  inteiro.
- **Paridade com o grep que agentes usam:** modos `content` / `files` / `count`, contexto `-A/-B/-C`,
  multilinha, filtro por linguagem (`--type rust`), e glob ordenado por **data de modificação** (mais
  recente primeiro).
- **Paginação estável:** `cursor` no JSON para pedir "a próxima página" da mesma resposta, sem refazer a
  busca.
- **Orçamento de tokens mensurável (`--budget N`):** corta a saída serializada completa (envelope JSON
  inteiro) para caber em teto estrito de $N \times 4$ bytes UTF-8 (tokens estimados). Se nem o envelope
  mínimo couber (ex.: `--budget 1`), retorna `results: []` com `truncated: true` e aviso explicativo sem
  quebrar a sintaxe JSON.
- **Modo compacto (`--compact` / `-c`):** minificação em linha única sem espaçamento redundante, omissão
  de campos derivados (`name`, `mtime_iso`) e caminhos relativizados por padrão para consumo ultrabaixo de
  contexto.
- **Consultas em lote:** `ff batch` (JSON com várias consultas) e ferramenta MCP que aceita uma lista —
  uma chamada em vez de cinco.
- **Proteção de segredos por padrão:** não exibir conteúdo de `.env`, chaves privadas, `*.jwt`, `*.har`
  e afins, e **mascarar** trechos com cara de token/senha nos resultados. `--include-secrets` só por
  pedido explícito da pessoa.

### 3.3 Nomes do comando: `ff` e `fastfile`

Nenhum sistema operacional tem `ff` nativo; os conflitos conhecidos são **aliases pessoais** (Firefox,
fastfetch, wrappers de fzf/ffmpeg). Como **alias ganha do executável** num shell, um agente que rodar
`ff` numa máquina com esse alias abriria outro programa sem perceber. Decisão:

- **`ff`** é o comando principal, para pessoas.
- O mesmo binário é instalado também como **`fastfile`**.
- Documentação para agentes, onboarding e configuração MCP usam **`fastfile`** (inequívoco). O servidor
  MCP se chama `fastfile`.

### 3.4 Uso remoto (VPS e outros servidores)

O FastFile **não implementa SSH nem fala com a rede** — isso preserva a regra de privacidade (§6). Uso
remoto é composição com o SSH que já existe:

```
ssh vps-hermes fastfile find "*.toml" --json      # CLI remoto, JSON volta pelo SSH
"fastfile-vps": { "command": "ssh", "args": ["vps-hermes", "fastfile", "mcp"] }   # MCP sobre SSH
```

No Linux não existe MFT: a fonte é a varredura paralela (velocidade de `fd`/ripgrep, não instantânea).
O ganho é o **mesmo contrato `ff/1`** e os mesmos recursos para agentes nos dois sistemas. Um índice ao
vivo no Linux (`fanotify` para o volume, `inotify` para pastas) é fase própria (§3.5).

### 3.5 Busca avançada para agentes (fase própria, após o quick win)

| Recurso | O que responde | Base técnica |
|---|---|---|
| `ff sym <nome>` | "onde está definido X?" (função, struct, classe) | tree-sitter por linguagem |
| `ff map [pasta]` | visão compacta de um repo: estrutura, linguagens, tamanhos, principais símbolos | `ff sym` + índice |
| `ff find --fuzzy` | nome aproximado (`archmd` → `ARQUITETURA.md`) | correspondência aproximada sobre a arena de nomes |
| `ff dupes` | arquivos duplicados (tamanho + hash) | índice + hash sob demanda |
| índice de trigramas | `ff grep` instantâneo em pastas escolhidas (estilo Zoekt) | índice de conteúdo incremental, após o indexador |
| índice ao vivo no Linux | `ff` rápido também em servidores | `fanotify` / `inotify` |

**Fora de escopo de propósito:** comandos que alteram arquivos e clonar todas as opções do ripgrep.

### 3.6 Benchmark de tarefas de agente

A tese "o `ff` vira padrão dos agentes" é provada com dados, não com opinião:

- **Dois níveis:** (1) teste de ferramenta — `ff` contra `rg`, `fd`, `Get-ChildItem` e as buscas
  internas, cronometrado; (2) teste de **tarefa de agente** — mesma tarefa nos harnesses do dono, com e
  sem o `ff`, medindo **tempo total, nº de chamadas de ferramenta, tokens e acerto**.
- **Modelo nivelado:** DeepSeek no DSH e no Hermes; Opus no Claude Code e no Antigravity — mesma versão
  e mesma configuração de raciocínio dentro de cada par. A conclusão sobre o `ff` vem **dentro do mesmo
  harness** (sem `ff` × com `ff`); nunca comparando modelos diferentes.
- **Duas variantes "com `ff`":** *obrigatório* (o prompt manda usar — ganho máximo) e *disponível* (MCP
  instalado, o agente escolhe — mede adoção espontânea).
- **Rigor:** sessão nova por rodada, prompts fixos versionados, ~5 repetições com mediana, cache de disco
  controlado (frio × frio, quente × quente), resposta correta definida antes. Hermes na VPS conta como
  "Linux", separado das tarefas de disco do Windows.
- **Tarefas iniciais:** diagnóstico de espaço em disco; achar todos os `config.toml` da máquina; "onde
  está definido `X`"; arquivos alterados na última hora; arquivos > 1 GB; grep de texto num repositório
  grande (caso em que se espera **empate de velocidade** com o ripgrep — o ganho ali é em tokens).

## 4. Fase 2 — Interface (egui)

- **Busca e lista:** campo de busca com resultados enquanto digita; lista **virtualizada** (milhões de
  linhas) com ordenação por nome, caminho, tamanho e data; filtros rápidos.
- **Pré-visualização** (`ff-preview`): arquivo aberto por **memória mapeada** (`memmap2`), sem carregar
  arquivos gigantes inteiros; realce de sintaxe (`syntect`); **destaque das ocorrências** da busca em
  conteúdo com navegação entre elas; detecção de binário (mostra hex/resumo em vez de lixo).
- **Diff lado a lado** (`ff-diff`): **Ctrl+clique em dois resultados → abrir comparação**. Diff de
  linhas com a crate `similar` (Patience/Myers), destaque **dentro da linha**, rolagem sincronizada,
  navegação entre diferenças. Caso típico: buscar `config.toml`, achar várias cópias e comparar duas.
- **Treemap** de uso de disco a partir do `ff-size`, com navegação por pasta.

### 4.1 Taxonomia de Tamanhos e Projeção de Recuperação (TM-03, SIZE-METRICS-001)

Para auxiliar na tomada de decisão de liberação de espaço em disco sem induzir o usuário ou agente a erro, o FastFile distingue rigorosamente três grandezas físicas:

1. **Tamanho Lógico (Nominal):** o comprimento do stream de dados do arquivo em bytes (`meta.len()`).
2. **Espaço Alocado (Size on Disk):** a quantidade de bytes realmente alocada em clusters ou blocos físicos no sistema de arquivos. Quando não puder ser obtido ou o sistema de arquivos não o expuser, é tratado como `None` e **nunca apresentado como 0 B**.
3. **Recuperação Estimada:** a projeção conservadora do espaço físico em disco que será efetivamente devolvido ao sistema operacional se o item for excluído:
   - **Hard Links:** Excluir um link individual não desaloca clusters caso existam outros links para o mesmo i-node/FRN (`reclaimable_size = Some(0)`). Em diretórios, os clusters de arquivos compartilhados são somados apenas uma vez e unicamente se todos os links do nó residirem dentro do diretório.
   - **Arquivos Esparsos e Comprimidos:** A recuperação é calculada sobre o espaço alocado em disco, e não sobre o tamanho lógico nominal.
   - **Placeholders de Nuvem (OneDrive Files On-Demand / Offline):** Arquivos desidratados têm espaço local zero; sua exclusão projeta `0 B` de recuperação física local.

O Treemap permite alternar o dimensionamento visual entre os modos `[ 📊 Lógico | 💾 Em disco | ♻️ Recuperável ]` e expõe todas as medidas de forma discriminada no cabeçalho e nos tooltips de inspeção.

## 5. Fase 3 — Extras

- Busca dentro de **PDF/DOCX** por extração de texto.
- Integração com o Explorer (menu de contexto "Buscar aqui" / "Comparar"), atalho global.
- Empacotamento: instalador, registro do indexador como serviço do Windows, atualização.

---

## 6. Segurança e privacidade

- **O índice de nomes de arquivos é dado sensível** (revela projetos, documentos, clientes). Fica
  **somente local**, com ACL restrita ao usuário. **Nenhuma telemetria** nem envio de dados.
- O processo elevado tem **superfície mínima**: não renderiza conteúdo, não abre arquivos do usuário
  para prévia, não executa nada recebido pela IPC além de consultas tipadas.
- A IPC valida o cliente (named pipe com ACL do usuário) e só aceita mensagens de um protocolo fechado.
- Prévia e diff rodam no processo **não elevado**.

## 7. Prior art

| Ferramenta | O que ensina |
|---|---|
| Everything (voidtools, C) | MFT + USN Journal; busca por varredura linear de nomes compactos |
| WizTree | MFT bruta para tamanhos; agregação em memória |
| Agent Ransack (C++) | busca em conteúdo rápida + prévia com ocorrências destacadas |
| WinMerge | diff lado a lado com destaque intra-linha |
| ripgrep / fd (Rust) | motor de busca em conteúdo e varredura paralela de alto desempenho |
| dua-cli / dust (Rust) | uso de disco por varredura (o método lento que o FastFile evita) |

## 8. Resumo de sequência

```
Base ── workspace + CI · estrutura do índice
Quick win ── CLI p/ agentes (ff/1) + MCP + grep/diff, sobre varredura paralela (sem admin)
           + recursos p/ agentes (§3.2) · ff/fastfile · Linux + uso via SSH · benchmark de tarefas
Agentes+ ── ff sym · ff map · fuzzy · dupes · trigramas · índice ao vivo no Linux (§3.5)
Fase 0 (GATE) ── índice MFT + USN + busca por nome ── matriz §2.6 ──▶ Go?  (CLI acelera: v1)
Fase 1 ── conteúdo pré-filtrado · tamanhos incrementais · indexador elevado + IPC + snapshot  (CLI: v2)
Fase 2 ── UI egui: lista · prévia · diff Ctrl+clique · treemap
Fase 3 ── PDF/DOCX · integração Explorer · instalador/serviço
```
