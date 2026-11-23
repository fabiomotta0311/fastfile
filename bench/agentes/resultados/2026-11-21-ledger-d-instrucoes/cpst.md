# CPST — claude-code 2.1.328 (Claude Code) · claude-sonnet-5-5

Runs: 9886cac12416. Premissas: `claude-code-relatado/1`.

**Lote incompleto:** orcamento_esgotado (3 pendentes)

| Braço | Válidas / total | Sucesso | Custo | CPST | p50 / p95 (s) | Adoção FF | Ferramentas/tent. |
|---|---|---|---|---|---|---|---|
| D | 9 / 9 | 9 (100.0%) | US$ 0.5070 | US$ 0.0563 | 38.0 / 47.3 | 0.0% | 4.1 |

| Braço | Bytes/tent. | Ponderado/tent. (p50 / p95) | % FastFile | Respostas > 8 KiB | Releituras | Repetidas |
|---|---|---|---|---|---|---|
| D | 4280 | 11365 (12280 / 18126) | 0.0% | 0.0 | 0.0 | 0.0 |

| Tarefa | Braço | Sucesso / válidas | CPST | Adoção FF | Bytes | Ponderado | Releituras | Retentativas |
|---|---|---|---|---|---|---|---|---|
| L05 | D | 2 / 2 | US$ 0.0610 | 0.0% | 6030 | 16712 | 0.0 | 0 |
| L06 | D | 2 / 2 | US$ 0.0462 | 0.0% | 862 | 1732 | 0.0 | 0 |
| L08 | D | 1 / 1 | US$ 0.0659 | 0.0% | 4404 | 16155 | 0.0 | 0 |
| L09 | D | 1 / 1 | US$ 0.0615 | 0.0% | 4871 | 17430 | 0.0 | 0 |
| L10 | D | 2 / 2 | US$ 0.0591 | 0.0% | 5885 | 12280 | 0.0 | 0 |
| L12 | D | 1 / 1 | US$ 0.0470 | 0.0% | 3689 | 7252 | 0.0 | 0 |

CPST = custo de todas as tentativas válidas ÷ sucessos. Custo `relatado` vem do harness (numa assinatura é a estimativa do harness, não fatura); `estimado` usa os preços declarados nas premissas.
