# CPST — claude-code 2.1.328 (Claude Code) · claude-sonnet-5-5

Runs: aea0a14344f5. Premissas: `claude-code-relatado/1`.

| Braço | Válidas / total | Sucesso | Custo | CPST | p50 / p95 (s) | Adoção FF | Ferramentas/tent. |
|---|---|---|---|---|---|---|---|
| A | 6 / 6 | 6 (100.0%) | US$ 0.2515 | US$ 0.0419 | 27.7 / 32.3 | — | 3.2 |
| B | 6 / 6 | 6 (100.0%) | US$ 0.2650 | US$ 0.0442 | 29.5 / 33.1 | 0.0% | 3.5 |

| Comparação | Tarefas | Δ sucesso (IC95) | Razão CPST (IC95) | Gate |
|---|---|---|---|---|
| B_vs_A | 6 | +0.0% (+0.0% a +0.0%) | 1.05 (1.02–1.09) | reprovado: CPST maior que a base (IC95 1.02–1.09) |

CPST = custo de todas as tentativas válidas ÷ sucessos. Custo `relatado` vem do harness (numa assinatura é a estimativa do harness, não fatura); `estimado` usa os preços declarados nas premissas.
