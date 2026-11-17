# ADR 0003: Distinção entre Tamanho Lógico, Espaço Alocado e Recuperação Estimada (SIZE-METRICS-001)

## Status
Aceito e Implementado (Fase 2, TM-03)

## Contexto
O objetivo principal do Treemap e das análises de uso de disco no FastFile é ajudar o usuário e agentes a identificar oportunidades reais de liberar espaço, entender as consequências e decidir o que remover.

Muitas ferramentas tradicionais confundem tamanho lógico com espaço recuperável. Essa confusão gera sérias distorções:
1. **Hard Links:** Múltiplas entradas apontando para o mesmo i-node/FRN. Deletar apenas um dos links não libera espaço no disco, pois os clusters permanecem alocados aos demais links.
2. **Arquivos Esparsos (Sparse Files):** Um arquivo de banco de dados ou imagem de máquina virtual (.vhd) pode ter 100 GB lógicos, mas ocupar apenas 2 GB alocados em disco. Prometer 100 GB de liberação é falso.
3. **Compressão (NTFS / ZFS / Btrfs):** Arquivos comprimidos ocupam menos clusters físicos que seu tamanho nominal descomprimido.
4. **Placeholders de Nuvem (OneDrive Files On-Demand, iCloud, etc.):** Arquivos desidratados marcados como `FILE_ATTRIBUTE_OFFLINE` ou `FILE_ATTRIBUTE_REPARSE_POINT` possuem tamanho lógico nominal, mas ocupação física local zero. Excluí-los não libera espaço em disco local.
5. **Valores Desconhecidos:** Quando o tamanho alocado não estiver disponível (ex.: sistemas ou volumes sem suporte), apresentar `0 B` induz ao erro de achar que o arquivo não ocupa espaço.

## Decisão
1. **Três Métricas Distintas (`SizeMetrics`):**
   - **Tamanho Lógico (`logical_size`):** Quantidade nominal de bytes do fluxo de dados (`meta.len()`).
   - **Espaço Alocado (`allocated_size: Option<u64>`):** Quantidade real de bytes alocados em clusters/blocos no sistema de arquivos. Quando desconhecido, é representado explicitamente como `None` e **nunca** exibido como `0 B`.
   - **Recuperação Estimada (`reclaimable_size: Option<u64>`):** Projeção conservadora do espaço físico em disco que será efetivamente devolvido ao sistema operacional se a exclusão ocorrer.

2. **Regras Conservadoras de Recuperação:**
   - **Arquivos Offline / Nuvem:** `reclaimable_size = Some(0)` com indicação de que o arquivo está desidratado na nuvem.
   - **Hard Links Individuais:** `reclaimable_size = Some(0)` com aviso de que o espaço permanece retido pelos outros links ativos no disco.
   - **Hard Links em Diretórios:** Os arquivos compartilhando o mesmo FRN/i-node são deduplicados. Se todos os links de um FRN estiverem contidos na mesma pasta excluída, seus clusters físicos são somados **uma única vez**. Se algum link residir fora da pasta, sua contribuição para a recuperação estimada da pasta é zero.
   - **Arquivos Esparsos e Comprimidos:** A recuperação estimada é estritamente o tamanho alocado real (`allocated_size`), e nunca o tamanho lógico nominal.

3. **Interface e Modos de Exibição:**
   - O Treemap disponibiliza botões para alternar o modo de dimensionamento visual: `[ 📊 Lógico | 💾 Em disco | ♻️ Recuperável ]`.
   - O cabeçalho exibe as 3 métricas agregadas da pasta.
   - O tooltip ao passar o mouse sobre cada item detalha as 3 medidas lado a lado com badges explicativos (`[Hard link]`, `[Nuvem]`, `[Esparso]`, `[Comprimido]`).
   - Formatação segura: valores desconhecidos (`None`) são exibidos como `"— (Não disponível)"`, preservando a clareza e prevenindo falsas promessas.
