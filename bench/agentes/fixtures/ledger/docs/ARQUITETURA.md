# Arquitetura do ledger

## Fluxo de um comando

```text
argumentos ──► lg-cli::args ──► Invocation
ledger.conf ─► lg-parse::config ─┐
                                 ├─► lg-cli::settings::Settings (padrões < arquivo < flags)
extratos CSV ─► lg-parse::statement ─► Vec<Txn>
                                        │
regras (usuário + padrão) ─► lg-rules::Engine::apply
                                        │
                                        ▼
                              lg-store::Store (índices por conta, categoria e mês)
                                        │
                     ┌──────────────────┼──────────────────┐
                     ▼                  ▼                  ▼
             lg-store::Query    lg-report::summary   lg-report::group
                     │                  │                  │
                     └──────────► lg-report::render ◄──────┘
                                        │
                                  lg-fmt (números, tabelas)
```

## Decisões

### Dinheiro sem ponto flutuante

`lg_core::Money` guarda unidades menores em `i64` junto com a referência estática à
moeda. A quantidade de casas vem de `Currency::minor_units` (tabela ISO 4217 em
`lg-core/src/currency_table.rs`). Operações entre moedas diferentes falham com
`Error::CurrencyMismatch` em vez de converter implicitamente.

### Arredondamento centralizado

Toda divisão que pode produzir fração de unidade menor passa por
`lg_core::rounding::div_round`. Os módulos de imposto (`lg-tax`), leitura de valores
(`Money::parse`) e rateio (`allocate`) não arredondam por conta própria. O modo
padrão é o bancário (`HalfEven`); regiões fiscais podem exigir outro (`lg-tax/src/regions.rs`).

O arredondamento é simétrico em torno de zero: um estorno precisa gerar exatamente o
imposto da compra original com o sinal trocado. Testes de imposto devem cobrir valores
positivos e negativos.

### Apresentação separada do cálculo

`lg-fmt` não conhece regras de negócio: recebe `Money` e `Locale` e devolve texto. As
larguras de tabela são medidas em caracteres, não em bytes (descrições de extratos têm
acentos).

### Índices do armazenamento

`lg-store::Store` mantém três índices (conta, categoria, mês). Toda operação que muda
uma transação (inserir, remover, trocar categoria) precisa atualizar os três. As
consultas escolhem um índice como ponto de partida e depois aplicam todos os filtros;
um id de índice que aponte para slot vazio é tratado como corrupção (`panic`).

### Exercício fiscal

`lg-report::group::FiscalCalendar` define o exercício pelo mês de início. O ano do
exercício é o ano civil em que ele começa: com início em abril, `FY2024` vai de
2024-04-01 a 2025-03-31, e o próprio mês de abril pertence ao exercício que começa nele.

## Onde mexer

| quero... | arquivo |
|---|---|
| aceitar um novo formato de data | `lg-core/src/date.rs` |
| reconhecer outro nome de coluna | `lg-parse/src/statement.rs` (`map_columns`) |
| mudar a ordem de desempate das regras | `lg-rules/src/engine.rs` (`rank`) |
| incluir uma localidade | `lg-fmt/src/locale.rs` |
| novo filtro de consulta | `lg-store/src/query.rs` e `lg-cli/src/args.rs` |
| nova chave de configuração | `lg-cli/src/settings.rs` e `ledger.conf.example` |
