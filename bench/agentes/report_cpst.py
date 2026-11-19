#!/usr/bin/env python3
"""Relatório de CPST (issue #88) a partir das linhas `fastfile-attempt/1` do runner.

CPST = custo de TODAS as tentativas válidas (falhas, timeouts e retentativas incluídas) ÷
conclusões corretas. Zero conclusões = CPST indefinido: o relatório mostra o gasto e 0/N.
Custo sem fonte confiável nunca é inventado: a alegação que dependeria dele fica bloqueada.

Uso:
    python report_cpst.py tentativas.jsonl [...] --premissas premissas.json \
        [--protocolo protocolo_promocao.json] [--json saida.json] [--md saida.md]
"""

import argparse
import json
import math
import random
import sys
from collections import Counter, defaultdict
from pathlib import Path

SCHEMA_TENTATIVA = "fastfile-attempt/1"
SCHEMA_RELATORIO = "fastfile-cpst-report/1"
BRACOS = ("A", "B", "C")
COMPARACOES = (("B", "A"), ("C", "B"), ("C", "A"))
# D (#120): catálogo do B com a busca nativa desligada. Mostra o efeito quando o agente usa o
# FastFile; nunca entra no gate nem no número principal.
BRACOS_DIAGNOSTICO = ("D",)
COMPARACOES_DIAGNOSTICO = (("D", "B"),)
CAMPOS_VOLUME = ("bytes_total", "bytes_fastfile", "bytes_ponderado", "respostas_grandes", "releituras",
                 "chamadas_repetidas", "arquivos_lidos")
CATEGORIAS_USO = ("input_tokens", "output_tokens", "cache_creation_input_tokens", "cache_read_input_tokens")
REAMOSTRAGENS = 2000
SEMENTE_BOOTSTRAP = 88


class ErroDados(Exception):
    """Linhas que não podem ser agregadas numa única série comparável."""


# ------------------------------------------------------------------ entrada


def carregar(caminhos):
    linhas = []
    for caminho in caminhos:
        with open(caminho, encoding="utf-8") as f:
            for n, texto in enumerate(f, 1):
                if not texto.strip():
                    continue
                try:
                    linha = json.loads(texto)
                except ValueError as e:
                    raise ErroDados(f"{Path(caminho).name}:{n}: JSON inválido") from e
                if linha.get("schema") != SCHEMA_TENTATIVA:
                    raise ErroDados(f"{Path(caminho).name}:{n}: esquema {linha.get('schema')!r} não suportado")
                linhas.append(linha)
    return linhas


def _serie(tentativas):
    """Uma série comparável: mesmo harness (versão), mesmo modelo, mesmo prompt."""
    chaves = {
        "harness": {f'{t["harness"]["nome"]} {t["harness"]["versao"]}' for t in tentativas},
        "modelo": {t.get("modelo") or t.get("modelo_pedido") for t in tentativas if t.get("modelo") or t.get("modelo_pedido")},
        "prompt": {t["prompt_sha256"] for t in tentativas},
    }
    misturadas = [k for k, v in chaves.items() if len(v) > 1]
    if misturadas:
        raise ErroDados(f"série mistura {', '.join(misturadas)}: separe os arquivos (não se compara entre modelos ou versões)")
    return {k: sorted(map(str, v))[0] if v else None for k, v in chaves.items()}


# ------------------------------------------------------------------ custo


def custo_tentativa(t, premissas):
    """(usd, fonte). Ordem: custo relatado pelo harness > uso × preço declarado > indisponível."""
    if t.get("custo_usd") is not None:
        base, fonte = float(t["custo_usd"]), "relatado"
    else:
        precos = (premissas.get("precos_usd_por_mtok") or {}).get(t.get("modelo") or "")
        uso = t.get("uso")
        if not precos or not uso or any(uso.get(c) is None for c in CATEGORIAS_USO if c in precos):
            return None, "indisponivel"
        base = sum((uso.get(c) or 0) * precos[c] / 1e6 for c in CATEGORIAS_USO if c in precos)
        fonte = "estimado_por_preco"
    local = 0.0
    cpu_hora = premissas.get("usd_por_cpu_hora")
    cpu = (t.get("recursos") or {}).get("cpu_s")
    if cpu_hora is not None and cpu is not None:
        local += cpu * cpu_hora / 3600
    amort = (premissas.get("amortizacao_setup") or {}).get(t["braco"])
    if amort:
        local += amort["usd"] / amort["tarefas"]
    return base + local, fonte


