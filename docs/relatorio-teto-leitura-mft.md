# Relatório técnico: teto de leitura de ~680 MiB/s ao ler a `$MFT` no FastFile

**Objetivo deste documento:** dar a outra IA (ou pessoa) o contexto completo para descobrir por que a leitura
bruta da `$MFT` NTFS satura em ~680 MiB/s numa máquina onde o disco deveria entregar bem mais.
**Atualização:** a seção 11 traz a causa **confirmada por experimento**: os dados da `$MFT` estão fisicamente
espalhados dentro do SSD (FTL); o FastFile, o BitLocker e o acesso ao volume bruto foram descartados. A seção 12
mostra onde estava o ganho de verdade: no início com snapshot, o índice passou a ficar pronto em 0,31 s (antes
1,76 s). Das seções 1 a 10, tudo é medição, leitura de código ou hipótese.

Data das medições: 2026-11-18. Repositório: `C:\Users\fabio\Projects\fastfile`, branch `chore/profissionalizacao`
(PR #71 em `fabiomotta0311/fastfile`).

---

## 1. O problema em uma frase

Ler os **6170 MiB da `$MFT`** do volume `C:` leva **~9,1 s (≈678 MiB/s)**, enquanto o teste de disco do próprio
Windows (`winsat`) reportou **~1660 MB/s** na mesma máquina. Ainda não sabemos o que impõe o teto de ~680 MiB/s.

## 2. A máquina (o que sabemos)

| Item | Valor | Fonte |
| --- | --- | --- |
| SO | Windows 11 Pro (build 10.0.26300) | ambiente da sessão |
| Disco | NVMe (modelo, geração PCIe e largura do link **desconhecidos**) | usuário |
| Criptografia | **BitLocker, XTS-AES 128** no `C:` | `manage-bde -status` |
| `winsat` | ~1660 MB/s (qual teste/parâmetros exatamente: **não registrados**) | saída colada pelo usuário |
| NTFS | registros de 1024 B; cluster de 4096 B | `fsutil fsinfo ntfsinfo C:` |
| `$MFT` | Valid Data Length ≈ 6,03 GB (6170 MiB); **6.318.592 registros**; **166 extents** (fragmentada) | `fsutil` + trace |
| Entradas indexadas | ~4,07 milhões | daemon |
| CPU | modelo e número de núcleos **desconhecidos** (o programa usa `clamp(2,4)` leitores por padrão) | — |

**Dados que faltam e devem ser coletados primeiro** (ver seção 8): modelo do SSD, largura/velocidade do link PCIe,
CPU, plano de energia, antivírus, se a CPU tem AES-NI, e se há outro volume sem BitLocker para comparar.

## 3. Como o programa lê a `$MFT` (leitura de código)

Arquivos: `crates/ff-index/src/windows/mft.rs` e `crates/ff-index/src/ntfs.rs`.

1. Abre o volume bruto `\\.\C:` com `CreateFileW`, **somente leitura**, `FILE_SHARE_READ|WRITE` e
   **`FILE_FLAG_NO_BUFFERING`** (padrão; `FASTFILE_MFT_BUFFERED=1` volta ao modo com cache).
2. Lê o registro 0, extrai o run list do `$DATA` (e o `$ATTRIBUTE_LIST`, que é **não residente** nesta máquina) e
   monta o mapa dos **166 extents**.
3. Planeja a leitura em blocos de **4 MiB** (`FASTFILE_MFT_CHUNK_MIB`, 1 a 64), alinhados ao cluster:
   6170 MiB → **1613 blocos**.
4. Dispara **N leitores** (`FASTFILE_MFT_READERS`; padrão `clamp(2,4)` do paralelismo disponível; nos testes: **4**).
   Cada leitor tem **seu próprio handle** do volume e pega o próximo bloco de um contador atômico compartilhado.
5. Cada leitura é **síncrona**: `SetFilePointerEx` + `ReadFile` (sem `OVERLAPPED`, sem I/O completion port).
   Logo, a profundidade de fila total é **no máximo igual ao número de leitores (4)**, com blocos de 4 MiB.
6. O leitor interpreta os registros (fixups, atributos) e envia o lote por um canal limitado a um único consumidor,
   que insere no índice.
7. Buffers alinhados (4096 B) para o modo sem cache; o consumidor roda numa única thread.

## 4. Medições (o que foi observado)

### 4.1 Última medição, com o código atual (`FASTFILE_MFT_TRACE=1`)

```
[mft] layout ok: 166 extents, 6318592 registros (6170 MiB)
[mft] modo=sem-cache leitores=4 blocos=1613 dados=6170 MiB leitura+parse=9.10s (678 MiB/s) ok=true
[mft] tempos (somas entre threads): leitores: io=34.54s parse=1.43s bloqueados=0.42s | consumidor: indice=1.84s espera=7.27s
Indexação completa concluída em 10.16s: 4074599 entradas.
```

Leitura dos números (aritmética, não hipótese):

- `io=34,54 s` somado entre **4 leitores** → ~8,6 s por leitor, para ~9,1 s de relógio. **Os leitores passam ~95% do
  tempo dentro de `ReadFile`.** O programa está esperando o disco/pilha de armazenamento.
- `parse=1,43 s` somado (≈0,36 s por leitor) e `indice=1,84 s` (consumidor): **CPU do programa não é o gargalo.**
- `espera=7,27 s` do consumidor esperando dados: ele fica ocioso a maior parte do tempo, esperando os leitores.
- `bloqueados=0,42 s`: o canal quase nunca enche; o consumidor acompanha.

Uma medição anterior semelhante deu `io=33,58 s parse=1,51 s ... espera=7,14 s` (consistente).

### 4.2 Histórico relevante

- O modo **com cache** do sistema limitava a ~240 MB/s (comentário no código); sem cache melhorou para o patamar atual.
- Houve uma matriz de testes **leitores × tamanho de bloco** que **não encontrou configuração claramente melhor**.
  **Os números exatos dessa matriz não estão neste relatório**; estão na transcrição da sessão (ver seção 9).
- Medições antigas de 25–32 s **estão contaminadas**: na época o programa abandonava a `$MFT` e caía no fallback
  para o USN sem avisar. **Não use esses números.** Só vale o que tem `ok=true` e o `layout ok` no trace.

## 5. O que já foi descartado (por medição)

| Hipótese | Resultado |
| --- | --- |
| CPU do programa limita | **Descartada:** parse 1,43 s e índice 1,84 s somados; ficam ociosos esperando I/O |
| Consumidor único limita | **Descartada:** `bloqueados` 0,42 s, consumidor espera 7,27 s |
| Cache do sistema (modo com cache) | Piorava (~240 MB/s); o modo sem cache já é o padrão |
| A leitura estava incompleta (só o 1º extent) | Corrigido: lê os 166 extents; mesmo assim o teto persiste |

## 6. O que NÃO foi descartado (hipóteses, **nenhuma confirmada**)

1. **BitLocker (XTS-AES 128).** Toda leitura do volume passa pelo `fvevol`, que decifra. O custo pode ser CPU do
   kernel (threads do sistema) e não aparecer no processo do FastFile. É a hipótese que eu (Claude) considero mais
   plausível, mas **não foi testada**: nunca comparamos com um volume sem BitLocker.
2. **Profundidade de fila baixa (QD ≤ 4, síncrona).** NVMes costumam precisar de QD maior ou de leituras maiores
   (ou I/O assíncrono) para chegar ao pico. Os leitores bloqueiam em `ReadFile`, então QD efetivo = nº de leitores.
   A matriz de leitores mostrou pouca melhora, o que enfraquece, mas não elimina, essa hipótese (o teto pode estar
   em outro ponto da pilha).
3. **Fragmentação da `$MFT` (166 extents).** Cada extent é contíguo, mas os 4 leitores pulam entre regiões. Em NVMe o
   custo costuma ser baixo, mas não foi medido isolando extents.
4. **Acesso ao volume bruto (`\\.\C:`) vs. a um arquivo.** Leituras no dispositivo do volume podem ter caminho
   diferente das de um arquivo grande (limites de tamanho de I/O do stack, divisão de requisições). Não testado.
5. **Limite do próprio dispositivo ou do link.** SSD com cache SLC/DRAM-less esgotado, link PCIe x2 ou Gen3,
   throttling térmico, gerenciamento de energia (ASPM/APST). **Desconhecido** porque faltam modelo e link.
6. **`winsat` pode não ser comparável.** Se o `winsat` mede com outros parâmetros (tamanho de bloco, QD, arquivo vs.
   volume), os 1660 MB/s não são uma referência direta para este padrão de leitura.
7. **Antivírus/filtros de sistema de arquivos** no caminho de I/O do volume (improvável em leitura bruta, mas não
   verificado).

## 7. Experimentos sugeridos (do mais informativo ao menos)

Objetivo: separar **(a)** teto do dispositivo, **(b)** custo do BitLocker e **(c)** limitação do padrão de leitura
do programa. Sempre usar o mesmo tamanho de bloco (4 MiB) e leitura **sem cache**.

1. **Referência independente com `diskspd`** (ferramenta da Microsoft) lendo o próprio volume ou um arquivo grande,
   variando QD: bloco 4 MiB, `-Sh` (sem cache e sem write-through), `-o 1/2/4/8/16/32`, sequencial. Se o `diskspd`
   passar bem de 680 MiB/s com QD alto → o limite é o padrão de leitura do FastFile (QD/síncrono). Se também
   travar em ~680 → é o dispositivo ou o BitLocker.
2. **Comparar com um volume sem BitLocker** no mesmo SSD (ou outro disco), com o mesmo teste. Isolará o custo da
   criptografia. Alternativa não destrutiva: observar durante a leitura o uso de CPU por núcleo e as threads do
   processo `System` (se um núcleo do kernel satura, é decifragem).
3. **Variar os knobs do programa**: `FASTFILE_MFT_READERS` = 1, 2, 4, 8, 16, 32 e `FASTFILE_MFT_CHUNK_MIB` = 1, 4,
   16, 64, registrando `leitura+parse` e `io=` somado de cada combinação (usar `FASTFILE_MFT_TRACE=1`).
4. **Trocar a leitura síncrona por I/O assíncrono** (`OVERLAPPED` + IOCP, ou várias requisições pendentes por
   handle) e medir. É a mudança de código mais provável para testar a hipótese 2.
5. **Ler só um extent grande** (um dos 166) em vez da `$MFT` toda, para isolar fragmentação.
6. **Monitorar** com Performance Monitor: `PhysicalDisk\Disk Read Bytes/sec`, `Avg. Disk Queue Length`,
   `Avg. Disk sec/Read` e CPU por núcleo, durante a indexação.

## 8. Comandos para coletar os dados que faltam (PowerShell, administrador)

```powershell
Get-PhysicalDisk | Format-List FriendlyName,Model,MediaType,BusType,Size,FirmwareVersion
Get-Disk | Format-List Number,FriendlyName,PartitionStyle,BusType,LogicalSectorSize,PhysicalSectorSize
Get-CimInstance Win32_Processor | Format-List Name,NumberOfCores,NumberOfLogicalProcessors,MaxClockSpeed
manage-bde -status C:
fsutil fsinfo ntfsinfo C:
winsat disk -drive c -ran -read
winsat disk -drive c -seq -read
powercfg /getactivescheme
```

Link PCIe do NVMe: Gerenciador de Dispositivos > controlador NVMe > Detalhes, ou ferramenta do fabricante.
Para o BitLocker, também vale `Get-BitLockerVolume`.

## 9. Como reproduzir a medição do FastFile

No `C:\Users\fabio\Projects\fastfile`, em **PowerShell de administrador** (o volume bruto exige elevação), usando
**sempre** o binário compilado (o `ff-indexer` do PATH é uma versão antiga e não serve):

```powershell
cd C:\Users\fabio\Projects\fastfile
cargo build --release --locked -p ff-indexer
$env:FASTFILE_MFT_TRACE = "1"
Remove-Item "$env:LOCALAPPDATA\FastFile\snapshots\c-8d29e7b8.ffidx" -ErrorAction SilentlyContinue
.\target\release\ff-indexer.exe C: --once
```

Remover o snapshot força a leitura completa da `$MFT` (sem isso o programa carrega o snapshot e retoma pelo USN).
Variáveis de ambiente para experimentar: `FASTFILE_MFT_READERS`, `FASTFILE_MFT_CHUNK_MIB`, `FASTFILE_MFT_BUFFERED`.

Transcrição completa da sessão, com a matriz de leitores × blocos e as saídas de `winsat`/`manage-bde`:
`C:\Users\fabio\.claude\projects\C--Users-fabio-Projects-fastfile\01e9ad51-7bd7-4433-a602-ec6bb052fbc1.jsonl`
(contém dados do usuário; revisar antes de compartilhar).

## 10. O que a outra IA deve entregar

- Uma **causa confirmada por experimento** (não por argumento), com os números do experimento que a separa das
  demais hipóteses da seção 6.
- Se a causa for o padrão de leitura do FastFile: a mudança de código e o `leitura+parse` antes/depois no mesmo
  volume.
- Se a causa for dispositivo ou BitLocker: o valor do teto e a conclusão de que não há ganho de software possível.

## 11. Resultado da investigação (2026-11-18)

**Causa provável, reproduzida por experimento: fragmentação interna do SSD (FTL), não o FastFile, nem o
BitLocker, nem a fila de I/O.** A `$MFT` é escrita em registros de 1 KiB atualizados um a um ao longo de anos; o
SSD grava cada reescrita numa página física nova, então endereços lógicos contíguos ficam espalhados pela NAND e a
leitura "sequencial" vira, dentro do disco, leitura aleatória. Esse custo não muda com QD nem com o tamanho do bloco.

Dados que faltavam: `C:` = ADATA IM2P33F3A 512 GB (NVMe, PCIe Gen3 x4); `D:` = Crucial P5 500 GB (Gen3 x4, sem
BitLocker); CPU Ryzen 5 5600H (6C/12T, AES-NI); plano de energia Equilibrado.

Ferramenta: leitor mínimo em Rust (`std` apenas, `FILE_FLAG_NO_BUFFERING`, `seek_read` síncrono, N threads com
contador atômico: o mesmo padrão do FastFile), lendo **arquivos** como usuário comum.

| Experimento (4 threads × 4 MiB, sem cache, salvo indicação) | C: (BitLocker) | D: (sem BitLocker) |
| --- | --- | --- |
| Arquivo de 6 GiB recém-gravado (~307 extents NTFS) | **2061 MiB/s** | 3061 MiB/s |
| Mesmo arquivo, 1/8/32 threads; blocos 128 KiB–4 MiB | 1838–2061 | 2453–3119 |
| `system.img` gravado de uma vez em 2023 (4 GiB) | 2238 | — |
| `.ost` do Outlook (reescrito aos pedaços, 4,5 GiB) | 1135 | — |
| Arquivo de 1 GiB **fresco** | 2109 | 3102 |
| **O mesmo arquivo** após reescrever cada página de 4 KiB uma vez, em ordem aleatória | **435** | **509** |
| Idem, com 32 threads | 424 | 502 |
| Idem, 1×4 MiB / 8×1 MiB / 32×128 KiB / 64×64 KiB | 393 / 399 / 362 / 364 | — |

O que isso descarta e o que mostra:

- **BitLocker:** o mesmo padrão de leitura no `C:` cifrado chega a ~2 GB/s. Não é o teto.
- **QD ≤ 4 síncrono / leitura do FastFile:** com dados "frescos", 4 threads × 4 MiB já saturam o disco; com dados
  espalhados, nem 32–64 requisições pendentes ajudam (igual ao que a matriz de leitores do FastFile mostrou).
- **Fragmentação NTFS (extents):** 307 extents leram a 2 GB/s.
- **Idade dos dados (retenção):** arquivo de 2023 gravado sequencialmente leu a 2,2 GB/s.
- **Mecanismo:** mesmos endereços lógicos, mesmo padrão de leitura; só a ordem em que as páginas foram
  **escritas** mudou, e a vazão caiu ~5× nos dois SSDs. Os 678 MiB/s da `$MFT` ficam entre o caso fresco e o
  totalmente embaralhado, coerente com uma `$MFT` parcialmente espalhada.

**Ainda não verificado (exige administrador):** ler o volume bruto `\\.\C:` na região da `$MFT` e na de um arquivo
contíguo, com o mesmo leitor, para descartar diferença entre acesso ao volume bruto e a arquivo. Esperado: ~400–700
MiB/s na `$MFT` e ~2000 MiB/s no controle.

**Checagem como administrador, 1ª rodada (volume bruto `\\.\C:`, 4 × 4 MiB, 512 MiB):** início da `$MFT`
(LCN 786432) = **362 MiB/s**; região do `system.img` (LCN 38029742) = **732 MiB/s**. A `$MFT` é 2× mais lenta que o
controle, coerente com a hipótese da FTL, mas o controle pelo volume bruto (732) ficou muito abaixo do mesmo arquivo
lido como arquivo (2238). **Inconclusivo:** o script lia 512 MiB a partir do 1º extent sem checar o tamanho dele, então
pode ter lido dados de outros arquivos. Se a diferença se confirmar nos mesmos bytes, o acesso ao volume bruto também
custa caro (hipótese 4 da seção 6) e a conclusão abaixo deixa de valer. Próximo passo: script v2, que compara arquivo e
volume bruto exatamente nos mesmos bytes.

**2ª rodada (v2):** o 1º extent do `system.img` tem só 21 MiB (o arquivo tem 24 extents), então a v1 de fato leu
dados de outros arquivos. Nos mesmos 20 MiB: arquivo 1495 MiB/s; volume bruto 22–26 MiB/s. Início da `$MFT` (256
MiB) pelo volume bruto: 204 MiB/s com 4 × 4 MiB e **46 MiB/s com 32 × 128 KiB** (fila maior piorou). A amostra de 20
MiB é curta demais (o tempo de abrir `\\.\C:` e a 1ª leitura de cada handle entram no cronômetro), mas o padrão
(volume bruto lento e piorando com mais requisições pequenas) indica **custo próprio do volume bruto**, hipótese 4
da seção 6. Próximo passo (v3): ler o arquivo inteiro pelo arquivo e pelo volume bruto seguindo os extents, com
handles abertos e aquecidos fora do cronômetro, e a própria `$MFT` do mesmo jeito.

**3ª rodada (v3), arquivo inteiro, mesmos bytes, handles abertos e aquecidos fora do cronômetro:**

| Arquivo (extents) | Config | Pelo arquivo | Pelo volume bruto | 1ª leitura dos handles do volume |
| --- | --- | --- | --- | --- |
| `system.img` 4107 MiB (24) | 4 × 4 MiB | 2260 MiB/s | **2216 MiB/s** | 0,72 s |
| | 16 × 1 MiB | 1754 | 1720 | 3,07 s |
| | 32 × 128 KiB | 1524 | 1583 | 5,72 s |
| `ggml-large-v3.bin` 2952 MiB (79) | 4 × 4 MiB | 2051 | **2019** | 0,83 s |
| | 16 × 1 MiB | 1748 | 1775 | 3,03 s |

- **Volume bruto ≈ arquivo em regime.** A hipótese 4 está descartada para a vazão. A v2 media o custo de partida.
- **Novo achado:** a **1ª leitura de cada handle novo de `\\.\C:` custa ~0,18–0,2 s** (0,72 s / 4, 3,07 s / 16,
  5,72 s / 32; no teste os handles foram aquecidos um após o outro). Em arquivo, a 1ª leitura custa ~1 ms. No
  FastFile, os 4 leitores abrem 4 handles novos, além do handle principal: até ~0,8 s dos 9,1 s. Isso também ajuda a
  explicar por que mais leitores não melhoravam o tempo total.
**A própria `$MFT`** (166 extents, 6170 MiB, lida pelo volume bruto seguindo os extents, mesmo leitor mínimo, sem
parse, handles aquecidos fora do cronômetro):

| Config | Vazão | 1ª leitura dos handles |
| --- | --- | --- |
| 4 × 4 MiB | **699 MiB/s** (8,83 s) | 0,76 s |
| 16 × 1 MiB | 686 MiB/s | 3,02 s |
| 32 × 128 KiB | 672 MiB/s | 6,09 s |

**Conclusão (confirmada por experimento):** no mesmo disco, com o mesmo leitor, o mesmo caminho (volume bruto) e o
mesmo padrão, arquivos gravados de uma vez rendem ~2000–2260 MiB/s e a `$MFT` rende ~680–700 MiB/s, sem reagir a
fila nem a tamanho de bloco. O leitor mínimo, que não faz parse, chega ao mesmo número do FastFile (678 MiB/s), então
o código do FastFile não acrescenta custo. A única variável restante são os dados: a `$MFT` está espalhada
fisicamente dentro do SSD (FTL), e o embaralhamento proposital de um arquivo reproduz o efeito (2109 → 435 MiB/s).
**O teto é do dispositivo para esses dados.**

Consequência para o código: **não há ganho de I/O a buscar** (assíncrono/IOCP, mais leitores ou blocos maiores) neste
disco. Ganhos possíveis vêm de **ler menos** (pular faixas livres via `$MFT:$BITMAP`, se forem contíguas) ou de **não
reler** a `$MFT` (snapshot + retomada pelo USN, já implementado). Desfragmentar no NTFS não mira a FTL, mas mover dados os regrava em sequência (efeito colateral). Na `$MFT` o ganho
seria temporário: ela volta a se espalhar com as atualizações de 1 KiB. Não testado.

**Handle único compartilhado: testado e descartado.** Hipótese: abrir um handle por leitor pagaria várias vezes a
1ª leitura (~0,2 s por handle). Implementação testada: um handle `FILE_FLAG_OVERLAPPED` sem cache compartilhado pelos
4 leitores, cada um com o seu `OVERLAPPED` e evento. A/B como administrador, 3 rodadas alternadas, snapshot removido
a cada rodada:

| Rodada | Um handle por leitor (`leitura+parse`) | Handle compartilhado |
| --- | --- | --- |
| 1 | 9,24 s | 9,02 s |
| 2 | 8,99 s | 9,13 s |
| 3 | 9,00 s | 9,05 s |

Diferença dentro do ruído (médias 9,08 s contra 9,07 s). Com os handles abertos em paralelo, o custo da 1ª leitura
se sobrepõe e não aparece no total. A mudança foi revertida (só acrescentava código `unsafe` sem ganho).

## 12. Onde estava o ganho: o início com snapshot (commit `68c2528`)

A leitura completa da `$MFT` só acontece na primeira indexação ou quando o snapshot se perde. No uso normal, o
`ff-indexer` carrega o snapshot e retoma pelo USN. Medido em `ff-indexer C: --once` (administrador, sem apagar o
snapshot), antes da mudança:

| Etapa | Tempo |
| --- | --- |
| Carregar o snapshot (365 MB, 4,07 milhões de entradas) | 1,36 s |
| Retomar pelo USN (6252 mudanças) | 0,40 s |
| **Índice pronto** | **1,76 s** |
| Regravar o snapshot (depois do índice pronto) | 0,93 s |

Carga do snapshot dividida por fase (leitor isolado, snapshot real, 1,29 s no total): ler o arquivo 0,10 s;
checksum FNV-1a byte a byte **0,39 s**; montar o `frn_map` (SipHash, sem pré-alocação) **0,44 s**; ~4 milhões de
leituras pequenas para entradas e nomes ~0,36 s. O disco não era o gargalo.

Mudança (formato `FFSNAP02`, `FFSNAP01` ainda aceito na carga): checksum FNV-1a por palavras de 8 bytes; leitura e
escrita em lotes; tamanhos do cabeçalho conferidos contra o tamanho do arquivo antes de alocar, com cada vetor alocado
de uma vez; `frn_map` com hasher rápido (multiplicação dobrada) no lugar do SipHash. Resultado:

| Medição | Antes | Depois |
| --- | --- | --- |
| Carregar o snapshot (leitor isolado) | 1,29 s | **0,23 s** |
| Carregar o snapshot (`ff-indexer`) | 1,36 s | 0,25 s |
| **Índice pronto** (`ff-indexer C: --once`) | **1,76 s** | **0,31 s** |
| Montar o snapshot na memória para salvar (CPU) | ≥ 0,39 s (só o checksum) | ~0,20 s |

Gravar o snapshot variou de 0,5 a 4,8 s entre rodadas iguais. Montar o snapshot na memória custa ~0,2 s; o resto é a
gravação de 365 MB no SSD do `C:`, que cai de vazão em gravações seguidas. Isso acontece depois do índice pronto (no
modo daemon, em segundo plano). `ff bench C: main .rs config.toml readme --iterations 50`: checkpoints A, B e E em
PASS (indexação pela `$MFT` em 9,76 s; P95 de 22–40 ms; 134 B/entrada).

Próximo ganho possível, não implementado: trocar o `frn_map` por uma tabela indexada pelo número do registro na
`$MFT` (~25 MB, montada em milissegundos), com plano B para fontes que não são NTFS. Estimativa: ~0,2 s a menos na
carga.

## 13. Aviso de calibração

Neste projeto, minhas hipóteses sobre esse gargalo (CPU, "teto de disco", cache) foram refutadas pelas medições do
usuário mais de uma vez. Trate qualquer explicação sem experimento, incluindo as da seção 6, como palpite.
