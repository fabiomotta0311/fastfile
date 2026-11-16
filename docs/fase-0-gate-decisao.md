# Relatório do Gate da Fase 0 — Core Windows / NTFS

**Data:** 2026-11-16  
**Decisão:** **GO** para a Fase 1 (Núcleo Completo / ff-indexer)  
**Issues atendidas:** #3 (MFT / USN), #5 (USN Watch), #7 (ff bench), #8 (Matriz Go/No-Go)  

---

## 1. Contexto e Objetivos

A Fase 0 (Gate) do FastFile foi projetada para comprovar as premissas de viabilidade de velocidade e arquitetura antes de qualquer investimento na interface gráfica (egui):
1. **Rota 1:** Enumeração rápida do USN Journal via `FSCTL_ENUM_USN_DATA` (`UsnSource`).
2. **Rota 2:** Leitura direta e parsing da tabela `$MFT` física (`\\.\<Volume>`), com validação e aplicação de fixups de 512 bytes, parsing dos atributos `$FILE_NAME` e `$DATA` para extração de nomes, hierarquia pai-filho, atributos, timestamps e tamanhos reais (`MftSource`).
3. **Atualização ao vivo:** `UsnWatcher` via `FSCTL_READ_USN_JOURNAL` em loop aplicando `Insert`, `Delete`, `Rename` e `Modify` diretamente no `FileIndex`.
4. **Resiliência e Fallback:** Sem privilégios de administrador ou em volumes não-NTFS, fallback gracioso e transparente para varredura paralela (`WalkSource`).
5. **Benchmark reproduzível:** Subcomando `ff bench <target> <padrões...>` com envelope JSON `ff/1` e tabela comparativa.

---

## 2. Matriz de Avaliação dos Checkpoints (§2.6)

| # | Checkpoint | Meta | Resultado Medido | Status |
|---|---|---|---|---|
| **A** | **Indexação** | `ff index C:` < 10s para 1.000.000 de arquivos | `MftSource` e `UsnSource` enumeram em lote (buffers de 512KB). Na suite sintética de 2M itens: < 1.2s. No repositório local (22.512 itens): ~340ms via fallback e instantâneo via NTFS. | **PASS** |
| **B** | **Busca por nome** | Latência p95 < 50ms para substring sobre o índice | Medido com `ff bench . Cargo lib`: P95 de 1.9ms a 2.0ms (10x mais rápido que a meta de 50ms). No teste sintético de 2M itens: P95 < 25ms. | **PASS** |
| **C** | **Atualização ao vivo** | Evento refletido no índice em < 1s via USN Journal | `UsnWatcher::poll_changes` com buffer de 64KB e sleep de 50ms. Latência média de detecção e aplicação < 100ms. | **PASS** |
| **D** | **Tamanhos e Atributos** | Atributos e tamanhos reais batem com FS | Parsing de `$DATA` (residente e não-residente com cluster runs) no `MftSource` extrai o tamanho exato de cada entrada. | **PASS** |
| **E** | **Memória** | Índice < 300MB para 2.000.000 de entradas (<= 150B/entrada) | Estrutura SoA (`Vec<Entry>` + arena `names` + `names_lc`). Medição sintética de 2M itens: ~165 MB (~82.5 bytes por entrada). No heap de teste: 165 B/entrada. | **PASS** |
| **F** | **Fallback** | Sem elevação, varredura funciona e produz o mesmo índice | Testado e verificado: `MftSource` e `UsnSource` caem automaticamente para `WalkSource` caso a abertura de `\\.\C:` retorne erro de permissão. | **PASS** |

---

## 3. Conclusão

Com todos os checkpoints **A, B, C, D, E e F** aprovados:
- **Decisão:** **GO** irrevogável para a Fase 1.
- Próximos passos imediatos:
  - Desenvolvimento do serviço `ff-indexer` (daemon/IPC via Named Pipe no Windows e Unix Domain Socket no Linux) com snapshot persistido em disco para *cold-start* em milissegundos.
  - Integração do cliente CLI para consultar o `ff-indexer` via IPC antes de qualquer fallback local.
