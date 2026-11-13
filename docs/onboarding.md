# Onboarding dos agentes (FastFile)

Prompts para colocar cada agente no projeto **já integrado ao ai-memory**. Cole o bloco do agente numa
sessão nova, aberta na raiz do repositório.

**Escopo deste projeto:** `workspace = "default"`, `project = "fabiomotta0311-fastfile"`
(está no [`.ai-memory.toml`](../.ai-memory.toml)). Nunca usar escopos de outros projetos nem legados.

**Pré-requisito fora do repo:** o plugin/MCP ai-memory de cada agente precisa estar habilitado e
apontando para o mesmo backend. O `.ai-memory.toml` define só *qual escopo* usar.

---

## DSH (Windows local, cliente MCP estático) — rode este primeiro

```
Projeto: FastFile — pasta C:\Users\fabio\Projects\fastfile
Escopo ai-memory (SEMPRE explícito em toda chamada de projeto): workspace="default", project="fabiomotta0311-fastfile".

1. Pré-checagem: as tools memory_* aparecem? Se não, PARE e me diga.
2. Marcador: o .ai-memory.toml já existe na raiz com esses valores. NÃO rode `ai-memory project init`
   nem sobrescreva — só confirme e me mostre o conteúdo.
3. Roteamento: chame memory_install_self_routing (escopo explícito). Grave o markered_block no AGENTS.md
   entre <!-- ai-memory:start --> e <!-- ai-memory:end -->, PRESERVANDO todo o resto do arquivo.
   Instale as skills gerenciadas que a tool retornar. Abaixo do bloco, fora dos marcadores, acrescente:
   "Neste projeto o DSH é cliente MCP estático: passe sempre workspace=default e
   project=fabiomotta0311-fastfile em toda chamada com escopo de projeto."
4. Confirme a captura: a sessão aparece como agent=dsh no projeto default/fabiomotta0311-fastfile
   (memory_read_session_observations). Se não, diagnostique antes de seguir.
5. Leia AGENTS.md e docs/ARQUITETURA.md. Trabalhe em Issues com label ai-ready sem blocked.
6. Ao fim do dia: lembre-me de rodar `ai-memory finalize-session --agent dsh`.

Entregue: conteúdo do .ai-memory.toml, diff do AGENTS.md (só o bloco + a linha do cliente estático),
skills instaladas e resultado do passo 4.
```

## Antigravity (Windows local)

```
Projeto: FastFile — abra C:\Users\fabio\Projects\fastfile (raiz).
Escopo ai-memory: workspace="default", project="fabiomotta0311-fastfile". Se suas tools pedem escopo
explícito, passe os dois em TODA chamada de projeto. Nunca use escopos legados (antigravity/*).
Confirme com ai_memory_status (ou equivalente) que project=fabiomotta0311-fastfile.

Regras: AGENTS.md (lei do repo) e docs/ARQUITETURA.md (o porquê). Pegue Issues ai-ready sem blocked.
Você está no Windows com admin disponível: é quem consegue rodar o gate (leitura da MFT exige admin) e
o `ff bench`. Cole o resultado do benchmark na Issue/PR.
Validação: cargo fmt --all -- --check · cargo clippy --workspace --all-targets -- -D warnings ·
cargo check --workspace · cargo test --workspace.
Privacidade: nunca commite índices, snapshots ou listas de arquivos reais da máquina.
```

## Hermes (VPS, headless)

```
Projeto: FastFile. Clone https://github.com/fabiomotta0311/fastfile na VPS e abra a raiz.
O .ai-memory.toml do repo já define workspace="default", project="fabiomotta0311-fastfile";
confirme com ai_memory_status antes de trabalhar. Uma sessão Hermes por repositório.

A VPS é Linux headless: NÃO há NTFS nem MFT. Pegue Issues de código portável — busca por nome sobre o
índice (com índice sintético), busca em conteúdo, diff, prévia de texto, formato do snapshot, testes.
Código de NTFS/USN fica para os agentes no Windows.
Validação: cargo fmt --all -- --check · cargo clippy --workspace --all-targets -- -D warnings ·
cargo check --workspace · cargo test --workspace.
```
