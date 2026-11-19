# ledger

Razão financeiro pessoal em linha de comando: importa extratos bancários em CSV,
categoriza os lançamentos por regras e produz relatórios por categoria, conta e período.

> Projeto sintético usado como corpus do benchmark de agentes do FastFile. Não há dados
> reais: descrições, alíquotas e contas são inventadas.

## Uso rápido

```text
ledger categories extratos/2024-03.csv
ledger --locale en-US balance --depth 2 extratos/*.csv
ledger --fiscal-start 4 period --by ano --tax-rate 18 extratos/*.csv
ledger query --category mercado --from 2024-03-01 --to 2024-03-31 extratos/2024-03.csv
```

A configuração padrão é lida de `ledger.conf` no diretório atual (ver
[`ledger.conf.example`](ledger.conf.example)). Opções da linha de comando vencem o arquivo.

## Crates

| crate | responsabilidade |
|---|---|
| `lg-core` | dinheiro em unidades menores, moedas ISO 4217, datas, contas, arredondamento |
| `lg-parse` | CSV, extratos bancários, `ledger.conf` |
| `lg-rules` | padrões com curinga, motor de categorização, regras padrão |
| `lg-tax` | alíquotas em ppm, regiões fiscais, cálculo por linha ou por nota |
| `lg-fmt` | números por localidade, valores com símbolo, tabelas de texto |
| `lg-store` | armazenamento em memória com índices e consultas |
| `lg-report` | períodos fiscais, totais, renderização |
| `lg-cli` | argumentos, configuração efetiva, comandos, binário `ledger` |

Mais detalhes em [`docs/ARQUITETURA.md`](docs/ARQUITETURA.md).

## Testes

```text
cargo test --workspace
```

Não há dependências externas; o workspace compila sem rede.
