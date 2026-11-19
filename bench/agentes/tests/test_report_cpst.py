"""Relatório de CPST (#88): agregação offline, sem rede e sem chamadas pagas."""

import json
import sys
import tempfile
import unittest
from pathlib import Path

BENCH = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(BENCH))
import report_cpst as rc  # noqa: E402
import run_agent_tasks as r  # noqa: E402

PROTOCOLO = json.loads((BENCH / "protocolo_promocao.json").read_text(encoding="utf-8"))
RELATADO = json.loads((BENCH / "premissas" / "claude-code-relatado.json").read_text(encoding="utf-8"))
TAREFAS = ["P03", "P04", "P05", "P08", "P09", "P10"]


def linha(tarefa="P03", braco="A", sucesso=True, custo=0.5, repeticao=0, tentativa=0, status="concluida",
          valida=True, uso=None, modelo="m1", total=60.0, adotou=None, versao="1.0"):
    return {
        "schema": rc.SCHEMA_TENTATIVA, "tipo": "tentativa", "run_id": "r1", "tarefa": tarefa, "braco": braco,
        "repeticao": repeticao, "tentativa": tentativa, "status": status, "sucesso": sucesso, "valida": valida,
        "custo_usd": custo, "uso": uso if uso is not None else {c: 100 for c in rc.CATEGORIAS_USO},
        "modelo": modelo, "modelo_pedido": modelo, "harness": {"nome": "h", "versao": versao},
        "prompt_sha256": "p" * 64, "duracao_s": {"setup": 1.0, "agente": total - 2, "avaliacao": 1.0, "total": total},
        "ferramentas": {"total": 4, "por_nome": {}, "fastfile": 1 if adotou else 0},
        "adotou_fastfile": adotou if braco != "A" else None,
        "recursos": {"cpu_s": 10.0, "rss_max_mb": 200.0, "fonte": "psutil_amostragem"},
    }


def lote(custo_a=1.0, custo_b=0.5, sucesso_b=True, tarefas=TAREFAS, reps=3):
    out = []
    for rep in range(reps):
        for t in tarefas:
            out.append(linha(t, "A", custo=custo_a, repeticao=rep))
            out.append(linha(t, "B", custo=custo_b, sucesso=sucesso_b, repeticao=rep, adotou=True))
    return out


