#!/usr/bin/env python3
"""Confirma, com `cargo` real e sem rede, que cada tarefa de correção é discriminante.

Para cada tarefa: (1) com o defeito, só os testes de `falhas_esperadas_antes` falham;
(2) com a correção (fixture base), nenhum teste da suíte falha (regressões preservadas).
Cada execução usa `target/` próprio (o cache compartilhado mascara a correção). Opt-in: exige `cargo`; não faz parte da CI padrão (ver `tests/test_corpus.py`).
"""

import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

import corpus

FALHA = re.compile(r"^test (\S+) \.\.\. FAILED$", re.M)


def _cargo(raiz: Path, alvo: Path, argv):
    env = {**os.environ, "CARGO_TARGET_DIR": str(alvo), "CARGO_NET_OFFLINE": "true"}
    p = subprocess.run(argv, cwd=raiz, env=env, capture_output=True, text=True)
    return p.returncode, sorted({n.split("::")[-1] for n in FALHA.findall(p.stdout)})


def _com_teste(raiz: Path, t):
    arq = raiz / t["teste_discriminante"]["arquivo"]
    arq.parent.mkdir(parents=True, exist_ok=True)
    arq.write_text(t["teste_discriminante"]["conteudo"], encoding="utf-8", newline="\n")


def verificar(t):
    erros = []
    with tempfile.TemporaryDirectory() as d:
        antes, depois = Path(d) / "antes", Path(d) / "depois"
        corpus.materializar(t, antes)
        _com_teste(antes, t)
        rc, falhas = _cargo(antes, antes / "_t", t["regressoes"][0])
        if rc == 0 or falhas != sorted(t["falhas_esperadas_antes"]):
            erros.append(f"antes: rc={rc} falhas={falhas}")
        corpus.materializar({**t, "mutacoes": []}, depois)
        _com_teste(depois, t)
        rc, falhas = _cargo(depois, depois / "_t", t["regressoes"][0])
        if rc != 0 or falhas:
            erros.append(f"depois: rc={rc} falhas={falhas}")
    return erros


def main():
    ruim = 0
    for t in corpus.carregar_correcoes():
        erros = verificar(t)
        print(t["id"], "ok" if not erros else erros)
        ruim += bool(erros)
    return 1 if ruim else 0


if __name__ == "__main__":
    sys.exit(main())
