# Regras de categorização

## Sintaxe dos padrões

| padrão | casa com |
|---|---|
| `NETFLIX` | exatamente `NETFLIX` (ignorando maiúsculas e espaços extras) |
| `UBER*` | `UBER`, `UBER TRIP 123`, `UBER EATS` |
| `*FARMACIA*` | qualquer descrição que contenha `FARMACIA` |
| `LOJA ?` | `LOJA 1`, `LOJA A` (exatamente um caractere) |

`*` casa com zero ou mais caracteres, inclusive no fim: `PADARIA*` casa com a descrição
`PADARIA` sozinha. Espaços repetidos na descrição contam como um só, porque vários
bancos completam o campo com espaços.

## Desempate

Quando mais de uma regra casa com a descrição:

1. vence a de maior prioridade (`[10] PADRAO => categoria`);
2. em empate de prioridade, vence a mais específica, isto é, a que tem mais caracteres
   literais (`MERCADO LIVRE*` vence `MERCADO*`);
3. persistindo o empate, vence a definida primeiro. As regras do usuário são carregadas
   antes das regras padrão.

## Regras padrão

Ficam em `crates/lg-rules/src/builtin.rs`, geradas a partir de `docs/regras-padrao.csv`.
Desligue com `builtin = nao` na seção `[rules]` do `ledger.conf`.
