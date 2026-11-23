#!/usr/bin/env python3
"""Corpus congelado do benchmark de agentes (CPST B01, issue #85).

Deriva os gabaritos dos seis cenários de descoberta a partir do fixture congelado
(`fixtures/`), com relógio controlado, e define a correção de respostas e a divisão
piloto/holdout das tarefas de correção. Só biblioteca padrão; sem rede e sem dados pessoais.

Uso:
    python bench/agentes/corpus.py gerar      # regrava gabaritos.json
    python bench/agentes/corpus.py verificar  # falha se gabaritos.json divergir do fixture
"""

import datetime as dt
import hashlib
import json
import math
import re
import shutil
import sys
import tomllib
from collections import Counter
from pathlib import Path

RAIZ = Path(__file__).resolve().parent
FIXTURES = RAIZ / "fixtures"
WS = FIXTURES / "ws"
# Workspaces congelados das tarefas de correção; `ws` é o original de P01–P10 (#85) e
# `ledger` o repositório maior das tarefas L01–L24 (#120). O holdout é por workspace e lote:
# L13–L24 (lote `busca`) são defeitos em que localizar o arquivo é o trabalho.
WORKSPACES = {"ws": WS, "ledger": FIXTURES / "ledger"}
META = FIXTURES / "meta.json"
GABARITOS = RAIZ / "gabaritos.json"
CORRECOES = RAIZ / "avaliador" / "correcoes"

VERSAO = 1
GIB = 2**30
JANELA_RECENTE_S = 3600
FRACAO_HOLDOUT = 0.4
SAL_DIVISAO = "cpst-b01:"

# Regras de visibilidade (AGENTS.md §3.2): padrões de projeto e proteção de segredos.
DIRS_IGNORADOS = frozenset({"target", ".git", "node_modules"})
NOMES_SEGREDO = frozenset({".env"})
SUFIXOS_SEGREDO = (".jwt", ".har")

CAMPOS_PRIVADOS = ("mutacoes", "teste_discriminante", "escopo_esperado", "falhas_esperadas_antes")


def _meta():
    return json.loads(META.read_text(encoding="utf-8"))


def relogio_padrao():
    return dt.datetime.fromisoformat(_meta()["relogio"].replace("Z", "+00:00"))


def _sha(dados: bytes) -> str:
    return hashlib.sha256(dados).hexdigest()


def digest_dir(raiz: Path) -> str:
    """Hash estável de uma árvore (caminhos relativos com '/' e bytes dos arquivos)."""
    h = hashlib.sha256()
    # Ordem pelo caminho relativo como texto: `Path` compara sem caixa no Windows e com caixa
    # no Linux, e o digest precisa ser o mesmo nos dois.
    for p in sorted((x for x in raiz.rglob("*") if x.is_file()), key=lambda x: x.relative_to(raiz).as_posix()):
        h.update(p.relative_to(raiz).as_posix().encode())
        h.update(b"\0")
        h.update(_sha(p.read_bytes()).encode())
        h.update(b"\n")
    return h.hexdigest()


def digest_fixture() -> str:
    return _sha((digest_dir(WS) + _sha(META.read_bytes())).encode())


def _oculto(partes) -> bool:
    return any(p in DIRS_IGNORADOS for p in partes[:-1]) or partes[-1] in NOMES_SEGREDO or partes[
        -1
    ].endswith(SUFIXOS_SEGREDO)


def _ignorado(partes) -> bool:
    return any(p in DIRS_IGNORADOS for p in partes[:-1])


def entradas():
    """Todas as entradas do fixture: reais (bytes do disco) e virtuais (apenas metadados)."""
    meta = _meta()
    padrao = meta["mtime_padrao_segundos_atras"]
    sobre = meta["mtime_segundos_atras"]
    saida = []
    for p in sorted(x for x in WS.rglob("*") if x.is_file()):
        rel = p.relative_to(WS).as_posix()
        saida.append(
            {
                "path": rel,
                "bytes": p.stat().st_size,
                "segundos_atras": sobre.get(rel, padrao),
                "real": True,
            }
        )
    for v in meta["virtuais"]:
        saida.append(
            {
                "path": v["path"],
                "bytes": v["bytes"],
                "segundos_atras": v["mtime_segundos_atras"],
                "real": False,
            }
        )
    return sorted(saida, key=lambda e: e["path"])


def _visivel(e) -> bool:
    return not _oculto(e["path"].split("/"))


def _texto(rel: str):
    try:
        return (WS / rel).read_bytes().decode("utf-8")
    except UnicodeDecodeError:
        return None


def _cenario_01(es):
    por_pasta = Counter()
    for e in es:
        partes = e["path"].split("/")
        if len(partes) > 1:
            por_pasta[partes[0]] += e["bytes"]
    ordem = sorted(por_pasta.items(), key=lambda kv: (-kv[1], kv[0]))[:5]
    return {"pastas": [{"path": k, "bytes": v} for k, v in ordem]}