class Cpst(unittest.TestCase):
    def test_falhas_e_retentativas_entram_no_custo(self):
        linhas = [linha(sucesso=True, custo=1.0), linha(sucesso=False, custo=2.0, status="timeout"),
                  linha(sucesso=False, custo=0.5, status="erro_harness"),
                  linha(sucesso=True, custo=0.5, tentativa=1)]
        a = rc.relatorio(linhas, RELATADO)["bracos"]["A"]
        self.assertEqual(a["custo_usd"], 4.0)
        self.assertEqual(a["sucessos"], 2)
        self.assertEqual(a["cpst_usd"], 2.0)
        self.assertEqual(a["retentativas"], 1)
        self.assertEqual(a["status"], {"concluida": 2, "timeout": 1, "erro_harness": 1})

    def test_zero_sucessos_e_indefinido_com_gasto_divulgado(self):
        a = rc.relatorio([linha(sucesso=False, custo=1.5)] * 3, RELATADO)["bracos"]["A"]
        self.assertIsNone(a["cpst_usd"])
        self.assertEqual(a["cpst_status"], "indefinido_zero_sucessos")
        self.assertEqual(a["custo_usd"], 4.5)
        self.assertIn("indefinido (0/3)", rc.markdown(rc.relatorio([linha(sucesso=False)] * 3, RELATADO)))

    def test_custo_desconhecido_bloqueia_a_alegacao(self):
        linhas = [linha(custo=1.0), linha(custo=None)]
        a = rc.relatorio(linhas, RELATADO)["bracos"]["A"]
        self.assertIsNone(a["custo_usd"])
        self.assertIsNone(a["cpst_usd"])
        self.assertEqual(a["cpst_status"], "indisponivel")
        self.assertEqual(a["custo_usd_parcial"], 1.0)
        self.assertIn("incompleto", rc.markdown(rc.relatorio(linhas, RELATADO)))

    def test_preco_declarado_estima_sem_inventar(self):
        premissas = {"id": "x", "precos_usd_por_mtok": {"m1": {"input_tokens": 3.0, "output_tokens": 15.0}}}
        uso = {"input_tokens": 1_000_000, "output_tokens": 100_000, "cache_creation_input_tokens": 0,
               "cache_read_input_tokens": 0}
        a = rc.relatorio([linha(custo=None, uso=uso)], premissas)["bracos"]["A"]
        self.assertAlmostEqual(a["custo_usd"], 4.5)
        self.assertEqual(a["cpst_status"], "estimado")
        outro = rc.relatorio([linha(custo=None, uso=uso, modelo="m2")], premissas)["bracos"]["A"]
        self.assertEqual(outro["cpst_status"], "indisponivel", "modelo sem preço declarado não é estimado")

    def test_amortizacao_e_cpu_declaradas_somam_por_tentativa(self):
        premissas = {"id": "x", "usd_por_cpu_hora": 0.36, "amortizacao_setup": {"B": {"usd": 1.0, "tarefas": 10}}}
        rel = rc.relatorio([linha(braco="B", custo=1.0, adotou=True), linha(braco="A", custo=1.0)], premissas)
        self.assertAlmostEqual(rel["bracos"]["B"]["custo_usd"], 1.0 + 0.001 + 0.1)
        self.assertAlmostEqual(rel["bracos"]["A"]["custo_usd"], 1.0 + 0.001)

    def test_contaminadas_ficam_fora_e_bloqueiam_o_gate(self):
        linhas = lote() + [linha("P03", "B", valida=False, custo=9.0, adotou=True)]
        rel = rc.relatorio(linhas, RELATADO, PROTOCOLO)
        self.assertEqual(rel["bracos"]["B"]["contaminadas"], 1)
        self.assertEqual(rel["bracos"]["B"]["custo_usd"], 0.5 * 18)
        self.assertEqual(rel["comparacoes"]["B_vs_A"]["gate"]["decisao"], "inconclusivo")

    def test_serie_nao_mistura_modelo_nem_versao(self):
        with self.assertRaises(rc.ErroDados):
            rc.relatorio([linha(modelo="m1"), linha(modelo="m2")], RELATADO)
        with self.assertRaises(rc.ErroDados):
            rc.relatorio([linha(versao="1"), linha(versao="2")], RELATADO)

    def test_lote_interrompido_aparece_como_incompleto(self):
        parada = {"schema": rc.SCHEMA_TENTATIVA, "tipo": "parada", "motivo": "orcamento_esgotado", "pendentes": 7}
        rel = rc.relatorio([linha(), parada], RELATADO)
        self.assertTrue(rel["incompleto"])
        self.assertIn("orcamento_esgotado (7 pendentes)", rc.markdown(rel))


class Gate(unittest.TestCase):
    def test_reducao_clara_aprova(self):
        rel = rc.relatorio(lote(custo_a=1.0, custo_b=0.5), RELATADO, PROTOCOLO)
        comp = rel["comparacoes"]["B_vs_A"]
        self.assertAlmostEqual(comp["razao_cpst"], 0.5)
        self.assertEqual(comp["gate"]["decisao"], "aprovado", comp["gate"])

    def test_custo_maior_reprova(self):
        rel = rc.relatorio(lote(custo_a=0.5, custo_b=1.0), RELATADO, PROTOCOLO)
        self.assertEqual(rel["comparacoes"]["B_vs_A"]["gate"]["decisao"], "reprovado")

    def test_sucesso_pior_reprova_mesmo_mais_barato(self):
        rel = rc.relatorio(lote(custo_b=0.1, sucesso_b=False), RELATADO, PROTOCOLO)
        gate = rel["comparacoes"]["B_vs_A"]["gate"]
        self.assertEqual(gate["decisao"], "reprovado")
        self.assertIn("sucesso", gate["motivos"][0])

    def test_poucas_tarefas_e_inconclusivo(self):
        rel = rc.relatorio(lote(tarefas=TAREFAS[:3]), RELATADO, PROTOCOLO)
        self.assertEqual(rel["comparacoes"]["B_vs_A"]["gate"]["decisao"], "inconclusivo")

    def test_bootstrap_e_deterministico(self):
        linhas = lote()
        linhas[0]["custo_usd"] = 3.0
        a = rc.relatorio(linhas, RELATADO, PROTOCOLO)["comparacoes"]
        b = rc.relatorio(linhas, RELATADO, PROTOCOLO)["comparacoes"]
        self.assertEqual(a, b)


