# Contribuindo com o FastFile

Leia antes [`AGENTS.md`](AGENTS.md) (regras estáveis) e [`docs/ARQUITETURA.md`](docs/ARQUITETURA.md).

## Fluxo
1. Abra uma issue (ou comente numa existente) antes de mudanças grandes.
2. Faça um branch a partir de `main` e abra um PR pequeno e focado.
3. Atualize o [`CHANGELOG.md`](CHANGELOG.md) em "Não lançado" quando a mudança for visível ao usuário.

## Validação (obrigatória antes do PR)
```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```
Mudanças em `ff-index`/`ff-search`: rode também `ff bench` (Windows, como administrador) e cole o resultado no PR.

## Regras que o CI impõe
- Todo `unsafe` precisa de um comentário `// SAFETY:` imediatamente acima (lint negado no workspace).
- Sem rede, telemetria ou upload no binário. Nunca versione índices, snapshots ou listas de arquivos reais.
- Mudança incompatível no JSON do CLI exige nova versão do esquema (`ff/2`).
- Código específico de Windows fica atrás de `#[cfg(windows)]`; o resto deve compilar e testar em Linux.

## Segurança
Não abra issue pública para vulnerabilidades: veja [`SECURITY.md`](SECURITY.md).
