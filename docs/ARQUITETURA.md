# FastFile — Plano de Arquitetura e Execução

> Ferramenta desktop nativa para Windows que junta, numa janela só, o ciclo de trabalho de um dev com
> arquivos: **achar → olhar → comparar → entender o espaço em disco**.
> Inspirações: **Everything** (busca por nome instantânea), **WizTree** (uso de disco), **Agent Ransack**
> (busca em conteúdo + pré-visualização) e **WinMerge** (diff lado a lado).
> Core em **Rust**; UI em **egui**. Mantido majoritariamente por agentes de IA (§0.2).

---

## 0. Princípio reitor — Gate de viabilidade (Fase 0)

**Nada de UI, diff, pré-visualização ou treemap começa antes de o índice da MFT passar no Go/No-Go.**

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
Fase 0 (GATE) ── índice MFT + USN + busca por nome (CLI `ff`) ── matriz §2.6 ──▶ Go?
Fase 1 ── conteúdo (ripgrep) · tamanhos · indexador elevado + IPC + snapshot
Fase 2 ── UI egui: lista · prévia · diff Ctrl+clique · treemap
Fase 3 ── PDF/DOCX · integração Explorer · instalador/serviço
```
