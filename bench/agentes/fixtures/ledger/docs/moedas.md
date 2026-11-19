# Moedas

A tabela `lg-core/src/currency_table.rs` segue a ISO 4217 (moedas ativas). O campo
`minor_units` define quantas casas a unidade menor tem e, portanto, como valores são
lidos, arredondados e exibidos:

| casas | exemplos |
|---|---|
| 0 | JPY, KRW, CLP, PYG, VND, ISK, UGX, XAF, XOF |
| 2 | BRL, USD, EUR, GBP e a maioria |
| 3 | BHD, IQD, JOD, KWD, LYD, OMR, TND |

Exemplos de exibição (`lg-fmt::format_amount`):

| valor | localidade | texto |
|---|---|---|
| 123456 BRL | pt-BR | `R$ 1.234,56` |
| -1200 USD | en-US | `-$12.00` |
| 1234 JPY | en-US | `¥1,234` |
| 1234 KWD | en-US | `KWD 1.234` |

Moedas sem símbolo próprio são exibidas com o código ISO seguido de espaço.