# ------------------------------------------------------------------ estatística


def quantil(valores, q):
    if not valores:
        return None
    v = sorted(valores)
    pos = (len(v) - 1) * q
    lo, hi = math.floor(pos), math.ceil(pos)
    return v[lo] + (v[hi] - v[lo]) * (pos - lo)


def _cpst(custo, sucessos):
    return custo / sucessos if sucessos else None


def agregar_braco(tentativas, premissas):
    validas = [t for t in tentativas if t.get("valida", True)]
    custos = [custo_tentativa(t, premissas) for t in validas]
    fontes = Counter(f for _, f in custos)
    custo_conhecido = all(c is not None for c, _ in custos)
    custo = sum(c for c, _ in custos if c is not None)
    sucessos = sum(1 for t in validas if t.get("sucesso"))
    uso = {c: sum((t.get("uso") or {}).get(c) or 0 for t in validas) for c in CATEGORIAS_USO}
    sem_uso = sum(1 for t in validas if not t.get("uso"))
    duracoes = [t["duracao_s"]["total"] for t in validas]
    agente = [t["duracao_s"]["agente"] for t in validas]
    cpu = [t["recursos"].get("cpu_s") for t in validas if (t.get("recursos") or {}).get("cpu_s") is not None]
    rss = [t["recursos"].get("rss_max_mb") for t in validas if (t.get("recursos") or {}).get("rss_max_mb") is not None]
    adocao = [t.get("adotou_fastfile") for t in validas if t.get("adotou_fastfile") is not None]
    primeira = [t for t in validas if t.get("repeticao") == 0]
    seguintes = [t for t in validas if t.get("repeticao", 0) > 0]
    return {
        "tentativas": len(tentativas),
        "validas": len(validas),
        "contaminadas": len(tentativas) - len(validas),
        "retentativas": sum(1 for t in validas if t.get("tentativa", 0) > 0),
        "status": dict(Counter(t["status"] for t in validas)),
        "sucessos": sucessos,
        "taxa_sucesso": sucessos / len(validas) if validas else None,
        "custo_usd": round(custo, 6) if custo_conhecido else None,
        "custo_usd_parcial": round(custo, 6),
        "custo_fontes": dict(fontes),
        "cpst_usd": _cpst(custo, sucessos) if custo_conhecido else None,
        "cpst_status": ("indisponivel" if not custo_conhecido else "indefinido_zero_sucessos" if not sucessos
                        else "estimado" if fontes.get("estimado_por_preco") else "relatado"),
        "uso_tokens": uso,
        "tentativas_sem_uso": sem_uso,
        "ferramentas_media": (sum(t["ferramentas"]["total"] for t in validas) / len(validas)) if validas else None,
        "adocao_fastfile": (sum(adocao) / len(adocao)) if adocao else None,
        "duracao_total_s": {"p50": quantil(duracoes, 0.5), "p95": quantil(duracoes, 0.95)},
        "duracao_agente_s": {"p50": quantil(agente, 0.5), "p95": quantil(agente, 0.95)},
        "cpu_s_media": (sum(cpu) / len(cpu)) if cpu else None,
        "rss_max_mb_p95": quantil(rss, 0.95),
        "primeira_repeticao": {"tentativas": len(primeira), "sucessos": sum(1 for t in primeira if t.get("sucesso"))},
        "repeticoes_seguintes": {"tentativas": len(seguintes), "sucessos": sum(1 for t in seguintes if t.get("sucesso"))},
        # Só com linhas do runner v2: relatórios antigos continuam se regenerando iguais.
        **({"volume": vol} if (vol := agregar_volume(validas)) else {}),
    }


def _media(valores):
    return sum(valores) / len(valores) if valores else None


def agregar_volume(validas):
    """Médias por tentativa das métricas de volume do runner v2 (None sem linhas com volume)."""
    com = [t["volume"] for t in validas if t.get("volume")]
    if not com:
        return None
    por_ferramenta = Counter()
    for v in com:
        por_ferramenta.update(v.get("bytes_por_ferramenta") or {})
    ponderado = [v["bytes_ponderado"] for v in com]
    total = sum(v["bytes_total"] for v in com)
    return {
        "tentativas_com_volume": len(com),
        **{f"{c}_media": _media([v.get(c, 0) for v in com]) for c in CAMPOS_VOLUME},
        "bytes_ponderado_p50": quantil(ponderado, 0.5),
        "bytes_ponderado_p95": quantil(ponderado, 0.95),
        "fracao_bytes_fastfile": (sum(v["bytes_fastfile"] for v in com) / total) if total else None,
        "bytes_por_ferramenta": dict(sorted(por_ferramenta.items(), key=lambda kv: (-kv[1], kv[0]))),
    }


