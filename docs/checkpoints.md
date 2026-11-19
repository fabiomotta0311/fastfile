# Checkpoint e delta local (experimental)

`fastfile checkpoint --experimental --root .` registra uma observação dos arquivos regulares da raiz. Guarde **somente o objeto `checkpoint`** no estado privado do harness. Envie esse objeto como JSON via stdin (feche o pipe ao terminar) para `fastfile changes --experimental --root . --since -`. Compare usando a mesma raiz e as mesmas opções. Use `--timeout <segundos>` para limitar uma chamada; `TIMEOUT` é cancelamento, não um delta. FastFile não cria arquivos de estado e não usa índice residente, Git CLI, rede ou journal.

## Resultado

O contrato experimental imprime um objeto JSON com `schema: "ff/2"`, `source: "portable_snapshot"`, `command` e `limitations`. `ff/1`, os demais comandos e as ferramentas MCP não mudam.

| `status` | O que fazer |
|---|---|
| `checkpoint_created` | Guarde `checkpoint` até `expires_at` (segundos Unix); TTL de uma hora. |
| `observed_changes` | Consuma `events`, ordenados por caminho, com `kind` igual a `added`, `modified` ou `deleted`. `path` segue `PATH-CWD-001`, mesmo para um arquivo excluído. |
| `no_observed_changes` | `events: []`: nenhum delta de conteúdo foi observado no escopo. Isso não atesta que nada aconteceu. |
| `recreate_checkpoint` | Descarte o estado e execute `checkpoint` novamente (`action: "run_checkpoint_again"`). `reason` indica estado inválido/expirado, versão, raiz/checkout ou política incompatível. |
| `indeterminate` | `coverage: "incomplete"`, sem checkpoint e sem eventos. Resolva a causa (`reason`, por exemplo `access_denied`, `link_or_reparse_point` ou `byte_budget`) antes de tirar conclusões. |

Esses status são respostas estruturadas com saída 0; falhas de invocação e timeout têm saída não zero. Uma leitura sem estado válido nunca cria um novo checkpoint implicitamente. Para avançar a referência, peça outro `checkpoint` de propósito.

## Escopo, identidade e limites

- O estado é serializável entre processos, mas vale apenas para a mesma raiz canônica e identidade do diretório. Na raiz de um checkout, `.git` identifica o diretório de metadados ou o arquivo marcador de um worktree; o destino de um marcador externo não é resolvido. O HEAD não é fixado: trocar de branch no mesmo checkout pode produzir um delta. Em subdiretórios sem marcador usa-se a identidade do próprio diretório.
- Leia o estado como opaco, gerencie-o localmente e não publique nem registre: contém nomes relativos e impressões digitais SHA-256; não contém conteúdo, destinos de links ou caminho absoluto da raiz. O estado **não é assinado**: use apenas o emitido pelo seu harness, não aceite checkpoints de terceiros como evidência. Nenhum caminho recebido no checkpoint autoriza uma leitura.
- Regras `.gitignore` e `.ignore` sob a raiz são aplicadas antes da coleta e entram na identidade da política. Regras em diretórios-pai, globais do Git e fora da raiz não são carregadas. Prefira a raiz do checkout a um subdiretório para incluir as regras do projeto. Alterar a política observada exige recriar o checkpoint.
- Pastas de build/dependências conhecidas e `.git` ficam de fora. `--include-artifacts` abre exceção **apenas** em `target/`, `build/` e `dist/` imediatamente sob a raiz, ainda excluindo diretórios de dependência/cache aninhados. Nomes de arquivos ou diretórios reconhecidos pela proteção de segredos continuam excluídos; não existe `--include-secrets` para esses comandos. A proteção é heurística, não um detector de todo conteúdo sensível. Use apenas em árvores locais confiáveis.
- A coleta é limitada a 20 mil entradas, 32 níveis, 1024 bytes por caminho portável, 64 KiB por arquivo de controle e 256 MiB de leitura. O estado e o conjunto de eventos são limitados a 2 MiB cada, sem contar o pequeno envelope. Se um limite impedir a coleta/comparação, não há delta parcial.
- São duas observações não atômicas, não um histórico. Edições com mesmo tamanho/mtime são detectadas pelo digest; mudanças que retornaram aos mesmos bytes, mudanças só de metadados e eventos transitórios não são observáveis. Renomear é exclusão + inclusão, sem inferência por digest. Links/reparse points em escopo não são seguidos, erros e corridas detectadas invalidam o resultado; não há garantia contra todas as mutações concorrentes ou contra topologias de montagem especiais.