class Integracao(unittest.TestCase):
    def test_linhas_reais_do_runner_simulado(self):
        """O relatório aceita o JSONL que o runner grava e a CLI produz JSON e Markdown."""
        with tempfile.TemporaryDirectory(prefix="ff-cpst-") as d:
            d = Path(d)
            tarefas = r.tarefas_selecionadas(["P03", "P05"])
            env = {k: v for k, v in __import__("os").environ.items() if not r._de_sessao_hospedeira(k)}
            servidor = d / "srv.py"
            servidor.write_text(
                "import json,sys\nfor l in sys.stdin:\n m=json.loads(l)\n"
                " r={'tools':[{'name':'find_files'}]} if m['method']=='tools/list' else {}\n"
                " print(json.dumps({'jsonrpc':'2.0','id':m['id'],'result':r}),flush=True)\n", encoding="utf-8")
            cfg = r.Config(adaptador=r.Simulado(), avaliar=lambda t, p: {"exit_code": 0, "reason": "tests_passed"},
                           saida=d / "t.jsonl", raiz_trabalho=d / "area", ff_comando=[sys.executable, str(servidor)],
                           ff_binario=servidor, env_base=env)
            r.executar(cfg, tarefas, r.planejar(tarefas, 1, ("A", "B")))
            premissas = d / "p.json"
            premissas.write_text(json.dumps(RELATADO), encoding="utf-8")
            codigo = rc.main([str(d / "t.jsonl"), "--premissas", str(premissas),
                              "--protocolo", str(BENCH / "protocolo_promocao.json"),
                              "--json", str(d / "r.json"), "--md", str(d / "r.md")])
            self.assertEqual(codigo, 0)
            rel = json.loads((d / "r.json").read_text(encoding="utf-8"))
            self.assertEqual(rel["schema"], rc.SCHEMA_RELATORIO)
            self.assertEqual(rel["bracos"]["A"]["sucessos"], 2)
            self.assertEqual(rel["bracos"]["B"]["adocao_fastfile"], 1.0)
            # 2 tarefas < mínimo do protocolo: nunca aprova. Reprovado também é válido, porque o
            # p95 de duração aqui é tempo real de parede e oscila no CI.
            self.assertNotEqual(rel["comparacoes"]["B_vs_A"]["gate"]["decisao"], "aprovado")
            md = (d / "r.md").read_text(encoding="utf-8")
            self.assertEqual(md, rc.markdown(rel), "tabela reconstruída só a partir do relatório")
            self.assertNotIn(str(d), md)

    def test_baselines_versionadas_se_regeneram(self):
        """Cada relatório publicado em resultados/ é exatamente o que as linhas versionadas produzem."""
        pastas = sorted(p for p in (BENCH / "resultados").iterdir() if p.is_dir())
        self.assertTrue(pastas)
        for pasta in pastas:
            versionado = json.loads((pasta / "cpst.json").read_text(encoding="utf-8"))
            linhas = rc.carregar([pasta / "tentativas.jsonl"])
            atual = rc.relatorio(linhas, versionado["premissas"], versionado["protocolo"])
            self.assertEqual(json.loads(json.dumps(atual)), versionado, pasta.name)
            self.assertEqual((pasta / "cpst.md").read_text(encoding="utf-8"), rc.markdown(versionado), pasta.name)
            digest = __import__("hashlib").sha256((pasta / "tentativas.jsonl").read_bytes()).hexdigest()
            self.assertIn(digest, (BENCH / "RESULTADOS.md").read_text(encoding="utf-8"),
                          f"{pasta.name}: SHA-256 publicado no RESULTADOS.md não confere")

    def test_cli_imprime_tabela_em_console_cp1252(self):
        import os
        import subprocess
        with tempfile.TemporaryDirectory() as d:
            arq = Path(d) / "t.jsonl"
            arq.write_text("".join(json.dumps(l) + "\n" for l in lote(reps=1)), encoding="utf-8")
            prem = Path(d) / "p.json"
            prem.write_text(json.dumps(RELATADO), encoding="utf-8")
            proc = subprocess.run([sys.executable, str(BENCH / "report_cpst.py"), str(arq), "--premissas", str(prem),
                                   "--protocolo", str(BENCH / "protocolo_promocao.json")],
                                  capture_output=True, env={**os.environ, "PYTHONIOENCODING": "cp1252", "PYTHONUTF8": "0"})
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertIn("Δ sucesso", proc.stdout.decode("utf-8"))

    def test_cli_recusa_esquema_desconhecido(self):
        with tempfile.TemporaryDirectory() as d:
            arq = Path(d) / "x.jsonl"
            arq.write_text(json.dumps({"schema": "outro/9"}) + "\n", encoding="utf-8")
            prem = Path(d) / "p.json"
            prem.write_text("{}", encoding="utf-8")
            self.assertEqual(rc.main([str(arq), "--premissas", str(prem)]), 2)


if __name__ == "__main__":
    unittest.main()