def por_tarefa(tentativas_por_braco, premissas):
    """Tabela tarefa x braço com denominadores, volume, adoção, sucesso e retrabalho (#120)."""
    linhas = []
    for braco, ts in tentativas_por_braco.items():
        grupos = defaultdict(list)
        for t in ts:
            grupos[t["tarefa"]].append(t)
        for tarefa, g in sorted(grupos.items()):
            validas = [t for t in g if t.get("valida", True)]
            custos = [custo_tentativa(t, premissas)[0] for t in validas]
            vols = [t["volume"] for t in validas if t.get("volume")]
            adocao = [t["adotou_fastfile"] for t in validas if t.get("adotou_fastfile") is not None]
            dur = [t["duracao_s"]["total"] for t in validas]
            sucessos = sum(1 for t in validas if t.get("sucesso"))
            custo = None if any(c is None for c in custos) else sum(custos)
            linhas.append({
                "tarefa": tarefa, "braco": braco, "tentativas": len(g), "validas": len(validas),
                "sucessos": sucessos, "custo_usd": custo,
                "cpst_usd": _cpst(custo, sucessos) if custo is not None else None,
                "adocao_fastfile": _media(adocao),
                "bytes_media": _media([v["bytes_total"] for v in vols]),
                "bytes_ponderado_media": _media([v["bytes_ponderado"] for v in vols]),
                "releituras_media": _media([v["releituras"] for v in vols]),
                "retentativas": sum(1 for t in validas if t.get("tentativa", 0) > 0),
                "duracao_total_s": {"p50": quantil(dur, 0.5), "p95": quantil(dur, 0.95)},
            })
    return sorted(linhas, key=lambda x: (x["tarefa"], x["braco"]))


def _por_tarefa(tentativas, premissas):
    """tarefa -> [custo total ou None se algum faltar, tentativas válidas, sucessos]."""
    acc = defaultdict(lambda: [0.0, 0, 0])
    for t in tentativas:
        if not t.get("valida", True):
            continue
        c, _ = custo_tentativa(t, premissas)
        a = acc[t["tarefa"]]
        a[0] = None if c is None or a[0] is None else a[0] + c
        a[1] += 1
        a[2] += 1 if t.get("sucesso") else 0
    return dict(acc)


def comparar(novo, base, premissas):
    """Diferença pareada por tarefa, com IC 95% por bootstrap sobre tarefas (semente fixa)."""
    pn, pb = _por_tarefa(novo, premissas), _por_tarefa(base, premissas)
    tarefas = sorted(set(pn) & set(pb))
    out = {"tarefas_pareadas": len(tarefas)}
    if not tarefas:
        return {**out, "status": "sem_pares"}

    def taxa(p, ts):
        n = sum(p[t][1] for t in ts)
        return sum(p[t][2] for t in ts) / n if n else None

    def razao(ts):
        if any(pn[t][0] is None or pb[t][0] is None for t in ts):
            return None
        sn, sb = sum(pn[t][2] for t in ts), sum(pb[t][2] for t in ts)
        if not sn or not sb:
            return None
        return (sum(pn[t][0] for t in ts) / sn) / (sum(pb[t][0] for t in ts) / sb)

    dif = taxa(pn, tarefas) - taxa(pb, tarefas)
    rng = random.Random(SEMENTE_BOOTSTRAP)
    difs, razoes = [], []
    for _ in range(REAMOSTRAGENS):
        amostra = [rng.choice(tarefas) for _ in tarefas]
        difs.append(taxa(pn, amostra) - taxa(pb, amostra))
        r = razao(amostra)
        if r is not None:
            razoes.append(r)
    r = razao(tarefas)
    indef = REAMOSTRAGENS - len(razoes)
    extra = _comparar_volume(novo, base, tarefas)
    return {
        **out,
        **extra,
        "status": "ok",
        "dif_taxa_sucesso": dif,
        "dif_taxa_sucesso_ic95": [quantil(difs, 0.025), quantil(difs, 0.975)],
        "razao_cpst": r,
        # Reamostras sem sucesso num dos lados deixam a razão indefinida: IC só se forem raras.
        "razao_cpst_ic95": ([quantil(razoes, 0.025), quantil(razoes, 0.975)]
                            if r is not None and indef <= REAMOSTRAGENS * 0.05 else None),
        "reamostras_razao_indefinida": indef,
    }


