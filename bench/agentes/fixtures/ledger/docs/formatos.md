# Formatos de entrada

## Extratos (CSV)

- Separador detectado no cabeçalho entre `;`, `,` e tabulação (o que mais aparece fora
  de aspas).
- Campos podem vir entre aspas duplas. Dentro das aspas o separador é literal e duas
  aspas seguidas (`""`) representam uma aspa na descrição:
  `05/03/2024;"LOJA ""CENTRAL"" LTDA";-10,00` tem a descrição `LOJA "CENTRAL" LTDA`.
- Colunas obrigatórias (por nome, sem diferenciar maiúsculas e acentos): `data`,
  `descricao`/`historico`, `valor`. Opcionais: `moeda`, `categoria`.
- Datas em `AAAA-MM-DD` ou `DD/MM/AAAA`. Datas impossíveis (30/02) são erro com o número
  da linha; 29/02 só existe em anos bissextos (divisíveis por 4, exceto os seculares não
  divisíveis por 400: 2000 é bissexto, 1900 não).
- Valores aceitam `1.234,56`, `1,234.56`, `-12,5` e o prefixo `R$`. Casas além das da
  moeda são arredondadas pelo modo configurado.
- Linhas vazias e linhas começadas por `#` são ignoradas.

## Configuração (`ledger.conf`)

Estilo INI; ver `ledger.conf.example`. Chaves são lidas como `secao.chave`. Comentários
com `#` ou `;` no início da linha, ou ` #` depois do valor.

## Regras (`regras.txt`)

Uma por linha, `[prioridade] PADRAO => categoria`; ver `docs/regras.md`.
