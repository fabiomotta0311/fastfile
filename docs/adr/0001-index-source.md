# ADR 0001: Fonte do Índice NTFS — MFT com fallback para USN e Walk (INDEX-SOURCE-001)

Adotamos a leitura bruta da `$MFT` (Rota 2, `MftSource`) como método padrão de indexação no Windows, com fallback automático em cascata para `UsnSource` (Rota 1, `FSCTL_ENUM_USN_DATA`) e `WalkSource`.
A `$MFT` provê nomes, hierarquia e tamanhos reais diretamente no primeiro escaneamento sem sobrecarga adicional de E/S, enquanto a enumeração pelo USN Journal e a varredura paralela garantem resiliência caso a leitura física bruta do dispositivo seja restrita.
A decisão cumpre os Checkpoints A, D e E da Fase 0 sem comprometer portabilidade ou integridade.
