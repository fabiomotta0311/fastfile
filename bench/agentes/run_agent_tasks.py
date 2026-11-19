#!/usr/bin/env python3
"""Braços A/B/C do benchmark CPST (issue #87): uma linha JSONL por tentativa.

A = ferramentas nativas do harness; B = FastFile atual via MCP (uso opcional);
C = B + capacidades experimentais (`FASTFILE_EXPERIMENTAL`). Entre braços só muda o
catálogo MCP: mesmo prompt, modelo, permissões, orçamento e checkout novo por tentativa.

Somente o harness confiável roda este script, nunca dentro do checkout do agente.
"""

import argparse
import difflib
import hashlib
import json
import os
import random
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

import corpus

SCHEMA = "fastfile-attempt/1"
RUNNER_VERSION = 1
BRACOS = ("A", "B", "C")
EXPERIMENTAIS = frozenset({"repair_context", "validate_plan"})
FF_EXPERIMENTAL = "context,validate_plan"
NOMES_FF = ("ff", "fastfile", "ff-indexer", "ff-ui")
PREFIXO_MCP_FF = "mcp__fastfile__"
GRADE_PATCH = Path(__file__).resolve().parent / "grade_patch.py"
# Gerados pelo agente ao rodar `cargo test` e recriados pelo avaliador: fora do diff.
GERADOS = frozenset({"target", "Cargo.lock"})
# Marcas de acesso ao material privado do avaliador (gabaritos, mutações, oráculos).
MARCAS_PRIVADAS = ("gabaritos.json", "avaliador/correcoes", "avaliador\\correcoes", "correcoes.lock")
# Runtime de uma sessão do Claude Code que esteja executando o runner: nunca chega ao agente.
PREFIXOS_SESSAO = ("CLAUDE_CODE_", "CLAUDE_AGENT_SDK_", "CLAUDE_PID", "CLAUDE_EFFORT", "CLAUDE_PREVIEW_")

PROMPT_VERSAO = 1
PROMPT_MODELO = (
    "Você está na raiz de um workspace Rust com um defeito.\n\n"
    "Tarefa: {prompt}\n\n"
    "Edite os arquivos-fonte para corrigir o defeito. Comando de teste: `{comando}`.\n"
    "Não há rede: dependências externas não estão disponíveis. Ao terminar, responda com um "
    "resumo de uma linha."
)


class ErroConfiguracao(Exception):
    """Configuração que invalidaria a comparação entre braços: nada é executado."""


# ------------------------------------------------------------------ planejamento


def tarefas_selecionadas(ids=None, incluir_holdout=False):
    """Tarefas do piloto (P01–P10, L01–L12); o holdout só entra com pedido explícito (#100)."""
    todas = corpus.carregar_correcoes()
    divisao = corpus.divisao(todas)
    permitidas = [t for t in todas if incluir_holdout or t["familia"] in divisao["piloto"]]
    if not ids:
        return permitidas
    por_id = {t["id"]: t for t in todas}
    escolhidas = []
    for i in ids:
        if i not in por_id:
            raise ErroConfiguracao(f"tarefa desconhecida: {i}")
        if por_id[i] not in permitidas:
            raise ErroConfiguracao(f"{i} é do holdout: use --incluir-holdout só na avaliação final")
        escolhidas.append(por_id[i])
    return escolhidas


def planejar(tarefas, repeticoes, bracos=BRACOS, semente=0):
    """Ordem balanceada: em cada repetição a ordem das tarefas é embaralhada e a dos braços
    gira (quadrado latino), para que nenhum braço rode sempre primeiro ou depois de outro."""
    rng = random.Random(semente)
    plano = []
    for rep in range(repeticoes):
        ordem = list(tarefas)
        rng.shuffle(ordem)
        for pos, tarefa in enumerate(ordem):
            k = (rep + pos) % len(bracos)
            for braco in bracos[k:] + bracos[:k]:
                plano.append({"tarefa": tarefa["id"], "braco": braco, "repeticao": rep})
    for i, item in enumerate(plano):
        item["ordem"] = i
    return plano


# ------------------------------------------------------------------ braços


def _executaveis_ff(diretorio):
    exts = [""] + (os.environ.get("PATHEXT", ".EXE").lower().split(";") if os.name == "nt" else [])
    d = Path(diretorio)
    return [d / (n + e) for n in NOMES_FF for e in exts if (d / (n + e)).is_file()]