def _cenario_02(es):
    reais = [e for e in es if e["real"] and _visivel(e) and e["path"].endswith("Cargo.toml")]
    raiz = tomllib.loads((WS / "Cargo.toml").read_text(encoding="utf-8"))
    membros = set()
    for padrao in raiz["workspace"]["members"]:
        if padrao.endswith("/*"):
            base = padrao[:-2]
            for e in reais:
                partes = e["path"].split("/")
                if len(partes) == 3 and partes[0] == base:
                    membros.add(e["path"])
        else:
            membros.add(f"{padrao}/Cargo.toml")
    ws = ["Cargo.toml"] + sorted(membros)
    tam = {e["path"]: e["bytes"] for e in reais}
    item = lambda p: {"path": p, "bytes": tam[p]}  # noqa: E731
    return {
        "workspace": [item(p) for p in ws],
        "checkout": [item(e["path"]) for e in reais],
    }


def _cenario_03(es, simbolo="parse_header"):
    rx = re.compile(rf"^\s*(?:pub(?:\([a-z]+\))?\s+)?fn\s+{simbolo}\b")
    defs = []
    for e in es:
        if not (e["real"] and _visivel(e) and e["path"].endswith(".rs")):
            continue
        for n, linha in enumerate((_texto(e["path"]) or "").splitlines(), 1):
            if rx.match(linha):
                defs.append(
                    {
                        "arquivo": e["path"],
                        "linha": n,
                        "assinatura": linha.strip().rstrip("{").strip(),
                    }
                )
    return {"simbolo": simbolo, "definicoes": defs}


def _cenario_04(es, agora, relogio):
    # Os mtimes do fixture são "segundos antes do relógio congelado"; `agora` desloca a janela.
    deslocamento = (agora - relogio).total_seconds()
    ok = []
    for e in es:
        if not _visivel(e):
            continue
        idade = e["segundos_atras"] + deslocamento
        if 0 <= idade < JANELA_RECENTE_S:
            ok.append(e["path"])
    return {"janela_segundos": JANELA_RECENTE_S, "arquivos": sorted(ok)}


def _cenario_05(es):
    return {
        "limite_bytes": GIB,
        "arquivos": [
            {"path": e["path"], "bytes": e["bytes"]}
            for e in es
            if _visivel(e) and e["bytes"] > GIB
        ],
    }


def _cenario_06(es, padrao="MAX_LINE"):
    rx = re.compile(rf"\b{padrao}\b")
    ocs = []
    for e in es:
        if not (e["real"] and _visivel(e)):
            continue
        texto = _texto(e["path"])
        if texto is None:
            continue
        for n, linha in enumerate(texto.splitlines(), 1):
            if rx.search(linha):
                ocs.append({"arquivo": e["path"], "linha": n})
    return {"padrao": padrao, "ocorrencias": ocs}


def gerar(agora=None):
    """Gabaritos dos seis cenários. Determinístico: depende só do fixture e do relógio."""
    relogio = relogio_padrao()
    agora = agora or relogio
    es = entradas()
    return {
        "versao": VERSAO,
        "fixture_digest": digest_fixture(),
        "relogio": relogio.isoformat().replace("+00:00", "Z"),
        "regras": {
            "dirs_ignorados": sorted(DIRS_IGNORADOS),
            "segredos_nomes": sorted(NOMES_SEGREDO),
            "segredos_sufixos": list(SUFIXOS_SEGREDO),
            "janela_recente": "0 <= agora - mtime < 3600 s (mtime futuro não conta)",
            "maior_que": "bytes > 2**30 (estrito)",
            "pastas": "filhos diretos da raiz; soma recursiva inclui entradas ignoradas e virtuais",
        },
        "cenarios": {
            "01_diagnostico_disco": _cenario_01(es),
            "02_todos_config_toml": _cenario_02(es),
            "03_onde_definido_x": _cenario_03(es),
            "04_arquivos_alterados_1h": _cenario_04(es, agora, relogio),
            "05_arquivos_maior_1gb": _cenario_05(es),
            "06_grep_repo_grande": _cenario_06(es),
        },
    }