def _ponderado_por_tarefa(tentativas):
    acc = defaultdict(list)
    for t in tentativas:
        if t.get("valida", True) and t.get("volume"):
            acc[t["tarefa"]].append(t["volume"]["bytes_ponderado"])
    return {k: sum(v) / len(v) for k, v in acc.items()}


def _comparar_volume(novo, base, tarefas):
    """Razão pareada do volume ponderado médio por tarefa, com IC 95% (mesma semente)."""
    vn, vb = _ponderado_por_tarefa(novo), _ponderado_por_tarefa(base)
    ts = [t for t in tarefas if t in vn and t in vb]
    if not ts:
        return {}

    def razao(amostra):
        den = sum(vb[t] for t in amostra)
        return sum(vn[t] for t in amostra) / den if den else None

    rng = random.Random(SEMENTE_BOOTSTRAP)
    rs = [x for x in (razao([rng.choice(ts) for _ in ts]) for _ in range(REAMOSTRAGENS)) if x is not None]
    return {"tarefas_com_volume": len(ts), "razao_bytes_ponderado": razao(ts),
            "razao_bytes_ponderado_ic95": [quantil(rs, 0.025), quantil(rs, 0.975)] if rs else None}


# ------------------------------------------------------------------ protocolo


def avaliar_gate(comp, braco_novo, braco_base, agregados, protocolo):
    """aprovado | reprovado | inconclusivo, com os motivos. Regras fixadas antes da rodada."""
    motivos = []
    novo, base = agregados[braco_novo], agregados[braco_base]
    if comp.get("status") != "ok":
        return {"decisao": "inconclusivo", "motivos": ["sem tarefas pareadas"]}
    if comp["tarefas_pareadas"] < protocolo["min_tarefas_pareadas"]:
        motivos.append(f"tarefas pareadas {comp['tarefas_pareadas']} < {protocolo['min_tarefas_pareadas']}")
    if novo["contaminadas"] or base["contaminadas"]:
        return {"decisao": "inconclusivo", "motivos": ["há tentativas contaminadas: investigue antes de concluir"]}
    if novo["cpst_status"] in ("indisponivel",) or base["cpst_status"] in ("indisponivel",):
        return {"decisao": "inconclusivo", "motivos": ["custo indisponível: alegação de CPST bloqueada"]}
    lo, hi = comp["dif_taxa_sucesso_ic95"]
    margem = -protocolo["margem_nao_inferioridade_sucesso_pp"] / 100
    if hi < margem:
        return {"decisao": "reprovado", "motivos": [f"sucesso pior que a margem (IC95 até {hi:+.1%})"]}
    if lo < margem:
        motivos.append(f"não inferioridade de sucesso não demonstrada (IC95 a partir de {lo:+.1%})")
    ic = comp.get("razao_cpst_ic95")
    alvo = 1 - protocolo["reducao_minima_cpst_pct"] / 100
    if ic is None:
        motivos.append("razão de CPST sem IC (sucessos insuficientes)")
    elif ic[0] > 1:
        return {"decisao": "reprovado", "motivos": [f"CPST maior que a base (IC95 {ic[0]:.2f}–{ic[1]:.2f})"]}
    elif ic[1] > alvo:
        motivos.append(f"redução de CPST não demonstrada (IC95 até {ic[1]:.2f}, alvo ≤ {alvo:.2f})")
    p95n, p95b = novo["duracao_total_s"]["p95"], base["duracao_total_s"]["p95"]
    teto = 1 + protocolo["piora_maxima_p95_pct"] / 100
    if p95n is not None and p95b and p95n > p95b * teto:
        return {"decisao": "reprovado", "motivos": [f"p95 de duração {p95n:.0f}s > {teto:.2f}× a base"]}
    return {"decisao": "inconclusivo" if motivos else "aprovado", "motivos": motivos}


# ------------------------------------------------------------------ relatório


