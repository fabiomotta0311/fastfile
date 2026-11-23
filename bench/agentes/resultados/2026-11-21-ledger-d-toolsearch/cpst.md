# CPST — claude-code 2.1.328 (Claude Code) · claude-sonnet-5-5

Runs: 2e7fdad2f1b9, dfc704c45191. Premissas: `claude-code-relatado/1`.

| Braço | Válidas / total | Sucesso | Custo | CPST | p50 / p95 (s) | Adoção FF | Ferramentas/tent. |
|---|---|---|---|---|---|---|---|
| D | 12 / 12 | 12 (100.0%) | US$ 0.8036 | US$ 0.0670 | 43.0 / 61.2 | 8.3% | 4.4 |

| Braço | Bytes/tent. | Ponderado/tent. (p50 / p95) | % FastFile | Respostas > 8 KiB | Releituras | Repetidas |
|---|---|---|---|---|---|---|
| D | 4791 | 14028 (12736 / 29590) | 9.4% | 0.0 | 0.0 | 0.0 |

| Tarefa | Braço | Sucesso / válidas | CPST | Adoção FF | Bytes | Ponderado | Releituras | Retentativas |
|---|---|---|---|---|---|---|---|---|
| L05 | D | 2 / 2 | US$ 0.0570 | 0.0% | 4268 | 10843 | 0.0 | 0 |
| L06 | D | 2 / 2 | US$ 0.0457 | 0.0% | 979 | 1966 | 0.0 | 0 |
| L08 | D | 2 / 2 | US$ 0.0656 | 50.0% | 5788 | 16218 | 0.0 | 0 |
| L09 | D | 2 / 2 | US$ 0.0650 | 0.0% | 6906 | 25162 | 0.0 | 0 |
| L10 | D | 2 / 2 | US$ 0.1085 | 0.0% | 5710 | 11634 | 0.0 | 0 |
| L12 | D | 2 / 2 | US$ 0.0600 | 0.0% | 5093 | 18346 | 0.0 | 0 |

CPST = custo de todas as tentativas válidas ÷ sucessos. Custo `relatado` vem do harness (numa assinatura é a estimativa do harness, não fatura); `estimado` usa os preços declarados nas premissas.