def path_sem_fastfile(path, harness):
    """PATH sem diretórios que contenham binários do FastFile (o agente só o vê via MCP).
    Falha se o harness divide diretório com o FastFile: removê-lo quebraria o braço."""
    partes = [p for p in path.split(os.pathsep) if p]
    limpo = [p for p in partes if not _executaveis_ff(p)]
    novo = os.pathsep.join(limpo)
    for nome in NOMES_FF:
        if shutil.which(nome, path=novo):
            raise ErroConfiguracao(f"binário do FastFile ainda visível no PATH: {nome}")
    if harness and shutil.which(harness, path=os.pathsep.join(partes)) and not shutil.which(harness, path=novo):
        raise ErroConfiguracao("o harness está no mesmo diretório que o FastFile; separe-os")
    return novo


def _de_sessao_hospedeira(nome):
    n = nome.upper()
    return n == "CLAUDECODE" or n.startswith(PREFIXOS_SESSAO)


def exigir_fora_de_sessao(env):
    """O harness real precisa de sessão própria: dentro de outra sessão do Claude Code ele
    recusa iniciar (e compartilharia runtime). Rode o runner num terminal comum."""
    if any(k.upper() == "CLAUDECODE" for k in env):
        raise ErroConfiguracao("runner iniciado dentro de uma sessão do Claude Code; rode-o num terminal comum")


def ambiente_agente(base, harness):
    """Ambiente do agente: sem FastFile no PATH, sem o opt-in experimental e sem variáveis de
    runtime de uma sessão do Claude Code que esteja rodando o runner (sessão, socket, tokens)."""
    env = {k: v for k, v in base.items()
           if k.upper() != "FASTFILE_EXPERIMENTAL" and not _de_sessao_hospedeira(k)}
    chave = next((k for k in env if k.upper() == "PATH"), "PATH")
    env[chave] = path_sem_fastfile(env.get(chave, ""), harness)
    return env


def servidor_mcp(braco, ff_comando):
    """Entrada `mcpServers` do braço, ou None no braço A."""
    if braco == "A":
        return None
    env = {"FASTFILE_EXPERIMENTAL": FF_EXPERIMENTAL} if braco == "C" else {}
    return {"type": "stdio", "command": ff_comando[0], "args": [*ff_comando[1:], "mcp"], "env": env}


def catalogo_mcp(servidor, base_env, timeout=20):
    """Nomes de `tools/list` do servidor do braço, numa sessão MCP real sobre stdio."""
    env = {k: v for k, v in base_env.items() if k.upper() != "FASTFILE_EXPERIMENTAL"}
    env.update(servidor["env"])
    pedidos = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
    ]
    entrada = "".join(json.dumps(p) + "\n" for p in pedidos)
    with tempfile.TemporaryDirectory(prefix="ff-catalogo-") as vazio:
        proc = subprocess.run([servidor["command"], *servidor["args"]], input=entrada, cwd=vazio, env=env,
                              capture_output=True, text=True, encoding="utf-8", timeout=timeout)
    for linha in proc.stdout.splitlines():
        try:
            msg = json.loads(linha)
        except ValueError:
            continue
        if msg.get("id") == 2:
            return sorted(t["name"] for t in msg["result"]["tools"])
    raise ErroConfiguracao("servidor MCP não respondeu tools/list")


def validar_catalogo(braco, nomes):
    """B expõe só o FastFile atual; C acrescenta exatamente as experimentais."""
    nomes = set(nomes)
    if braco == "B":
        if not nomes or nomes & EXPERIMENTAIS:
            raise ErroConfiguracao(f"braço B expõe capacidade candidata: {sorted(nomes & EXPERIMENTAIS)}")
    elif braco == "C" and not EXPERIMENTAIS <= nomes:
        raise ErroConfiguracao(f"braço C sem as experimentais: {sorted(EXPERIMENTAIS - nomes)}")


def sha256_arquivo(caminho):
    h = hashlib.sha256()
    with open(caminho, "rb") as f:
        for bloco in iter(lambda: f.read(1 << 20), b""):
            h.update(bloco)
    return h.hexdigest()


# ------------------------------------------------------------------ patch


def _arquivos(raiz, ignorar):
    out = {}
    for p in sorted(Path(raiz).rglob("*")):
        rel = p.relative_to(raiz)
        if rel.parts[0] in ignorar or not p.is_file():
            continue
        out[rel.as_posix()] = p.read_bytes()
    return out


def _linhas(dados):
    linhas = dados.decode("utf-8").splitlines(True)
    if linhas and not linhas[-1].endswith("\n"):
        linhas[-1] += "\n\\ No newline at end of file\n"
    return linhas