def relatorio(linhas, premissas, protocolo=None):
    tentativas = [l for l in linhas if l.get("tipo") == "tentativa"]
    paradas = [l for l in linhas if l.get("tipo") == "parada"]
    if not tentativas:
        raise ErroDados("nenhuma tentativa nas linhas")
    serie = _serie(tentativas)
    por_braco = defaultdict(list)
    for t in tentativas:
        por_braco[t["braco"]].append(t)
    agregados = {b: agregar_braco(por_braco[b], premissas) for b in BRACOS + BRACOS_DIAGNOSTICO if por_braco[b]}
    comparacoes = {}
    for novo, base in COMPARACOES:
        if novo in agregados and base in agregados:
            comp = comparar(por_braco[novo], por_braco[base], premissas)
            if protocolo:
                comp["gate"] = avaliar_gate(comp, novo, base, agregados, protocolo)
            comparacoes[f"{novo}_vs_{base}"] = comp
    for novo, base in COMPARACOES_DIAGNOSTICO:
        if novo in agregados and base in agregados:
            comp = comparar(por_braco[novo], por_braco[base], premissas)
            comp["gate"] = {"decisao": "diagnostico", "motivos": ["uso obrigatório; fora do gate (#120)"]}
            comparacoes[f"{novo}_vs_{base}"] = comp
    com_volume = any(t.get("volume") for t in tentativas)
    return _estavel({
        "schema": SCHEMA_RELATORIO,
        "serie": serie,
        "runs": sorted({t["run_id"] for t in tentativas}),
        "premissas": premissas,
        "protocolo": protocolo,
        "paradas": [{"motivo": p["motivo"], "pendentes": p.get("pendentes")} for p in paradas],
        "incompleto": bool(paradas),
        "bracos": agregados,
        "comparacoes": comparacoes,
        **({"por_tarefa": por_tarefa({b: por_braco[b] for b in agregados}, premissas)} if com_volume else {}),
    })


def _estavel(v):
    """12 dígitos significativos: o sum() do Python 3.12+ difere no último ulp e o relatório
    versionado tem de se regenerar igual em qualquer versão."""
    if isinstance(v, float):
        return float(f"{v:.12g}")
    if isinstance(v, dict):
        return {k: _estavel(x) for k, x in v.items()}
    if isinstance(v, (list, tuple)):
        return [_estavel(x) for x in v]
    return v


def _fmt(v, pct=False, usd=False, nd=2):
    if v is None:
        return "—"
    if pct:
        return f"{v:.1%}"
    if usd:
        return f"US$ {v:.4f}"
    return f"{v:.{nd}f}" if isinstance(v, float) else str(v)


def _ic(ic):
    return "—" if not ic else f"{ic[0]:.2f}–{ic[1]:.2f}"


def _markdown_volume(r):
    out = []
    vols = {b: a["volume"] for b, a in r["bracos"].items() if a.get("volume")}
    if vols:
        out += ["", "| Braço | Bytes/tent. | Ponderado/tent. (p50 / p95) | % FastFile | Respostas > 8 KiB "
                "| Releituras | Repetidas |", "|---|---|---|---|---|---|---|"]
        for b, v in vols.items():
            p50, p95 = _fmt(v["bytes_ponderado_p50"], nd=0), _fmt(v["bytes_ponderado_p95"], nd=0)
            out.append(f"| {b} | {_fmt(v['bytes_total_media'], nd=0)} | {_fmt(v['bytes_ponderado_media'], nd=0)} "
                       f"({p50} / {p95}) | {_fmt(v['fracao_bytes_fastfile'], pct=True)} | "
                       f"{_fmt(v['respostas_grandes_media'], nd=1)} | {_fmt(v['releituras_media'], nd=1)} | "
                       f"{_fmt(v['chamadas_repetidas_media'], nd=1)} |")
    razoes = [(n, c) for n, c in r["comparacoes"].items() if c.get("razao_bytes_ponderado") is not None]
    if razoes:
        out += ["", "| Comparação | Razão do volume ponderado (IC95) |", "|---|---|"]
        for n, c in razoes:
            out.append(f"| {n} | {_fmt(c['razao_bytes_ponderado'])} ({_ic(c['razao_bytes_ponderado_ic95'])}) |")
    if r.get("por_tarefa"):
        out += ["", "| Tarefa | Braço | Sucesso / válidas | CPST | Adoção FF | Bytes | Ponderado | Releituras "
                "| Retentativas |", "|---|---|---|---|---|---|---|---|---|"]
        for x in r["por_tarefa"]:
            out.append(f"| {x['tarefa']} | {x['braco']} | {x['sucessos']} / {x['validas']} | "
                       f"{_fmt(x['cpst_usd'], usd=True)} | {_fmt(x['adocao_fastfile'], pct=True)} | "
                       f"{_fmt(x['bytes_media'], nd=0)} | {_fmt(x['bytes_ponderado_media'], nd=0)} | "
                       f"{_fmt(x['releituras_media'], nd=1)} | {x['retentativas']} |")
    return out


