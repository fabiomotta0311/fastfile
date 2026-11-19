# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e versionamento
[SemVer](https://semver.org/lang/pt-BR/). O esquema JSON do CLI (`ff/1`) tem versionamento próprio
(ver [`docs/cli-schema.md`](docs/cli-schema.md)).

## [Não lançado]

### Alterado
- **`validate_plan` fora do catálogo MCP padrão:** passa a exigir `FASTFILE_EXPERIMENTAL=validate_plan`
  no ambiente do servidor, como o `repair_context`. Sem o opt-in a ferramenta não aparece em `tools/list`
  e a chamada devolve `UNKNOWN_TOOL`. O catálogo padrão volta a 11 ferramentas, o braço "FastFile atual"
  do benchmark CPST (#87) deixa de expor a capacidade candidata e o esquema não custa tokens em toda sessão.

### Adicionado
- **Plano de validação experimental (CPST):** `fastfile validate-plan --experimental` sugere como validar
  uma mudança Rust a partir do checkout Git (HEAD ou `--base` → índice, índice → working tree e untracked),
  com `coverage` por camada, `first_check`, `additional`, `broadening_reasons`, `argv` estruturado e
  evidência (teste, manifesto ou suíte conhecida). O mesmo plano (`ff/validate-plan/experimental-2`) é
  exposto no MCP em `validate_plan`, com opt-in explícito (`experimental: true`) e `budget` que trunca
  **ampliando** a sugestão (`budget_truncated`, `coverage.complete: false`) em vez de estreitá-la.
  Nada é executado: `executed: false` e `evidence_status: "not_executed"` são invariantes, e só o harness
  roda os testes. Alteração em configuração, renomeação/remoção, código gerado, caminho excluído do escopo,
  manifesto ambíguo ou dependência local não resolvida caem na suíte do workspace; o workflow CI é usado
  apenas como evidência literal de uma suíte conhecida, nunca como fonte de comandos.
- **Cold start com snapshot:** no Windows o `ff-indexer` passa a usar um snapshot por padrão, em
  `%LOCALAPPDATA%\FastFile\snapshots` (privado ao usuário), carregando o índice salvo e retomando pelo USN
  em vez de reindexar o volume. No volume real (4.070.762 entradas) o snapshot carrega em 1,29 s e a retomada
  pelo USN concluiu com sucesso (+273 entradas), contra ≈9,8 s para reindexar pela `$MFT`. O snapshot é regravado a cada 10 min se o índice mudou (`FASTFILE_SNAPSHOT_INTERVAL_SECS`)
  e logo após uma retomada. `--no-snapshot` desativa; `--snapshot <arquivo>` escolhe o caminho. Em modo
  serviço não há padrão (outra conta). O tempo total da retomada ainda não foi medido.
- Gravação de snapshot atômica (arquivo temporário + `rename`) e `0600` em Unix; um snapshot de outro alvo ou
  sem alvo registrado é ignorado.
- Attestation de proveniência (SLSA) em todos os artefatos da release e assinatura Authenticode opcional
  (ativada pelo segredo `WINDOWS_CERT_PFX_BASE64`); ver [`docs/assinatura.md`](docs/assinatura.md).
- `SECURITY.md`, `deny.toml` (cargo-deny no CI), Dependabot, `rust-toolchain.toml`.
- Lint `clippy::undocumented_unsafe_blocks` (negado no workspace): todo `unsafe` documenta seu `// SAFETY:`.
- Melhorias de UX na interface: atalhos, menu de contexto, duplo clique e notificações (toast).

- `ff_index::ntfs`: parsers puros e multiplataforma de buffers USN e registros MFT (antes embutidos nos
  laços de leitura do Windows), cobertos por testes, mutação e alvos de fuzz (`usn_buffer`, `mft_record`).
- Testes de robustez determinísticos (`*_robustness.rs`) para snapshot, DOCX/PDF, IPC e MCP, e alvos de
  `cargo-fuzz` em `fuzz/` com job semanal (`.github/workflows/fuzz.yml`).

### Alterado
- **Snapshot ~5,6× mais rápido de carregar** (formato `FFSNAP02`): no volume real (4.075.958 entradas,
  365 MB) a carga caiu de 1,29 s para 0,23 s. O checksum passou a ser FNV-1a por palavras de 8 bytes
  (0,39 s → 0,05 s), a leitura é feita em lotes e os vetores são alocados de uma vez depois de conferir os
  tamanhos do cabeçalho contra o tamanho do arquivo, e o `frn_map` usa um hasher rápido em vez do SipHash.
  Snapshots `FFSNAP01` continuam sendo lidos (0,76 s) e são regravados no formato novo na próxima gravação.
  De ponta a ponta (`ff-indexer C: --once`, snapshot + retomada pelo USN), o índice fica pronto em 0,31 s
  (antes 1,76 s). `ff bench C:` segue com os checkpoints A, B e E em PASS.
- Indexação da `$MFT` com leitores em paralelo (`read_mft_parallel`, até 8, cada um com seu handle do volume).
  Medições mostraram que parse + inserção custam só ~0,5 s por milhão de registros: o tempo (38 s em NTFS
  real, ~160 MB/s num NVMe) era I/O síncrono de fila 1. `FASTFILE_MFT_READERS=1` força leitura sequencial
  (recomendado em HDD). Medido em NVMe real com BitLocker (`$MFT` de 6 GB): 1 leitor ≈ 540 MiB/s (11,4 s),
  2 a 16 leitores ≈ 680 MiB/s (≈9 s), com ou sem cache e com blocos de 1 a 16 MiB. Em 9 s, os leitores passam
  93% do tempo em I/O e o consumidor 7 s ocioso (inserção no índice: 1,9 s): o teto é o caminho de leitura
  do volume cru, não o parse. O padrão é até 4 leitores, blocos de 4 MiB (`FASTFILE_MFT_CHUNK_MIB`).
- Leitura da `$MFT` sem o cache do sistema (`FILE_FLAG_NO_BUFFERING`, buffers alinhados), com retorno automático
  ao I/O com cache se o sistema recusar. `FASTFILE_MFT_BUFFERED=1` força o modo com cache e
  `FASTFILE_MFT_TRACE=1` imprime modo, leitores, vazão e tempo da leitura.
- Versão do workspace alinhada com a tag `v0.2.0` (`Cargo.toml`, instalador, AUR, snap, flatpak).
- `[profile.release]` com LTO, `codegen-units = 1` e `strip`.
- CI e release usam `--locked`.

### Corrigido
- **Caminhos no índice da `$MFT`:** a raiz do NTFS (registro 5) tem a si mesma como pai, o que criava um ciclo
  em toda cadeia de pais: `build_path` falhava em todo o índice (e custava ~11 ms por chamada). A raiz agora
  fica sem pai e se chama `C:`, como nas outras origens. Medido: 500 chamadas, de 8,18 s para 0,54 ms.
- **Retomada pelo USN:** lê o diário até o fim (antes parava na primeira leitura de 64 KiB) e o snapshot é
  regravado após retomar, para que o ponto do USN não envelheça até o diário dar a volta (erro 1181).
  Retomada de 1.741 mudanças em 100 ms; índice pronto em 1,2 s (reindexação completa: ~11 s).
- **Segurança:** o Named Pipe do indexador (Windows) usava a DACL padrão, legível por outros usuários
  locais; agora só o usuário atual e o SYSTEM têm acesso, e clientes remotos são rejeitados. Se a DACL não
  puder ser criada, a IPC não inicia.
- **Segurança:** o socket Unix era criado com as permissões do `umask` antes do `chmod 0600` e podia
  disputar o caminho em `/tmp`; agora nasce em diretório privado `0700` e é movido atomicamente.
- `load_snapshot` abortava (`capacity overflow`/OOM) com contagens corrompidas no cabeçalho, antes de
  validar o checksum; a alocação agora é limitada e proporcional aos dados reais.
- Registros MFT corrompidos (atributo no fim do registro, `attr_len` pequeno) indexavam fora dos limites
  e podiam derrubar a indexação; timestamps `FILETIME` extremos não estouram mais `i64`. Entradas criadas
  ou alteradas pelo watcher USN agora mantêm as flags hidden/system (antes só read-only).
- Extração de PDF não derruba mais a busca/prévia com PDFs malformados (pânico de `pdf-extract` capturado).
- Extração de DOCX limita o `word/document.xml` descompactado (32 MiB) contra *zip bombs*.
- **Índice incompleto no Windows:** a leitura da `$MFT` começava no primeiro cluster e parava em 500.000
  registros, sem seguir os extents da tabela; numa MFT de ~6 GB (≈6,3 milhões de registros) o `ff` indexava
  só ~175 mil arquivos. Agora o run list do `$DATA` do registro 0 é lido (`parse_mft_layout`) e todos os
  extents são percorridos (`plan_mft_reads`). Se o layout for ilegível/incompleto ou uma leitura falhar, usa
  o USN (índice completo, sem tamanhos) em vez de deixar um índice parcial.
  Em NTFS real o registro 0 comporta só 81 extents (4,0 de 6,5 GB) e o resto fica em registros de extensão
  via `$ATTRIBUTE_LIST` (não residente nesta máquina): agora seguido por `resolve_mft_layout`. Medido em NTFS
  real (NVMe, `$MFT` de 6 GB com 166 extents): 4,07 milhões de entradas em 9,8 s, 135 B/entrada (524 MB),
  busca P50 ≈ 12 ms / P95 ≈ 14 ms. As promessas de "2 ms" e "quase zero de memória" do README foram corrigidas.
- `ff bench` mostrava `rota: mft` mesmo quando a MFT era abandonada e o USN usado; agora exibe
  `usn (fallback da mft)` e o motivo (`MftSource::fallback_reason`).
- Build release no Windows falhava no link (`__mingw_fprintf`) quando a variável de ambiente `CC` apontava
  para o MinGW; `.cargo/config.toml` anula `CC` para o alvo MSVC e o `cc-rs` usa o `cl.exe`.
- Leitura de registros USN/MFT (Windows) e eventos inotify (Linux) não cria mais referências
  desalinhadas sobre buffers de bytes (comportamento indefinido); nomes UTF-16 são decodificados sem
  ponteiros brutos.
- Fluxos centrais: resolução de caminhos, paginação do `find` por IPC, watcher contínuo, UI que não
  bloqueia com o daemon parado, distribuição/CI (CORE-01 a CORE-05).

## [0.2.0] - 2026-11-17

Release publicada a partir de `fix(core,ipc,ui)`: correção P0 dos fluxos centrais.

## [0.1.0] - 2026-11-16

Primeira versão: índice NTFS (MFT + USN) com fallback, busca por nome e conteúdo, treemap, diff,
prévia, CLI `ff`/`fastfile` com JSON `ff/1` e servidor MCP.

[Não lançado]: https://github.com/fabiomotta0311/fastfile/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/fabiomotta0311/fastfile/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/fabiomotta0311/fastfile/releases/tag/v0.1.0