def extrair_patch(original, trabalho, ignorar=()):
    """Diff Git (-p1) do checkout do agente contra uma cópia intocada da mesma revisão."""
    ignorar = GERADOS | set(ignorar)
    antes, depois = _arquivos(original, ignorar), _arquivos(trabalho, ignorar)
    partes = []
    for rel in sorted(antes.keys() | depois.keys()):
        a, b = antes.get(rel), depois.get(rel)
        if a == b:
            continue
        cab = f"diff --git a/{rel} b/{rel}\n"
        if a is None:
            cab += "new file mode 100644\n"
        elif b is None:
            cab += "deleted file mode 100644\n"
        try:
            la, lb = _linhas(a or b""), _linhas(b or b"")
        except UnicodeDecodeError:
            partes.append(cab + f"Binary files a/{rel} and b/{rel} differ\n")
            continue
        corpo = list(difflib.unified_diff(la, lb, "/dev/null" if a is None else f"a/{rel}",
                                          "/dev/null" if b is None else f"b/{rel}"))
        partes.append(cab + "".join(linha if linha.endswith("\n") else linha + "\n" for linha in corpo))
    return "".join(partes)


# ------------------------------------------------------------------ harness


@dataclass
class Pedido:
    trabalho: Path
    prompt: str
    env: dict
    mcp: dict | None
    timeout_s: float
    orcamento_usd: float | None
    modelo: str | None
    tarefa: dict = field(repr=False, default=None)
    braco: str = ""


@dataclass
class Execucao:
    status: str  # concluida | timeout | erro_harness | orcamento_excedido
    sessao: str | None = None
    ferramentas: list = field(default_factory=list)
    entradas_ferramentas: list = field(default_factory=list)
    uso: dict | None = None
    custo_usd: float | None = None
    modelo: str | None = None
    catalogo_sessao: list | None = None
    mcp_sessao: list | None = None
    turnos: int | None = None
    recursos: dict = field(default_factory=dict)
    # stderr do harness, só para o operador: nunca vai para a linha da tentativa.
    diagnostico: str | None = None
    resultado_subtipo: str | None = None


def ler_stream_json(linhas):
    """Eventos `--output-format stream-json` do Claude Code -> Execucao (sem status final)."""
    ex = Execucao(status="erro_harness")
    resultado = None
    for linha in linhas:
        try:
            ev = json.loads(linha)
        except ValueError:
            continue
        tipo = ev.get("type")
        if tipo == "system" and ev.get("subtype") == "init":
            ex.sessao = ev.get("session_id")
            ex.modelo = ev.get("model")
            ex.catalogo_sessao = sorted(ev.get("tools") or [])
            ex.mcp_sessao = sorted(s.get("name", "") for s in ev.get("mcp_servers") or [])
        elif tipo == "assistant":
            for bloco in (ev.get("message") or {}).get("content") or []:
                if isinstance(bloco, dict) and bloco.get("type") == "tool_use":
                    ex.ferramentas.append(bloco.get("name", "?"))
                    ex.entradas_ferramentas.append(json.dumps(bloco.get("input"), ensure_ascii=False))
        elif tipo == "result":
            resultado = ev
    if resultado is not None:
        ex.sessao = ex.sessao or resultado.get("session_id")
        uso = resultado.get("usage")
        if isinstance(uso, dict):
            ex.uso = {k: uso.get(k) for k in ("input_tokens", "output_tokens",
                                               "cache_creation_input_tokens", "cache_read_input_tokens")}
        custo = resultado.get("total_cost_usd")
        ex.custo_usd = float(custo) if isinstance(custo, (int, float)) else None
        ex.turnos = resultado.get("num_turns")
        sub = resultado.get("subtype", "")
        ex.resultado_subtipo = sub or None
        ex.status = ("orcamento_excedido" if "budget" in sub
                     else "erro_harness" if resultado.get("is_error") or sub not in ("success", "")
                     else "concluida")
        if ex.status != "concluida":
            # Mensagem de erro do harness (ex.: autenticação): só para o operador.
            erro = resultado.get("result") or resultado.get("errors") or ""
            ex.diagnostico = (erro if isinstance(erro, str) else json.dumps(erro, ensure_ascii=False))[:500]
    return ex