def markdown(r):
    """Tabela reconstruída só a partir do JSON do relatório (que vem das linhas sanitizadas)."""
    s = r["serie"]
    out = [f"# CPST — {s['harness']} · {s['modelo']}", "",
           f"Runs: {', '.join(r['runs'])}. Premissas: `{r['premissas'].get('id', 'sem id')}`."]
    if r["incompleto"]:
        out += ["", "**Lote incompleto:** " + "; ".join(f"{p['motivo']} ({p['pendentes']} pendentes)" for p in r["paradas"])]
    out += ["", "| Braço | Válidas / total | Sucesso | Custo | CPST | p50 / p95 (s) | Adoção FF | Ferramentas/tent. |",
            "|---|---|---|---|---|---|---|---|"]
    for b, a in r["bracos"].items():
        cpst = {"indefinido_zero_sucessos": f"indefinido (0/{a['validas']})",
                "indisponivel": "indisponível"}.get(a["cpst_status"], _fmt(a["cpst_usd"], usd=True))
        custo = _fmt(a["custo_usd"], usd=True) if a["custo_usd"] is not None else f"≥ {_fmt(a['custo_usd_parcial'], usd=True)} (incompleto)"
        out.append(f"| {b} | {a['validas']} / {a['tentativas']} | {a['sucessos']} ({_fmt(a['taxa_sucesso'], pct=True)}) | "
                   f"{custo} | {cpst} | {_fmt(a['duracao_total_s']['p50'], nd=1)} / {_fmt(a['duracao_total_s']['p95'], nd=1)} | "
                   f"{_fmt(a['adocao_fastfile'], pct=True)} | {_fmt(a['ferramentas_media'], nd=1)} |")
    if r["comparacoes"]:
        out += ["", "| Comparação | Tarefas | Δ sucesso (IC95) | Razão CPST (IC95) | Gate |", "|---|---|---|---|---|"]
        for nome, c in r["comparacoes"].items():
            if c["status"] != "ok":
                out.append(f"| {nome} | 0 | — | — | inconclusivo |")
                continue
            ic = c["dif_taxa_sucesso_ic95"]
            ric = c["razao_cpst_ic95"]
            gate = c.get("gate", {})
            out.append(f"| {nome} | {c['tarefas_pareadas']} | {c['dif_taxa_sucesso']:+.1%} ({ic[0]:+.1%} a {ic[1]:+.1%}) | "
                       f"{_fmt(c['razao_cpst'])} ({'—' if not ric else f'{ric[0]:.2f}–{ric[1]:.2f}'}) | "
                       f"{gate.get('decisao', '—')}{': ' + '; '.join(gate['motivos']) if gate.get('motivos') else ''} |")
    out += _markdown_volume(r)
    out += ["", "CPST = custo de todas as tentativas válidas ÷ sucessos. Custo `relatado` vem do harness "
            "(numa assinatura é a estimativa do harness, não fatura); `estimado` usa os preços declarados nas premissas."]
    return "\n".join(out) + "\n"


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("tentativas", nargs="+", type=Path)
    p.add_argument("--premissas", type=Path, required=True, help="preços e custos locais declarados (JSON)")
    p.add_argument("--protocolo", type=Path, help="tolerâncias de promoção fixadas antes da rodada (JSON)")
    p.add_argument("--json", type=Path)
    p.add_argument("--md", type=Path)
    a = p.parse_args(argv)
    try:
        premissas = json.loads(a.premissas.read_text(encoding="utf-8"))
        protocolo = json.loads(a.protocolo.read_text(encoding="utf-8")) if a.protocolo else None
        r = relatorio(carregar(a.tentativas), premissas, protocolo)
    except ErroDados as e:
        print(json.dumps({"erro": "dados", "motivo": str(e)}), file=sys.stderr)
        return 2
    texto = json.dumps(r, ensure_ascii=False, indent=2, sort_keys=True)
    if a.json:
        # LF em todo SO: os bytes versionados não dependem de onde o relatório foi gerado.
        a.json.write_text(texto + "\n", encoding="utf-8", newline="\n")
    md = markdown(r)
    if a.md:
        a.md.write_text(md, encoding="utf-8", newline="\n")
    if not a.json and not a.md:
        # Console do Windows (cp1252) não codifica Δ/≥: a tabela sai sempre em UTF-8.
        sys.stdout.buffer.write(md.encode("utf-8"))
        sys.stdout.flush()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
