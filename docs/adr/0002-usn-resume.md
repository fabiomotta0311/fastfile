# ADR 0002: Retomada Incremental do USN Journal a partir de Snapshot (USN-RESUME-001)

## Status
Aceito e Verificado (Fase 1)

## Contexto
O FastFile visa um tempo de subida (*cold start*) em milissegundos (< 50ms). Reindexar todo o volume NTFS a cada inicialização é inviável para atingir essa meta em discos com milhões de arquivos.

## Decisão
1. O estado residente do `FileIndex` (arenas de nomes, entradas e `frn_map`) é serializado em disco em formato binário compacto versionado (`FFSNAP01`; desde 2026-11 `FFSNAP02`, mesmo corpo com checksum por palavras de 8 bytes, e o `FFSNAP01` continua aceito na carga), acompanhado do identificador do journal (`journal_id`) e do último USN processado (`last_usn`).
2. Na inicialização:
   - Se existir arquivo de snapshot válido e o checksum bater, o `FileIndex` é carregado imediatamente.
   - O indexador consulta o volume NTFS via `FSCTL_QUERY_USN_JOURNAL`.
   - Se `journal_id` coincidir e `lowest_valid_usn <= snapshot.last_usn`, o journal **não foi truncado**. O indexador lê apenas os eventos entre `snapshot.last_usn` e `next_usn`, aplicando as alterações incrementais em milissegundos.
   - Se o journal foi truncado ou o `journal_id` divergir, o snapshot é descartado e é disparada uma reindexação completa via MFT (Rota 2).
3. O arquivo de snapshot é gravado com permissões restritas ao usuário local (privacidade e segurança de dados pessoais).