class Monitor:
    """Amostra CPU e RSS da árvore de processos do harness (psutil, quando disponível)."""

    def __init__(self, pid):
        self.cpu_s, self.rss_mb, self._parar = None, None, threading.Event()
        try:
            import psutil
        except ImportError:
            self._t = None
            return
        self._ps, self._raiz = psutil, psutil.Process(pid)
        self._cpu, self._vistos = {}, {}
        self._t = threading.Thread(target=self._amostrar, daemon=True)
        self._t.start()

    def _amostrar(self):
        while not self._parar.is_set():
            try:
                procs = [self._raiz, *self._raiz.children(recursive=True)]
            except self._ps.Error:
                break
            rss = 0
            for p in procs:
                self._vistos.setdefault(p.pid, p)
                try:
                    t = p.cpu_times()
                    self._cpu[p.pid] = t.user + t.system
                    rss += p.memory_info().rss
                except self._ps.Error:
                    pass
            self.rss_mb = max(self.rss_mb or 0, rss / 2**20)
            self._parar.wait(0.25)
        self.cpu_s = sum(self._cpu.values()) if self._cpu else None

    def parar(self):
        self._parar.set()
        if self._t:
            self._t.join()
        return {"cpu_s": self.cpu_s, "rss_max_mb": self.rss_mb,
                "fonte": "psutil_amostragem" if self._t else "indisponivel"}

    def matar_arvore(self):
        """Mata a árvore atual e também descendentes já vistos que ficaram órfãos (no Windows
        um órfão com o checkout como diretório atual impede apagá-lo e segura o pipe de saída).
        `is_running` confere o horário de criação, então um PID reutilizado não é morto."""
        if not self._t:
            return 0
        alvos = dict(self._vistos)
        try:
            alvos.update({p.pid: p for p in self._raiz.children(recursive=True)})
        except self._ps.Error:
            pass
        alvos.pop(self._raiz.pid, None)
        mortos = 0
        for p in alvos.values():
            try:
                if p.is_running():
                    p.kill()
                    mortos += 1
            except self._ps.Error:
                pass
        self._ps.wait_procs(list(alvos.values()), timeout=5)
        return mortos


class ClaudeCode:
    """Adaptador real: `claude -p` em sessão limpa (sem persistência, só MCP do braço)."""

    nome = "claude-code"
    medido = True
    artefatos = frozenset({".claude"})

    def __init__(self, executavel="claude", permissao="acceptEdits", ferramentas=None):
        self.executavel = executavel
        self.permissao = permissao
        self.ferramentas = ferramentas or ["Read", "Edit", "Write", "Glob", "Grep", "Bash(cargo:*)"]

    def versao(self):
        try:
            out = subprocess.run([self.executavel, "--version"], capture_output=True, text=True, timeout=30)
            return out.stdout.strip() or None
        except (OSError, subprocess.TimeoutExpired):
            return None

    def argv(self, pedido, mcp_arquivo):
        argv = [self.executavel, "-p", pedido.prompt, "--output-format", "stream-json", "--verbose",
                "--no-session-persistence", "--setting-sources", "project", "--strict-mcp-config",
                "--permission-mode", self.permissao, "--allowedTools", *self.ferramentas]
        nomes_ff = f"{PREFIXO_MCP_FF}*" if pedido.mcp else None
        if nomes_ff:
            argv[argv.index("--allowedTools") + 1:argv.index("--allowedTools") + 1] = [nomes_ff]
        argv += ["--mcp-config", str(mcp_arquivo)]
        if pedido.orcamento_usd is not None:
            argv += ["--max-budget-usd", f"{pedido.orcamento_usd:.4f}"]
        if pedido.modelo:
            argv += ["--model", pedido.modelo]
        return argv

    def executar(self, pedido):
        with tempfile.TemporaryDirectory(prefix="ff-mcp-", ignore_cleanup_errors=True) as d:
            mcp_arquivo = Path(d) / "mcp.json"
            servidores = {"fastfile": pedido.mcp} if pedido.mcp else {}
            mcp_arquivo.write_text(json.dumps({"mcpServers": servidores}), encoding="utf-8")
            return executar_stream(self.argv(pedido, mcp_arquivo), pedido)


def executar_stream(argv, pedido):
    exe = shutil.which(argv[0], path=pedido.env.get("PATH") or pedido.env.get("Path")) or argv[0]
    linhas, fim = [], threading.Event()
    # stderr vai para arquivo (sem risco de travar o pipe) e só aparece no diagnóstico do operador.
    erros = tempfile.TemporaryFile()
    try:
        proc = subprocess.Popen([exe, *argv[1:]], cwd=pedido.trabalho, env=pedido.env, stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=erros)
    except OSError as e:
        erros.close()
        return Execucao(status="erro_harness", diagnostico=type(e).__name__)
    monitor = Monitor(proc.pid)

    def ler():
        for bruto in proc.stdout:
            linhas.append(bruto.decode("utf-8", "replace"))
        fim.set()

    leitor = threading.Thread(target=ler, daemon=True)
    leitor.start()
    try:
        proc.wait(pedido.timeout_s)
        estourou = False
    except subprocess.TimeoutExpired:
        estourou = True
    # O harness saiu (ou estourou): nenhum descendente pode sobreviver à tentativa.
    if estourou:
        orfaos = monitor.matar_arvore()
        proc.kill()
        proc.wait()
    else:
        fim.wait(2)
        orfaos = monitor.matar_arvore()
    leitor.join(10)
    proc.stdout.close()
    erros.seek(0)
    diagnostico = erros.read()[-2048:].decode("utf-8", "replace").strip()
    erros.close()
    ex = ler_stream_json(linhas)
    ex.diagnostico = " | ".join(x for x in (ex.diagnostico, diagnostico) if x) or None
    ex.recursos = {**monitor.parar(), "processos_encerrados": orfaos}
    if estourou:
        ex.status = "timeout"
    elif proc.returncode and ex.status == "concluida":
        ex.status = "erro_harness"
    return ex