def serializar(g) -> str:
    return json.dumps(g, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


# ---------------------------------------------------------------- correção de respostas


def _p(caminho) -> str:
    c = str(caminho).replace("\\", "/")
    while c.startswith("./"):
        c = c[2:]
    return c


def _chave(cenario, item):
    if cenario in ("04_arquivos_alterados_1h",):
        return (_p(item["path"] if isinstance(item, dict) else item),)
    if cenario in ("01_diagnostico_disco", "02_todos_config_toml", "05_arquivos_maior_1gb"):
        return (_p(item["path"]), int(item["bytes"]))
    if cenario == "03_onde_definido_x":
        return (_p(item["arquivo"]), int(item["linha"]), " ".join(item["assinatura"].split()))
    if cenario == "06_grep_repo_grande":
        return (_p(item["arquivo"]), int(item["linha"]))
    raise KeyError(cenario)


_CAMPO = {
    "01_diagnostico_disco": "pastas",
    "02_todos_config_toml": "workspace",
    "03_onde_definido_x": "definicoes",
    "04_arquivos_alterados_1h": "arquivos",
    "05_arquivos_maior_1gb": "arquivos",
    "06_grep_repo_grande": "ocorrencias",
}
_CAMPO_RESPOSTA = {**_CAMPO, "02_todos_config_toml": "arquivos"}


def corrigir(cenario, resposta, gabarito=None):
    """Compara a resposta do agente com o gabarito. Arquivo faltante ou sobrando reprova."""
    g = (gabarito or gerar())["cenarios"][cenario][_CAMPO[cenario]]
    try:
        r = resposta[_CAMPO_RESPOSTA[cenario]]
        esperado = [_chave(cenario, i) for i in g]
        obtido = [_chave(cenario, i) for i in r]
    except (KeyError, TypeError, ValueError):
        return {"aprovado": False, "erro": "resposta fora do formato", "faltando": [], "sobrando": []}
    faltando = sorted((Counter(esperado) - Counter(obtido)).elements())
    sobrando = sorted((Counter(obtido) - Counter(esperado)).elements())
    ordem_ok = esperado == obtido if cenario == "01_diagnostico_disco" else True
    return {
        "aprovado": not faltando and not sobrando and ordem_ok,
        "faltando": faltando,
        "sobrando": sobrando,
        "ordem_ok": ordem_ok,
    }


# ---------------------------------------------------------------- tarefas de correção


def carregar_correcoes():
    return [json.loads(p.read_text(encoding="utf-8")) for p in sorted(CORRECOES.glob("[A-Z][0-9][0-9].json"))]


def workspace(tarefa) -> str:
    return tarefa.get("workspace", "ws")


def lockfile(tarefa) -> Path:
    """Cargo.lock do workspace da tarefa; fica fora do fixture, junto do avaliador."""
    ws = workspace(tarefa)
    return CORRECOES.with_suffix(".lock") if ws == "ws" else CORRECOES / f"{ws}.lock"


def lote(tarefa):
    """Lote de tarefas dentro do workspace; `None` para as originais (#120: `busca`)."""
    return tarefa.get("lote")


def divisao(tarefas):
    """Holdout por família dentro de cada workspace e lote: incluir um workspace ou um lote
    novo não muda a divisão dos que já existiam."""
    por_ws = {}
    for t in tarefas:
        por_ws.setdefault((workspace(t), lote(t) or ""), []).append(t["familia"])
    out = {"holdout": [], "piloto": []}
    for fams in por_ws.values():
        d = divisao_por_familia(fams)
        out["holdout"] += d["holdout"]
        out["piloto"] += d["piloto"]
    return {k: sorted(v) for k, v in out.items()}


def divisao_por_familia(familias):
    """Holdout por família: ordena por sha256(sal+família) e reserva as primeiras 40% (teto)."""
    fams = sorted(set(familias))
    ordem = sorted(fams, key=lambda f: _sha((SAL_DIVISAO + f).encode()))
    n = math.ceil(len(fams) * FRACAO_HOLDOUT)
    return {"holdout": sorted(ordem[:n]), "piloto": sorted(ordem[n:])}


def visao_agente(tarefa):
    """Único recorte da tarefa que o agente pode ver: sem mutações, testes ou escopo esperado."""
    return {
        "id": tarefa["id"],
        "prompt": tarefa["prompt"],
        "estado_inicial_digest": tarefa["estado_inicial_digest"],
        "comando_teste": tarefa["regressoes"][0],
    }


def materializar(tarefa, destino: Path) -> str:
    """Checkout limpo (sem .git, sem avaliador, sem `target/`) com o defeito aplicado."""
    destino = Path(destino)
    if destino.exists() and any(destino.iterdir()):
        raise FileExistsError(destino)
    origem = WORKSPACES[workspace(tarefa)]
    shutil.copytree(origem, destino, dirs_exist_ok=True, ignore=shutil.ignore_patterns("target", "Cargo.lock"))
    for m in tarefa["mutacoes"]:
        arq = destino / m["arquivo"]
        texto = arq.read_bytes().decode("utf-8")
        if texto.count(m["de"]) != 1:
            raise ValueError(f"{tarefa['id']}: trecho deve ocorrer 1 vez em {m['arquivo']}")
        arq.write_bytes(texto.replace(m["de"], m["para"]).encode("utf-8"))
    return digest_dir(destino)


def _main(argv):
    cmd = argv[1] if len(argv) > 1 else "verificar"
    novo = serializar(gerar())
    if cmd == "gerar":
        GABARITOS.write_text(novo, encoding="utf-8", newline="\n")
        print(f"gravado {GABARITOS.name}")
        return 0
    atual = GABARITOS.read_text(encoding="utf-8") if GABARITOS.exists() else ""
    if atual != novo:
        print("gabaritos.json diverge do fixture; rode `corpus.py gerar` e revise o diff")
        return 1
    print("ok")
    return 0


if __name__ == "__main__":
    sys.exit(_main(sys.argv))
