# Política de segurança

## Versões suportadas
Recebe correções de segurança a última versão publicada (`main` e a release mais recente).

## Reportando uma vulnerabilidade
**Não abra issue pública.** Use o recurso *Private vulnerability reporting* do GitHub
(aba **Security → Report a vulnerability** em https://github.com/fabiomotta0311/fastfile/security/advisories/new).

Inclua: versão, sistema operacional, passos para reproduzir e impacto. Resposta inicial em até 7 dias.

## Escopo de especial interesse
- O indexador elevado (`ff-indexer`) e a IPC por *named pipe* (só o usuário que iniciou pode conectar).
- Vazamento de segredos pelo CLI/MCP (proteção ligada por padrão).
- Parsers de entrada não confiável: PDF, DOCX/zip, MFT/USN.
- Qualquer comportamento de rede: o `ff` não deve fazer nenhuma.