class Simulado:
    """Adaptador sem modelo para o smoke offline. `comportamento(pedido)` devolve uma ação:
    resolver | nada | timeout | sem_uso | ler_gabarito | sessao_fixa | erro."""

    nome = "simulado"
    medido = False
    artefatos = frozenset()

    def __init__(self, comportamento=None, custo_usd=0.01):
        self.comportamento = comportamento or (lambda pedido: "resolver")
        self.custo_usd = custo_usd

    def versao(self):
        return f"simulado/{RUNNER_VERSION}"

    def executar(self, pedido):
        acao = self.comportamento(pedido)
        usa_ff = pedido.mcp is not None
        ex = Execucao(status="concluida", sessao=f"sim-{uuid.uuid4().hex}", modelo="simulado",
                      uso={"input_tokens": 1000, "output_tokens": 200,
                           "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0},
                      custo_usd=self.custo_usd, turnos=3,
                      recursos={"cpu_s": None, "rss_max_mb": None, "fonte": "indisponivel"})
        ex.catalogo_sessao = ["Edit", "Read"] + ([f"{PREFIXO_MCP_FF}find_symbol"] if usa_ff else [])
        ex.mcp_sessao = ["fastfile"] if usa_ff else []
        ex.ferramentas = ["Read"] + ([f"{PREFIXO_MCP_FF}find_symbol"] if usa_ff else [])
        ex.entradas_ferramentas = ['{"file_path": "src/lib.rs"}']
        if acao in ("resolver", "sessao_fixa", "ler_gabarito", "sem_uso"):
            referencia = Path(tempfile.mkdtemp(prefix="ff-sim-ref-"))
            try:
                corpus.materializar({**pedido.tarefa, "mutacoes": []}, referencia / "r")
                for rel in pedido.tarefa["escopo_esperado"]:
                    shutil.copyfile(referencia / "r" / rel, pedido.trabalho / rel)
            finally:
                shutil.rmtree(referencia, ignore_errors=True)
            ex.ferramentas.append("Edit")
        if acao == "sessao_fixa":
            ex.sessao = "sim-sessao-fixa"
        elif acao == "ler_gabarito":
            ex.entradas_ferramentas.append(json.dumps({"file_path": str(corpus.GABARITOS)}))
        elif acao == "sem_uso":
            ex.uso, ex.custo_usd = None, None
        elif acao == "timeout":
            ex.status = "timeout"
        elif acao == "erro":
            ex.status, ex.uso, ex.custo_usd = "erro_harness", None, None
        return ex


# ------------------------------------------------------------------ avaliação


def avaliador_docker(imagem, timeout_suite=60):
    def avaliar(tarefa, patch_arquivo):
        proc = subprocess.run([sys.executable, str(GRADE_PATCH), tarefa["id"], str(patch_arquivo),
                               "--image", imagem, "--timeout", str(timeout_suite)],
                              capture_output=True, text=True, encoding="utf-8", timeout=timeout_suite * 4 + 120)
        try:
            veredito = json.loads(proc.stdout)
        except ValueError:
            return {"exit_code": 3, "reason": "grader_output_invalid"}
        return {k: veredito.get(k) for k in ("exit_code", "reason", "phase", "patch_sha256", "runs",
                                             "changed_paths", "sandbox_image")}
    return avaliar


def sem_avaliacao(tarefa, patch_arquivo):
    return {"exit_code": None, "reason": "nao_avaliado"}


# ------------------------------------------------------------------ execução


@dataclass
class Config:
    adaptador: object
    avaliar: object
    saida: Path
    raiz_trabalho: Path
    ff_comando: list | None = None
    ff_binario: Path | None = None
    modelo: str | None = None
    timeout_s: float = 900
    orcamento_tentativa_usd: float | None = None
    orcamento_total_usd: float | None = None
    retentativas: int = 0
    semente: int = 0
    harness_executavel: str | None = None
    env_base: dict = field(default_factory=lambda: dict(os.environ))


def _contaminacoes(ex, sessoes, repo):
    achados = []
    if ex.sessao and ex.sessao in sessoes:
        achados.append("sessao_repetida")
    texto = "\n".join(ex.entradas_ferramentas)
    if any(m in texto for m in MARCAS_PRIVADAS) or str(repo) in texto:
        achados.append("material_privado_acessado")
    return achados


def _tamanho_mb(raiz):
    return sum(p.stat().st_size for p in Path(raiz).rglob("*") if p.is_file()) / 2**20


def _validar_raiz_trabalho(raiz):
    raiz = Path(raiz).resolve()
    repo = corpus.RAIZ.parents[1].resolve()
    if raiz == repo or repo in raiz.parents:
        raise ErroConfiguracao("a área de trabalho precisa ficar fora do repositório")
    raiz.mkdir(parents=True, exist_ok=True)
    return raiz


def preparar_bracos(cfg, bracos):
    """Valida PATH e catálogo de cada braço antes de gastar qualquer tentativa."""
    info = {}
    env = ambiente_agente(cfg.env_base, cfg.harness_executavel)
    for braco in bracos:
        servidor = servidor_mcp(braco, cfg.ff_comando) if braco != "A" else None
        if braco != "A":
            if not cfg.ff_comando:
                raise ErroConfiguracao(f"braço {braco} exige --ff-bin")
            nomes = catalogo_mcp(servidor, env)
            validar_catalogo(braco, nomes)
        else:
            nomes = []
        info[braco] = {"servidor": servidor, "catalogo_ff": nomes}
    return env, info


def executar(cfg, tarefas, plano, run_id=None):
    """Roda o plano; devolve as linhas gravadas. Para cedo, com linha `parada`, quando o
    orçamento não comporta outra tentativa ou o custo deixou de ser verificável."""
    run_id = run_id or uuid.uuid4().hex[:12]
    raiz = _validar_raiz_trabalho(cfg.raiz_trabalho)
    repo = corpus.RAIZ.parents[1].resolve()
    por_id = {t["id"]: t for t in tarefas}
    bracos = sorted({p["braco"] for p in plano})
    env, info = preparar_bracos(cfg, bracos)
    comum = {
        "schema": SCHEMA, "run_id": run_id, "runner_version": RUNNER_VERSION,
        "harness": {"nome": cfg.adaptador.nome, "versao": cfg.adaptador.versao()},
        "modelo_pedido": cfg.modelo, "prompt_versao": PROMPT_VERSAO,
        "prompt_sha256": hashlib.sha256(PROMPT_MODELO.encode()).hexdigest(),
        "ff_sha256": sha256_arquivo(cfg.ff_binario) if cfg.ff_binario else None,
        "timeout_s": cfg.timeout_s, "orcamento_tentativa_usd": cfg.orcamento_tentativa_usd,
        "semente": cfg.semente,
    }
    if cfg.adaptador.medido:
        exigir_fora_de_sessao(cfg.env_base)
    if cfg.adaptador.medido and cfg.orcamento_total_usd is None:
        raise ErroConfiguracao("harness real exige --max-usd (teto de gasto autorizado)")
    gasto, sessoes, linhas = 0.0, set(), []
    cfg.saida.parent.mkdir(parents=True, exist_ok=True)
    with cfg.saida.open("a", encoding="utf-8", newline="\n") as out:  # LF: mesmos bytes em todo SO
        def gravar(linha):
            out.write(json.dumps(linha, ensure_ascii=False, sort_keys=True) + "\n")
            out.flush()
            linhas.append(linha)

        for item in plano:
            tarefa = por_id[item["tarefa"]]
            for tentativa in range(cfg.retentativas + 1):
                if cfg.orcamento_total_usd is not None:
                    reserva = cfg.orcamento_tentativa_usd or 0.0
                    if gasto + reserva > cfg.orcamento_total_usd or gasto >= cfg.orcamento_total_usd:
                        gravar({**comum, "tipo": "parada", "motivo": "orcamento_esgotado",
                                "gasto_usd": round(gasto, 6), "pendentes": len(plano) - item["ordem"]})
                        return linhas
                linha = _tentativa(cfg, tarefa, item, tentativa, env, info, repo, raiz, sessoes)
                linha.update(comum)
                gravar(linha)
                if linha["custo_usd"] is not None:
                    gasto += linha["custo_usd"]
                elif cfg.adaptador.medido:
                    gravar({**comum, "tipo": "parada", "motivo": "custo_inverificavel",
                            "gasto_usd": round(gasto, 6), "pendentes": len(plano) - item["ordem"] - 1})
                    return linhas
                if linha["status"] != "erro_harness":
                    break
    return linhas


def _tentativa(cfg, tarefa, item, tentativa, env, info, repo, raiz, sessoes):
    inicio = time.monotonic()
    iniciado = datetime.now(timezone.utc).isoformat(timespec="seconds")
    attempt_id = f"{item['tarefa']}-{item['braco']}-r{item['repeticao']}-t{tentativa}-{uuid.uuid4().hex[:6]}"
    area = Path(tempfile.mkdtemp(prefix="ff-tentativa-", dir=raiz))
    try:
        original, trabalho = area / "original", area / "checkout"
        digest = corpus.materializar(tarefa, original)
        if corpus.materializar(tarefa, trabalho) != tarefa["estado_inicial_digest"] or digest != tarefa["estado_inicial_digest"]:
            raise ErroConfiguracao(f"{tarefa['id']}: checkout não confere com a revisão congelada")
        visao = corpus.visao_agente(tarefa)
        prompt = PROMPT_MODELO.format(prompt=visao["prompt"], comando=" ".join(visao["comando_teste"]))
        servidor = info[item["braco"]]["servidor"]
        pedido = Pedido(trabalho=trabalho, prompt=prompt, env=env, mcp=servidor, timeout_s=cfg.timeout_s,
                        orcamento_usd=cfg.orcamento_tentativa_usd, modelo=cfg.modelo, tarefa=tarefa,
                        braco=item["braco"])
        setup = time.monotonic() - inicio
        t0 = time.monotonic()
        ex = cfg.adaptador.executar(pedido)
        agente = time.monotonic() - t0
        contaminacao = _contaminacoes(ex, sessoes, repo)
        if ex.sessao:
            sessoes.add(ex.sessao)
        if item["braco"] == "A" and any(f.startswith(PREFIXO_MCP_FF) for f in ex.catalogo_sessao or []):
            contaminacao.append("fastfile_no_braco_a")
        if item["braco"] != "A" and ex.mcp_sessao is not None and "fastfile" not in ex.mcp_sessao:
            contaminacao.append("fastfile_ausente_no_catalogo")
        t0 = time.monotonic()
        patch = extrair_patch(original, trabalho, cfg.adaptador.artefatos)
        patch_arquivo = area / "candidato.diff"
        patch_arquivo.write_bytes(patch.encode("utf-8"))
        veredito = (cfg.avaliar(tarefa, patch_arquivo) if ex.status == "concluida" and not contaminacao
                    else {"exit_code": None, "reason": "nao_avaliado"})
        avaliacao = time.monotonic() - t0
        disco = _tamanho_mb(area)
    finally:
        shutil.rmtree(area, ignore_errors=True)
    ff = [f for f in ex.ferramentas if f.startswith(PREFIXO_MCP_FF)]
    contagem = {}
    for f in ex.ferramentas:
        contagem[f] = contagem.get(f, 0) + 1
    return {
        "tipo": "tentativa", "attempt_id": attempt_id, "tarefa": tarefa["id"], "familia": tarefa["familia"],
        "braco": item["braco"], "repeticao": item["repeticao"], "ordem": item["ordem"], "tentativa": tentativa,
        "iniciado_em": iniciado, "cache_inicial": "frio_checkout_novo",
        "status": ex.status, "resultado_subtipo": ex.resultado_subtipo,
        "sessao": ex.sessao, "modelo": ex.modelo, "turnos": ex.turnos,
        "uso": ex.uso, "uso_status": "observado" if ex.uso else "indisponivel",
        "custo_usd": ex.custo_usd, "custo_fonte": "relatado_pelo_harness" if ex.custo_usd is not None else "indisponivel",
        "ferramentas": {"total": len(ex.ferramentas), "por_nome": contagem, "fastfile": len(ff)},
        "adotou_fastfile": bool(ff) if item["braco"] != "A" else None,
        "catalogo_ff": info[item["braco"]]["catalogo_ff"],
        "contaminacao": contaminacao, "valida": not contaminacao,
        "patch_sha256": hashlib.sha256(patch.encode("utf-8")).hexdigest(), "patch_vazio": not patch,
        "veredito": veredito,
        "sucesso": veredito.get("exit_code") == 0 and not contaminacao,
        "duracao_s": {"setup": round(setup, 3), "agente": round(agente, 3), "avaliacao": round(avaliacao, 3),
                      "total": round(time.monotonic() - inicio, 3)},
        "recursos": {**ex.recursos, "disco_mb": round(disco, 3)},
    }


# ------------------------------------------------------------------ CLI


def sondar(adaptador, env, timeout_s, orcamento_usd, modelo):
    """Prova de captura: uma sessão mínima no braço A deve exportar init, sessão, uso e custo."""
    d = Path(tempfile.mkdtemp(prefix="ff-sonda-"))
    try:
        pedido = Pedido(trabalho=d, prompt="Responda apenas: OK", env=env, mcp=None,
                        timeout_s=timeout_s, orcamento_usd=orcamento_usd, modelo=modelo)
        ex = adaptador.executar(pedido)
    finally:
        # A limpeza nunca derruba o relatório: o resultado da chamada paga sempre é impresso.
        shutil.rmtree(d, ignore_errors=True)
    faltando = [nome for nome, ok in (("status_concluida", ex.status == "concluida"), ("sessao", ex.sessao),
                                      ("catalogo_init", ex.catalogo_sessao is not None), ("uso", ex.uso),
                                      ("custo", ex.custo_usd is not None)) if not ok]
    if ex.mcp_sessao:
        faltando.append("sessao_nao_limpa_mcp")
    if d.exists():
        faltando.append("area_presa_por_processo")
    return {"schema": "fastfile-probe/1", "harness": {"nome": adaptador.nome, "versao": adaptador.versao()},
            "status": ex.status, "resultado_subtipo": ex.resultado_subtipo,
            "modelo": ex.modelo, "uso": ex.uso, "custo_usd": ex.custo_usd,
            "mcp_sessao": ex.mcp_sessao, "ferramentas_sessao": len(ex.catalogo_sessao or []),
            "processos_encerrados": ex.recursos.get("processos_encerrados"),
            "aprovado": not faltando, "faltando": faltando,
            # Só no relatório ao operador; pode citar caminhos locais, então não vai para os JSONL.
            "diagnostico": None if not faltando else (ex.diagnostico or "")[:500]}


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    pl = sub.add_parser("planejar", help="mostra a ordem balanceada das tentativas")
    ex = sub.add_parser("executar", help="roda os braços e grava uma linha por tentativa")
    so = sub.add_parser("sondar", help="prova que o harness exporta eventos e uso (gasta 1 chamada)")
    for s in (pl, ex):
        s.add_argument("--tarefas", default="", help="IDs separados por vírgula (padrão: piloto)")
        s.add_argument("--incluir-holdout", action="store_true")
        s.add_argument("--repeticoes", type=int, default=1)
        s.add_argument("--bracos", default="A,B,C")
        s.add_argument("--semente", type=int, default=0)
    for s in (ex, so):
        s.add_argument("--adaptador", choices=("simulado", "claude-code"), required=True)
        s.add_argument("--modelo")
        s.add_argument("--timeout", type=float, default=900)
        s.add_argument("--max-usd-tentativa", type=float)
        s.add_argument("--claude", default="claude", help="executável do Claude Code")
    ex.add_argument("--saida", type=Path, required=True)
    ex.add_argument("--area", type=Path, required=True, help="raiz descartável fora do repositório")
    ex.add_argument("--ff-bin", type=Path, help="binário fastfile dos braços B/C")
    ex.add_argument("--max-usd", type=float, help="teto de gasto do lote (obrigatório no harness real)")
    ex.add_argument("--imagem", help="ID sha256 da imagem do grade_patch.py (sem ela: nao_avaliado)")
    ex.add_argument("--retentativas", type=int, default=0)
    a = p.parse_args(argv)

    try:
        if a.cmd == "sondar":
            if a.adaptador == "claude-code" and a.max_usd_tentativa is None:
                raise ErroConfiguracao("sondar exige --max-usd-tentativa")
            if a.adaptador == "claude-code":
                exigir_fora_de_sessao(os.environ)
            adaptador = ClaudeCode(a.claude) if a.adaptador == "claude-code" else Simulado()
            env = ambiente_agente(dict(os.environ), a.claude)
            print(json.dumps(sondar(adaptador, env, a.timeout, a.max_usd_tentativa, a.modelo), ensure_ascii=False))
            return 0
        tarefas = tarefas_selecionadas([t for t in a.tarefas.split(",") if t], a.incluir_holdout)
        bracos = tuple(b for b in a.bracos.split(",") if b)
        if not set(bracos) <= set(BRACOS):
            raise ErroConfiguracao(f"braços válidos: {','.join(BRACOS)}")
        plano = planejar(tarefas, a.repeticoes, bracos, a.semente)
        if a.cmd == "planejar":
            for item in plano:
                print(json.dumps(item))
            return 0
        adaptador = ClaudeCode(a.claude) if a.adaptador == "claude-code" else Simulado()
        cfg = Config(adaptador=adaptador, avaliar=avaliador_docker(a.imagem) if a.imagem else sem_avaliacao,
                     saida=a.saida, raiz_trabalho=a.area,
                     ff_comando=[str(a.ff_bin.resolve())] if a.ff_bin else None,
                     ff_binario=a.ff_bin, modelo=a.modelo, timeout_s=a.timeout,
                     orcamento_tentativa_usd=a.max_usd_tentativa, orcamento_total_usd=a.max_usd,
                     retentativas=a.retentativas, semente=a.semente,
                     harness_executavel=a.claude if a.adaptador == "claude-code" else None)
        linhas = executar(cfg, tarefas, plano)
    except ErroConfiguracao as e:
        print(json.dumps({"erro": "configuracao", "motivo": str(e)}), file=sys.stderr)
        return 2
    tentativas = [l for l in linhas if l["tipo"] == "tentativa"]
    print(json.dumps({"tentativas": len(tentativas), "sucessos": sum(l["sucesso"] for l in tentativas),
                      "parada": next((l["motivo"] for l in linhas if l["tipo"] == "parada"), None)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
