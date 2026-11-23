# CPST — claude-code 2.1.328 (Claude Code) · claude-sonnet-5-5

Runs: 5caefc6caddd. Premissas: `claude-code-relatado/1`.

| Braço | Válidas / total | Sucesso | Custo | CPST | p50 / p95 (s) | Adoção FF | Ferramentas/tent. |
|---|---|---|---|---|---|---|---|
| A | 6 / 6 | 6 (100.0%) | US$ 0.3359 | US$ 0.0560 | 45.2 / 58.9 | — | 3.3 |
| B | 6 / 6 | 6 (100.0%) | US$ 0.3296 | US$ 0.0549 | 47.0 / 51.7 | 0.0% | 3.0 |
| D | 6 / 6 | 6 (100.0%) | US$ 0.4129 | US$ 0.0688 | 45.6 / 48.1 | 0.0% | 4.3 |

| Comparação | Tarefas | Δ sucesso (IC95) | Razão CPST (IC95) | Gate |
|---|---|---|---|---|
| B_vs_A | 6 | +0.0% (+0.0% a +0.0%) | 0.98 (0.93–1.03) | inconclusivo: redução de CPST não demonstrada (IC95 até 1.03, alvo ≤ 0.90) |
| D_vs_B | 6 | +0.0% (+0.0% a +0.0%) | 1.25 (1.01–1.69) | diagnostico: uso obrigatório; fora do gate (#120) |

| Braço | Bytes/tent. | Ponderado/tent. (p50 / p95) | % FastFile | Respostas > 8 KiB | Releituras | Repetidas |
|---|---|---|---|---|---|---|
| A | 5947 | 17221 (14434 / 36814) | 0.0% | 0.0 | 0.0 | 0.0 |
| B | 5059 | 13504 (10480 / 30906) | 0.0% | 0.0 | 0.0 | 0.0 |
| D | 4135 | 12609 (12094 / 21756) | 0.0% | 0.0 | 0.0 | 0.0 |

| Comparação | Razão do volume ponderado (IC95) |
|---|---|
| B_vs_A | 0.78 (0.63–0.90) |
| D_vs_B | 0.93 (0.71–1.36) |

| Tarefa | Braço | Sucesso / válidas | CPST | Adoção FF | Bytes | Ponderado | Releituras | Retentativas |
|---|---|---|---|---|---|---|---|---|
| L05 | A | 1 / 1 | US$ 0.0546 | — | 4827 | 12431 | 0.0 | 0 |
| L05 | B | 1 / 1 | US$ 0.0529 | 0.0% | 3902 | 9656 | 0.0 | 0 |
| L05 | D | 1 / 1 | US$ 0.0565 | 0.0% | 3879 | 8599 | 0.0 | 0 |
| L06 | A | 1 / 1 | US$ 0.0410 | — | 762 | 1398 | 0.0 | 0 |
| L06 | B | 1 / 1 | US$ 0.0433 | 0.0% | 762 | 1398 | 0.0 | 0 |
| L06 | D | 1 / 1 | US$ 0.0475 | 0.0% | 1148 | 2304 | 0.0 | 0 |
| L08 | A | 1 / 1 | US$ 0.0621 | — | 6526 | 18763 | 0.0 | 0 |
| L08 | B | 1 / 1 | US$ 0.0537 | 0.0% | 4067 | 9978 | 0.0 | 0 |
| L08 | D | 1 / 1 | US$ 0.0576 | 0.0% | 4067 | 10997 | 0.0 | 0 |
| L09 | A | 1 / 1 | US$ 0.0720 | — | 11484 | 42831 | 0.0 | 0 |
| L09 | B | 1 / 1 | US$ 0.0704 | 0.0% | 10176 | 37307 | 0.0 | 0 |
| L09 | D | 1 / 1 | US$ 0.0672 | 0.0% | 4791 | 23231 | 0.0 | 0 |
| L10 | A | 1 / 1 | US$ 0.0550 | — | 6287 | 16436 | 0.0 | 0 |
| L10 | B | 1 / 1 | US$ 0.0555 | 0.0% | 5532 | 10982 | 0.0 | 0 |
| L10 | D | 1 / 1 | US$ 0.1249 | 0.0% | 6288 | 17331 | 0.0 | 0 |
| L12 | A | 1 / 1 | US$ 0.0512 | — | 5797 | 11468 | 0.0 | 0 |
| L12 | B | 1 / 1 | US$ 0.0539 | 0.0% | 5915 | 11704 | 0.0 | 0 |
| L12 | D | 1 / 1 | US$ 0.0592 | 0.0% | 4639 | 13192 | 0.0 | 0 |

CPST = custo de todas as tentativas válidas ÷ sucessos. Custo `relatado` vem do harness (numa assinatura é a estimativa do harness, não fatura); `estimado` usa os preços declarados nas premissas.
