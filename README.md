<div align="center">

<img src="assets/fastfile-banner.svg" alt="FastFile Banner" width="100%" />

<br/>

[![Version](https://img.shields.io/badge/versão-v0.1.0-brightgreen.svg?style=for-the-badge&logo=rust)](https://github.com/fabiomotta0311/fastfile/releases/tag/v0.1.0)
[![Windows](https://img.shields.io/badge/Windows-NTFS%20%7C%20USN-0078D6.svg?style=for-the-badge&logo=windows)](https://github.com/fabiomotta0311/fastfile)
[![macOS (Experimental)](https://img.shields.io/badge/macOS-Experimental-orange.svg?style=for-the-badge&logo=apple&logoColor=white)](docs/macos.md)
[![Linux](https://img.shields.io/badge/Linux-Inotify%20%7C%20Fanotify-FCC624.svg?style=for-the-badge&logo=linux&logoColor=black)](https://github.com/fabiomotta0311/fastfile)
[![MCP Ready](https://img.shields.io/badge/MCP-Servidor%20Nativo-8A2BE2.svg?style=for-the-badge&logo=anthropic)](docs/mcp.md)
[![i18n](https://img.shields.io/badge/i18n-6%20Idiomas%20Nativos-blue.svg?style=for-the-badge)](#-internacionalização-e-acessibilidade-i18n)
[![Zero Telemetry](https://img.shields.io/badge/Privacidade-100%25%20Local-success.svg?style=for-the-badge)](README.md#privacidade)
[![License](https://img.shields.io/badge/Licen%C3%A7a-MIT%20%2F%20Apache--2.0-blue.svg?style=for-the-badge)](LICENSE-MIT)

<br/>

### **O canivete suíço definitivo de arquivos: busca instantânea em 2 ms, visualizador de disco com inteligência de limpeza, prévia nativa e diff lado a lado.**
*Desenvolvido nativamente em Rust. Feito para humanos no dia a dia e desenhado como a interface de arquivos ideal para Agentes de IA.*

<br/>

<img src="assets/fastfile-preview.svg" alt="FastFile Interface Desktop" width="100%" />

<br/>

<img src="assets/fastfile-features.svg" alt="FastFile Recursos Principais" width="100%" />

</div>

---

## 🎯 Por que o FastFile existe?

Quantos programas você precisa abrir ao longo de um dia de desenvolvimento para lidar com arquivos?

- 🔍 **Procurar arquivos num piscar de olhos?** Você abre o *Everything*.
- 📊 **Descobrir o que está lotando o seu SSD?** Você abre o *WizTree* ou *WinDirStat*.
- 👁️ **Achar trechos de código com prévia?** Você abre o *Agent Ransack* ou o VS Code.
- ⚖️ **Comparar dois arquivos rapidamente?** Você abre o *WinMerge* ou o *Meld*.
- 🤖 **E os seus Agentes de IA (Claude Code, Cursor, Antigravity, Hermes)?** Não usam nenhum deles: percorrem diretórios pelo terminal com `ls -la`, gastam milhares de tokens com pastas como `node_modules/` ou `target/` e correm o risco de ler arquivos com segredos (`.env`, chaves privadas).

> **O FastFile unifica tudo isso em um único aplicativo nativo em Rust:**
> Abre em milissegundos, consome quase zero de memória, tem interface gráfica fluida e moderna, atalhos no Windows Explorer e um **servidor MCP de primeira classe** para abastecer seus agentes autônomos com precisão cirúrgica.

---

## ⚡ Tabela Comparativa: 4 Ferramentas em 1

| Recurso | FastFile | Everything | WizTree | Agent Ransack | WinMerge |
| :--- | :---: | :---: | :---: | :---: | :---: |
| **Busca instantânea por nome (< 2 ms)** | ✅ *(MFT + USN)* | ✅ | ❌ | ❌ | ❌ |
| **Treemap visual proporcional de espaço** | ✅ *(Squarified)* | ❌ | ✅ | ❌ | ❌ |
| **Pré-visualização com Syntax Highlighting** | ✅ *(memmap2 + AST)* | Parcial *(só básica)* | ❌ | Parcial *(só texto)* | ❌ |
| **Diff Lado a Lado (com Location Pane)** | ✅ *(Ctrl+clique)* | ❌ | ❌ | ❌ | ✅ |
| **Busca em PDFs e DOCX nativa** | ✅ *(Extração direta)* | Parcial *(via IFilter)* | ❌ | ✅ | Parcial *(com plugin)* |
| **Diagnóstico de oportunidades de limpeza** | ✅ *(Regras explicáveis)* | ❌ | ❌ | ❌ | ❌ |
| **Simulação por meta & alertas de nuvem** | ✅ *(Consultivo)* | ❌ | ❌ | ❌ | ❌ |
| **Detecção de duplicados com hard links** | ✅ *(FNV-1a / BLAKE3)* | ❌ | ❌ | ❌ | ❌ |
| **Comparação de snapshots e crescimento** | ✅ *(Local)* | ❌ | ❌ | ❌ | ❌ |
| **Servidor MCP integrado para Agentes** | ✅ *(Nativo)* | ❌ | ❌ | ❌ | ❌ |
| **JSON estruturado com limite de tokens (`--budget`)** | ✅ *(Envelope `ff/1`)* | ❌ | ❌ | ❌ | ❌ |
| **Proteção automática contra vazamento de segredos** | ✅ *(Nativa)* | ❌ | ❌ | ❌ | ❌ |
| **Multiplataforma (Win, Mac e Linux)** | ✅ | ❌ *(Só Win)* | ❌ *(Só Win)* | ❌ *(Só Win)* | ❌ *(Só Win)* |

---

## 🚀 Instalação Rápida

### No Windows (Tier 1 — Recomendado e Totalmente Suportado)

Você pode instalar com um único comando pelo **Windows Package Manager (winget)**:

```powershell
winget install FabioMotta.FastFile
```

Ou baixar o pacote ZIP da [última release](https://github.com/fabiomotta0311/fastfile/releases/latest), extrair em qualquer pasta e executar o instalador autônomo (não requer Rust ou compiladores instalados):

```powershell
.\install.ps1
# ou
.\scripts\install.ps1
```

> **Pronto!** O FastFile estará no seu `PATH`, registrado nos menus de contexto do Windows Explorer e acessível a qualquer momento:
> - Pressione `Win + R` e digite `fastfile` (ou `ff`)
> - Ou pressione o atalho global **`Ctrl + Alt + F`** (ou `Win + Shift + F`) para focar a janela instantaneamente.

### No Linux / Servidores / VPS (Headless & SSH — Suportado)

O FastFile roda perfeitamente em modo CLI e servidor MCP sem interface gráfica no Linux:

```bash
# Compilação otimizada em release
cargo build --release --bin fastfile --bin ff

# Instalação no sistema
sudo cp target/release/fastfile target/release/ff /usr/local/bin/

# Consultas remotas ultrarrápidas via SSH por humanos ou agentes
ssh usuario@servidor fastfile du /var/log --top 10 --json
```

### No macOS (Apple Silicon M1/M2/M3/M4 & Intel — Suporte Experimental)

> ⚠️ **Nota:** A plataforma macOS está classificada como **experimental**. A interface gráfica e comandos CLI funcionam via varredura tradicional, e a integração com Homebrew está disponível.

Você pode instalar com o **Homebrew** ou via script universal:

```bash
# Via Homebrew
brew install fabiomotta0311/fastfile/fastfile

# Ou clonando e executando o instalador automático
git clone https://github.com/fabiomotta0311/fastfile.git
cd fastfile && ./scripts/install.sh
```

> No Mac, use **`Cmd + F`** para buscar e **`Cmd + clique`** para comparar arquivos no Diff. Veja o [guia do macOS (`docs/macos.md`)](docs/macos.md).

---

## 🤖 Feito para Agentes de IA (MCP & CLI)

O FastFile foi concebido desde a primeira linha de código para resolver o maior gargalo dos agentes de inteligência artificial modernos: **o consumo e desperdício de tokens de contexto**.

<div align="center">
  <img src="https://img.shields.io/badge/Economia%20de%20Tokens-70%25-success?style=for-the-badge" alt="Economia de Tokens" />
  <img src="https://img.shields.io/badge/Velocidade%20de%20Execu%C3%A7%C3%A3o-3.5x%20mais%20r%C3%A1pido-blue?style=for-the-badge" alt="Speedup" />
</div>

### 1. Conectando como Servidor MCP
Basta adicionar a configuração ao seu cliente compatível (**Claude Code**, **Antigravity**, **Cursor**, **Hermes**, **Windsurf**):

```json
{
  "mcpServers": {
    "fastfile": {
      "command": "fastfile",
      "args": ["mcp"]
    }
  }
}
```

### 2. Recursos Pensados para LLMs
- **Orçamento de Tokens (`--budget`):** Limite a resposta ao tamanho exato que a LLM pode processar. Os arquivos mais relevantes vêm primeiro, com resumo compacto e ponteiro de paginação (`cursor`).
- **Filtro Inteligente de Ruído:** Pastas de compilação e dependências (`target/`, `node_modules/`, `.git/`, `dist/`) são ignoradas por padrão dentro de repositórios.
- **Proteção Ativa de Segredos:** Mascaramento automático em tempo real para arquivos `.env`, chaves privadas (`id_rsa`, `*.pem`), tokens JWT e dumps de rede (`*.har`).
- **Somente Leitura e Seguro:** O FastFile não apaga, move ou altera arquivos. É 100% seguro para ser operado por agentes autônomos.

---

## 💻 Recursos da Interface Gráfica

### 🔍 Busca Enquanto Digita & Prévia Instantânea
- **Busca por nome em tempo real (< 2 ms):** Leitura de baixo nível da MFT no Windows (e varredura otimizada em Linux/macOS) com escuta contínua do USN Journal.
- **Barra de pasta/escopo alvo (`Path / Folder`):** Limite a busca dinamicamente a um subdiretório específico ou partição sem perder a velocidade do índice global.
- **Pílulas de filtro rápido:** Código, Documentos, Mídia, Compactados ou > 100 MB.
- **Tabela virtualizada com splitter redimensionável:** Controle individual de largura das colunas, tempo de busca exibido no rodapé (ex: `em 1,2 ms`) e exportação direta da seleção ou lista completa em **CSV**.
- **Pré-visualização não bloqueante por memória mapeada (`memmap2`):**
  - Realce de sintaxe em dezenas de linguagens (`syntect`) com detecção inteligente de tipo.
  - Extração nativa de texto para documentos **PDF e DOCX**.
  - Hex Dump formatado para arquivos binários grandes, sem risco de exaustão de memória RAM.
  - Resiliência total contra arquivos desaparecidos ou temporariamente bloqueados pelo sistema operacional.

### 📊 Treemap Inteligente de Uso de Disco & Otimização de Espaço
- **Mapa térmico retangular proporcional (Squarified Treemap):** Visualização geométrica que reflete o peso real de pastas e arquivos no disco, com drill-down por clique duplo e atalhos rápidos (`C:\`, `D:\`, `Usuário`, `Downloads`).
- **Varredura assíncrona cooperativa:** Execução fluida em segundo plano com barra de progresso em tempo real, cancelamento instantâneo e retenção de análise parcial caso haja pastas bloqueadas por permissão.
- **Lista ordenada sincronizada bidirecionalmente:** Tabela virtualizada conectada ao mapa térmico: selecionar na lista destaca o bloco no mapa e vice-versa, permitindo acesso imediato a arquivos minúsculos que não teriam área de clique no treemap.
- **Três métricas físicas reais de espaço:** Distinção explícita entre *Tamanho Lógico* (nominal), *Espaço Alocado* (físico em clusters) e *Recuperação Estimada* (espaço efetivamente liberável em disco, descontando hard links compartilhados e compressão NTFS). Nunca exibe valor desconhecido como zero.
- **Painel de inspeção e ações rápidas:** Metadados completos do item selecionado, datas (criação, modificação, acesso), contagem de itens internos, maiores arquivos contidos, abrir localização no explorador, copiar caminho e prévia integrada.
- **Filtros orientados à recuperação de espaço:** Presets inteligentes explicáveis (*Maiores arquivos*, *Downloads antigos por idade de modificação*, *Instaladores e imagens ISO*, *Vídeos e gravações grandes*, *Arquivos compactados*, *Itens sem modificação recente*) e filtros combinados customizados.
- **Motor de regras explicáveis de oportunidades de limpeza:** Reconhecimento automático contextual de artefatos de build (`target`, `node_modules`, `.gradle`, `venv`), caches conhecidos, instaladores, dados de aplicação (`.git`, bancos de dados) e sistema operacional protegido. *Conteúdo pessoal nunca é chamado de lixo.*
- **Detecção de duplicados integrada à análise visual:** Hashing por conteúdo sob demanda (FNV-1a / BLAKE3) com cancelamento e barra de progresso, diferenciando hard links de cópias reais e sem pré-seleção forçada de arquivos pessoais.
- **Simulação de recuperação consultiva:** Modo 100% consultivo (o FastFile nunca apaga nem altera arquivos). Deduplicação hierárquica automática (marcar pasta pai e filho não duplica a estimativa) e exportação da simulação completa para CSV RFC 4180.
- **Modos de cor com legendas e indicadores textuais:** Alternância rápida entre *Tipo de Arquivo*, *Idade de Modificação* (<7d, <30d, <90d, <180d, >180d) e *Categoria de Oportunidade*, com legenda interativa e badges textuais nos blocos para acessibilidade.
- **Snapshots locais e comparação de crescimento:** Histórico de ocupação com retenção configurável, ranking de pastas que mais cresceram, novos arquivos e arquivos removidos entre análises do mesmo escopo. Falta de permissão não é confundida com espaço liberado.
- **Locais protegidos e exclusões de sugestões:** Opções "Ignorar esta sugestão" e "Não sugerir limpeza nesta pasta", persistidas localmente em JSON com controle para revisar e desfazer exclusões.
- **Planejamento por meta e alternativas de nuvem:** Meta de recuperação definida pelo usuário com presets rápidos e barra de progresso visual. Orientação de alternativas responsáveis: **distinção vital entre liberar espaço local em nuvem (OneDrive, Dropbox, iCloud) mantendo o arquivo na nuvem vs excluir arquivo sincronizado (que remove de todos os dispositivos)**. Orientações contextuais para comandos de ecossistemas (`cargo clean`, `npm prune`, `git gc`, `docker system prune`).

### ⚖️ Diff Lado a Lado Estilo WinMerge
- **Comparação instantânea:** Selecione dois arquivos com `Ctrl + clique` (ou `Cmd + clique` no macOS) ou envie pelo menu de contexto do Windows Explorer.
- **Location Pane lateral:** Uma barra de navegação visual que mapeia todas as inserções, remoções e alterações do arquivo completo.
- **Realce intra-linha:** Identificação caractere a caractere ou palavra por palavra das mudanças dentro de uma linha modificada.
- **Divisória redimensionável e rolagem sincronizada:** Suporte a arquivos de qualquer tamanho com atalhos de salto entre diferenças.

### 🌐 Internacionalização e Acessibilidade (i18n)
- **6 Idiomas Nativos:** Suporte completo a Português (PT-BR), Inglês (EN), Espanhol (ES), Francês (FR), Italiano (IT) e Russo (RU).
- **Detecção automática:** Identifica a preferência de idioma do sistema operacional na inicialização e oferece alternador dinâmico no topo da interface.

### ⌨️ Atalhos Globais e Facilidade de Acesso
- **Atalho global de foco:** Pressione **`Ctrl + Alt + F`** (ou `Win + Shift + F`) no Windows para focar a janela instantaneamente de qualquer lugar.
- **Executável universal:** Pressione `Win + R` e digite `fastfile` (ou `ff`).
- **Integração com o Windows Explorer:** Menus de contexto de botão direito para abrir localização, buscar em pastas ou comparar arquivos diretamente.

---

## 🛠️ CLI Rápida para o Terminal (`ff`)

Para quem prefere a linha de comando no terminal ou scripts de automação:

```powershell
# Buscar arquivos por padrão, extensão ou regex
ff find "*.rs"
ff find "report" --type doc

# Consulta com saída JSON compacta e controle de orçamento de tokens para Agentes
ff find "*.toml" --json --compact --budget 500

# Diagnóstico de espaço ocupado (estilo 'du' ultra veloz)
ff du C:\Projetos --top 10

# Busca de conteúdo dentro de arquivos (grep de alta performance)
ff grep "fn parse_mft" --type rust

# Encontrar onde uma função, classe ou struct está definida via AST
ff sym AuthenticatedUser

# Visão panorâmica e compacta da estrutura de um repositório
ff map . --budget 2000

# Detecção instantânea de arquivos duplicados por tamanho e hash
ff dupes C:\Downloads
```

---

## 📊 Benchmarks de Desempenho

Resultados medidos comparando agentes autônomos executando tarefas reais de engenharia com e sem o FastFile:

| Tarefa do Agente | Sem FastFile | Com FastFile | Ganho de Produtividade |
| :--- | :---: | :---: | :---: |
| **Diagnóstico de espaço em disco** | 18,4 s *(4.850 tokens, 5 chamadas)* | **5,2 s** *(1.410 tokens, 1 chamada)* | **3,5× mais rápido · -70% tokens** |
| **Localizar arquivos de configuração** | 14,5 s *(4.200 tokens, 4 chamadas)* | **4,9 s** *(1.300 tokens, 1 chamada)* | **3,0× mais rápido · -69% tokens** |
| **Localizar definição de símbolos (AST)** | 13,8 s *(3.950 tokens, 4 chamadas)* | **4,6 s** *(1.250 tokens, 1 chamada)* | **3,0× mais rápido · -68% tokens** |
| **Grep em repositório extenso** | 12,1 s *(3.640 tokens, 4 chamadas)* | **4,4 s** *(1.320 tokens, 1 chamada)* | **2,8× mais rápido · -64% tokens** |

> *Metodologia detalhada e relatórios reproduzíveis disponíveis em [`bench/agentes/RESULTADOS.md`](bench/agentes/RESULTADOS.md).*

---

## 🔒 Privacidade em Primeiro Lugar

- **Zero Telemetria:** O FastFile não possui telemetria, não realiza chamadas para a internet e não coleta dados analíticos.
- **O índice é 100% seu:** O índice de arquivos é estritamente local e armazenado com permissões restritas ao usuário corrente.
- **Menor Privilégio:** Apenas o indexador de baixo nível opcional utiliza privilégios elevados para leitura da MFT; a interface gráfica, as prévias e os diffs rodam como usuário comum desprivilegiado.

---

## 📚 Documentação Técnica

- 🏛️ [**Arquitetura do Sistema (`docs/ARQUITETURA.md`)**](docs/ARQUITETURA.md) — Detalhes da MFT, USN Journal, SoA e decisões de design.
- 🍏 [**Guia do macOS (`docs/macos.md`)**](docs/macos.md) — Instalação no Mac (Apple Silicon & Intel), Homebrew e Launchd.
- 🤖 [**Guia do Servidor MCP (`docs/mcp.md`)**](docs/mcp.md) — Configuração passo a passo para Claude Code, Antigravity e Cursor.
- 🌐 [**Uso Remoto via SSH (`docs/remoto.md`)**](docs/remoto.md) — Guia de execução em servidores Linux headless.
- 📦 [**Distribuição e Empacotamento (`docs/distribuicao-lojas.md`)**](docs/distribuicao-lojas.md) — WinGet, Scoop, AUR e Flatpak.

---

## 📄 Licença

Distribuído sob licença dupla **MIT** ou **Apache-2.0**. Consulte os arquivos de licença para maiores detalhes.
