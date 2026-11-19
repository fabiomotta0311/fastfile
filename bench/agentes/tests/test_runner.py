"""Runner dos braços A/B/C (#87): smoke offline, sem modelo, sem rede e sem gasto."""

import importlib.util
import json
import os
import shutil
import sys
import tempfile
import time
import unittest
from collections import Counter
from pathlib import Path

BENCH = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(BENCH))
import corpus  # noqa: E402
import grade_patch  # noqa: E402
import run_agent_tasks as r  # noqa: E402

SERVIDOR_FALSO = r'''
import json, os, sys
nomes = ["find_files", "read_files", "find_symbol"]
extra = os.environ.get("FAKE_EXTRA", "")
nomes += [n for n in extra.split(",") if n]
exp = os.environ.get("FASTFILE_EXPERIMENTAL", "")
if "context" in exp: nomes.append("repair_context")
if "validate_plan" in exp: nomes.append("validate_plan")
for linha in sys.stdin:
    msg = json.loads(linha)
    if msg.get("method") == "initialize":
        print(json.dumps({"jsonrpc": "2.0", "id": msg["id"], "result": {}}), flush=True)
    elif msg.get("method") == "tools/list":
        print(json.dumps({"jsonrpc": "2.0", "id": msg["id"],
                          "result": {"tools": [{"name": n} for n in nomes]}}), flush=True)
'''

CLAUDE_FALSO = r'''
import json, sys, time
modo = open(sys.argv[0] + ".modo", encoding="utf-8").read().strip()
def ev(x): print(json.dumps(x), flush=True)
ev({"type": "system", "subtype": "init", "session_id": "s-" + str(time.time_ns()), "model": "m-falso",
    "tools": ["Read", "Edit"], "mcp_servers": []})
ev({"type": "assistant", "message": {"content": [
    {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "src/lib.rs"}},
    {"type": "text", "text": "ok"}]}})
ev({"type": "user", "message": {"content": [
    {"type": "tool_result", "tool_use_id": "t1", "content": [{"type": "text", "text": "ação" * 10}]}]}})
if modo == "lento":
    time.sleep(60)
if modo == "orfao":
    import os, subprocess
    # Filho que sobrevive ao pai com o checkout como diretório atual (como um helper do harness).
    subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"], cwd=os.getcwd(),
                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(1.0)
if modo == "recusa":
    sys.stderr.write("Error: recusado pelo harness\n")
    sys.exit(1)
if modo == "erro_api":
    ev({"type": "result", "subtype": "success", "is_error": True, "session_id": "x", "num_turns": 1,
        "result": "Invalid API key - Please run /login", "total_cost_usd": 0,
        "usage": {"input_tokens": 0, "output_tokens": 0}})
    sys.exit(1)
if modo != "sem_resultado":
    ev({"type": "result", "subtype": "success", "is_error": False, "session_id": "x", "num_turns": 2,
        "total_cost_usd": 0.0123, "usage": {"input_tokens": 10, "output_tokens": 5,
        "cache_creation_input_tokens": 1, "cache_read_input_tokens": 2}})
'''


def avaliador_offline(tarefa, patch_arquivo):
    """Avaliação sem Docker: aceita se o diff tocar exatamente o escopo esperado."""
    texto = patch_arquivo.read_text(encoding="utf-8")
    tocados = {l[len("diff --git a/"):].split(" b/")[0] for l in texto.splitlines() if l.startswith("diff --git a/")}
    ok = tocados == set(tarefa["escopo_esperado"])
    return {"exit_code": 0 if ok else 1, "reason": "tests_passed" if ok else "candidate_tests_failed"}


class Base(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory(prefix="ff-runner-test-")
        self.tmp = Path(self._tmp.name)
        self.addCleanup(self._tmp.cleanup)
        servidor = self.tmp / "servidor.py"
        servidor.write_text(SERVIDOR_FALSO, encoding="utf-8")
        self.ff = servidor
        # Os testes podem rodar dentro de uma sessão do Claude Code: o ambiente base não a herda.
        self.env = {k: v for k, v in os.environ.items()
                    if k.upper() not in ("FAKE_EXTRA", "FASTFILE_EXPERIMENTAL") and not r._de_sessao_hospedeira(k)}

    def cfg(self, adaptador=None, **kw):
        base = dict(adaptador=adaptador or r.Simulado(), avaliar=avaliador_offline,
                    saida=self.tmp / "out" / "tentativas.jsonl", raiz_trabalho=self.tmp / "area",
                    ff_comando=[sys.executable, str(self.ff)], ff_binario=self.ff, env_base=self.env)
        base.update(kw)
        return r.Config(**base)

    def tarefas(self, ids=("P01",)):
        return r.tarefas_selecionadas(list(ids), incluir_holdout=True)


class Planejamento(Base):
    def test_ordem_balanceada_e_deterministica(self):
        tarefas = self.tarefas(["P01", "P02", "P03"])
        plano = r.planejar(tarefas, 3, semente=7)
        self.assertEqual(plano, r.planejar(tarefas, 3, semente=7))
        self.assertEqual(len(plano), 3 * 3 * 3)
        por_celula = Counter((p["tarefa"], p["repeticao"]) for p in plano)
        self.assertTrue(all(n == 3 for n in por_celula.values()))
        primeiro = Counter(p["braco"] for i, p in enumerate(plano) if i % 3 == 0)
        self.assertEqual(sorted(primeiro.values()), [3, 3, 3])

    def test_holdout_fica_fora_por_padrao(self):
        todas = corpus.carregar_correcoes()
        holdout = set(corpus.divisao(todas)["holdout"])
        padrao = r.tarefas_selecionadas()
        self.assertTrue(padrao)
        self.assertFalse({t["familia"] for t in padrao} & holdout)
        alvo = next(t["id"] for t in todas if t["familia"] in holdout)
        with self.assertRaises(r.ErroConfiguracao):
            r.tarefas_selecionadas([alvo])


class Bracos(Base):
    def test_path_do_agente_nao_ve_o_fastfile(self):
        bin_ff = self.tmp / "ffdir"
        bin_ff.mkdir()
        nome = "ff.exe" if os.name == "nt" else "ff"
        (bin_ff / nome).write_bytes(b"")
        (bin_ff / nome).chmod(0o755)
        outro = self.tmp / "outro"
        outro.mkdir()
        path = os.pathsep.join([str(bin_ff), str(outro)])
        self.assertEqual(r.path_sem_fastfile(path, None), str(outro))

    def test_harness_no_mesmo_diretorio_do_fastfile_e_recusado(self):
        d = self.tmp / "junto"
        d.mkdir()
        sufixo = ".exe" if os.name == "nt" else ""
        for n in ("ff", "claude"):
            (d / (n + sufixo)).write_bytes(b"")
            (d / (n + sufixo)).chmod(0o755)
        with self.assertRaises(r.ErroConfiguracao):
            r.path_sem_fastfile(str(d), "claude")

    def test_ambiente_do_agente_nao_herda_a_sessao_hospedeira(self):
        base = {**self.env, "CLAUDECODE": "1", "CLAUDE_CODE_SESSION_ID": "s", "CLAUDE_CODE_MESSAGING_TOKEN": "t",
                "CLAUDE_AGENT_SDK_VERSION": "v", "FASTFILE_EXPERIMENTAL": "context", "ANTHROPIC_API_KEY": "k"}
        env = r.ambiente_agente(base, None)
        for nome in ("CLAUDECODE", "CLAUDE_CODE_SESSION_ID", "CLAUDE_CODE_MESSAGING_TOKEN",
                     "CLAUDE_AGENT_SDK_VERSION", "FASTFILE_EXPERIMENTAL"):
            self.assertNotIn(nome, env)
        self.assertEqual(env["ANTHROPIC_API_KEY"], "k", "credencial do próprio harness é preservada")

    def test_harness_real_dentro_de_sessao_e_recusado(self):
        adaptador = r.Simulado()
        adaptador.medido = True
        cfg = self.cfg(adaptador=adaptador, orcamento_total_usd=1.0, env_base={**self.env, "CLAUDECODE": "1"})
        with self.assertRaisesRegex(r.ErroConfiguracao, "terminal comum"):
            r.executar(cfg, self.tarefas(), r.planejar(self.tarefas(), 1, ("A",)))
        self.assertFalse(cfg.saida.exists())

    def test_catalogo_por_braco(self):
        env = dict(self.env)
        cmd = [sys.executable, str(self.ff)]
        b = r.catalogo_mcp(r.servidor_mcp("B", cmd), env)
        c = r.catalogo_mcp(r.servidor_mcp("C", cmd), {**env, "FASTFILE_EXPERIMENTAL": "context"})
        r.validar_catalogo("B", b)
        r.validar_catalogo("C", c)
        self.assertFalse(set(b) & r.EXPERIMENTAIS, "o ambiente do harness não pode vazar para B")
        self.assertTrue(r.EXPERIMENTAIS <= set(c))

    def test_braco_b_contaminado_e_recusado_antes_de_gastar(self):
        cfg = self.cfg(env_base={**self.env, "FAKE_EXTRA": "validate_plan"})
        with self.assertRaises(r.ErroConfiguracao):
            r.executar(cfg, self.tarefas(), r.planejar(self.tarefas(), 1, ("B",)))
        self.assertFalse(cfg.saida.exists())

    def test_area_de_trabalho_dentro_do_repositorio_e_recusada(self):
        cfg = self.cfg(raiz_trabalho=BENCH / "nao-criar")
        with self.assertRaises(r.ErroConfiguracao):
            r.executar(cfg, self.tarefas(), r.planejar(self.tarefas(), 1))
        self.assertFalse((BENCH / "nao-criar").exists())


class Smoke(Base):
    def test_smoke_abc_uma_linha_por_tentativa(self):
        tarefas = self.tarefas(["P01", "P03"])
        plano = r.planejar(tarefas, 2)
        linhas = r.executar(self.cfg(), tarefas, plano, run_id="smoke")
        gravadas = [json.loads(l) for l in (self.tmp / "out" / "tentativas.jsonl").read_text(encoding="utf-8").splitlines()]
        self.assertEqual(gravadas, json.loads(json.dumps(linhas, ensure_ascii=False)))
        self.assertEqual(len(gravadas), len(plano))
        for linha in gravadas:
            self.assertEqual(linha["schema"], r.SCHEMA)
            self.assertEqual(linha["run_id"], "smoke")
            self.assertEqual(linha["status"], "concluida")
            self.assertTrue(linha["sucesso"], linha)
            self.assertTrue(linha["valida"])
            self.assertEqual(linha["uso_status"], "observado")
            self.assertEqual(len(linha["ff_sha256"]), 64)
            self.assertIn("total", linha["duracao_s"])
        adocao = {l["braco"]: l["adotou_fastfile"] for l in gravadas}
        self.assertEqual(adocao, {"A": None, "B": True, "C": True})
        self.assertEqual({l["attempt_id"] for l in gravadas}.__len__(), len(gravadas))
        self.assertEqual(list((self.tmp / "area").iterdir()), [], "área descartável deve ser limpa")

    def test_registro_nao_tem_caminhos_locais(self):
        linhas = r.executar(self.cfg(), self.tarefas(), r.planejar(self.tarefas(), 1))
        texto = json.dumps(linhas, ensure_ascii=False)
        for segredo in (str(self.tmp), str(Path.home()), str(BENCH)):
            self.assertNotIn(segredo, texto)
            self.assertNotIn(segredo.replace("\\", "\\\\"), texto)

    def test_contadores_indisponiveis_sao_marcados(self):
        cfg = self.cfg(adaptador=r.Simulado(lambda p: "sem_uso"))
        linha = r.executar(cfg, self.tarefas(), r.planejar(self.tarefas(), 1, ("A",)))[0]
        self.assertIsNone(linha["uso"])
        self.assertEqual(linha["uso_status"], "indisponivel")
        self.assertEqual(linha["custo_fonte"], "indisponivel")

    def test_harness_real_sem_custo_para_o_lote(self):
        adaptador = r.Simulado(lambda p: "sem_uso")
        adaptador.medido = True
        cfg = self.cfg(adaptador=adaptador, orcamento_total_usd=1.0)
        linhas = r.executar(cfg, self.tarefas(["P01", "P03"]), r.planejar(self.tarefas(["P01", "P03"]), 1, ("A",)))
        self.assertEqual([l["tipo"] for l in linhas], ["tentativa", "parada"])
        self.assertEqual(linhas[-1]["motivo"], "custo_inverificavel")

    def test_harness_real_exige_teto(self):
        adaptador = r.Simulado()
        adaptador.medido = True
        with self.assertRaises(r.ErroConfiguracao):
            r.executar(self.cfg(adaptador=adaptador), self.tarefas(), r.planejar(self.tarefas(), 1, ("A",)))

    def test_timeout_conta_como_tentativa_com_custo(self):
        cfg = self.cfg(adaptador=r.Simulado(lambda p: "timeout"))
        linha = r.executar(cfg, self.tarefas(), r.planejar(self.tarefas(), 1, ("A",)))[0]
        self.assertEqual(linha["status"], "timeout")
        self.assertFalse(linha["sucesso"])
        self.assertEqual(linha["veredito"]["reason"], "nao_avaliado")
        self.assertEqual(linha["custo_usd"], 0.01)

    def test_orcamento_para_antes_de_estourar(self):
        tarefas = self.tarefas(["P01", "P03"])
        cfg = self.cfg(adaptador=r.Simulado(custo_usd=0.4), orcamento_total_usd=1.0, orcamento_tentativa_usd=0.4)
        linhas = r.executar(cfg, tarefas, r.planejar(tarefas, 1))
        tentativas = [l for l in linhas if l["tipo"] == "tentativa"]
        self.assertEqual(len(tentativas), 2)
        self.assertEqual(linhas[-1]["motivo"], "orcamento_esgotado")
        self.assertEqual(linhas[-1]["pendentes"], 4)

    def test_erro_de_harness_gera_retentativa_registrada(self):
        chamadas = []

        def comportamento(p):
            chamadas.append(1)
            return "erro" if len(chamadas) == 1 else "resolver"

        cfg = self.cfg(adaptador=r.Simulado(comportamento), retentativas=1)
        linhas = r.executar(cfg, self.tarefas(), r.planejar(self.tarefas(), 1, ("A",)))
        self.assertEqual([(l["tentativa"], l["status"]) for l in linhas], [(0, "erro_harness"), (1, "concluida")])

    def test_gabarito_acessado_invalida_a_tentativa(self):
        cfg = self.cfg(adaptador=r.Simulado(lambda p: "ler_gabarito"))
        linha = r.executar(cfg, self.tarefas(), r.planejar(self.tarefas(), 1, ("A",)))[0]
        self.assertIn("material_privado_acessado", linha["contaminacao"])
        self.assertFalse(linha["valida"])
        self.assertFalse(linha["sucesso"])
        self.assertEqual(linha["veredito"]["reason"], "nao_avaliado")

    def test_sessao_repetida_e_contaminacao(self):
        tarefas = self.tarefas(["P01", "P03"])
        cfg = self.cfg(adaptador=r.Simulado(lambda p: "sessao_fixa"))
        linhas = r.executar(cfg, tarefas, r.planejar(tarefas, 1, ("A",)))
        self.assertEqual(linhas[0]["contaminacao"], [])
        self.assertIn("sessao_repetida", linhas[1]["contaminacao"])

    def test_sem_mudanca_reprova(self):
        cfg = self.cfg(adaptador=r.Simulado(lambda p: "nada"))
        linha = r.executar(cfg, self.tarefas(), r.planejar(self.tarefas(), 1, ("A",)))[0]
        self.assertTrue(linha["patch_vazio"])
        self.assertFalse(linha["sucesso"])


class Patch(Base):
    def test_patch_aplica_com_git_e_ignora_gerados(self):
        tarefa = self.tarefas()[0]
        original, trabalho = self.tmp / "o", self.tmp / "t"
        corpus.materializar(tarefa, original)
        corpus.materializar(tarefa, trabalho)
        alvo = trabalho / tarefa["escopo_esperado"][0]
        alvo.write_bytes(alvo.read_bytes().rstrip(b"\n") + b"\n// fim sem quebra")
        (trabalho / "target" / "debug").mkdir(parents=True)
        (trabalho / "target" / "debug" / "x").write_bytes(b"\0")
        (trabalho / "Cargo.lock").write_text("# gerado\n", encoding="utf-8")
        (trabalho / ".claude").mkdir()
        (trabalho / ".claude" / "s.json").write_text("{}", encoding="utf-8")
        patch = r.extrair_patch(original, trabalho, ignorar={".claude"})
        self.assertEqual(patch.count("diff --git"), 1, patch)
        self.assertIn("\\ No newline at end of file", patch)
        # Mesmo caminho do avaliador: sem configuração do operador (ex.: core.autocrlf).
        self.assertEqual(grade_patch.git_apply(original, patch.encode("utf-8"), "--whitespace=nowarn").returncode, 0)
        self.assertEqual(alvo.read_bytes(), (original / tarefa["escopo_esperado"][0]).read_bytes())

    def test_arquivo_novo_aparece_no_patch(self):
        tarefa = self.tarefas()[0]
        original, trabalho = self.tmp / "o", self.tmp / "t"
        corpus.materializar(tarefa, original)
        corpus.materializar(tarefa, trabalho)
        (trabalho / "novo.rs").write_text("fn x() {}\n", encoding="utf-8")
        patch = r.extrair_patch(original, trabalho)
        self.assertIn("new file mode 100644", patch)
        self.assertIn("+++ b/novo.rs", patch)


class AdaptadorClaude(Base):
    def falso(self, modo):
        script = self.tmp / "claude_falso.py"
        script.write_text(CLAUDE_FALSO, encoding="utf-8")
        Path(str(script) + ".modo").write_text(modo, encoding="utf-8")
        return [sys.executable, str(script)]

    def pedido(self, timeout=30, mcp=None):
        trabalho = self.tmp / "w"
        trabalho.mkdir(exist_ok=True)
        return r.Pedido(trabalho=trabalho, prompt="p", env=dict(self.env), mcp=mcp, timeout_s=timeout,
                        orcamento_usd=0.5, modelo="m")

    def test_stream_json_vira_uso_custo_e_ferramentas(self):
        ex = r.executar_stream(self.falso("ok"), self.pedido())
        self.assertEqual(ex.status, "concluida")
        self.assertEqual(ex.custo_usd, 0.0123)
        self.assertEqual(ex.uso["input_tokens"], 10)
        self.assertEqual(ex.ferramentas, ["Read"])
        self.assertEqual(ex.resultados_bytes, [60], "bytes UTF-8 do tool_result casado pelo id")
        self.assertEqual(ex.mcp_sessao, [])
        self.assertTrue(ex.sessao.startswith("s-"))

    def test_sem_evento_result_nao_inventa_uso(self):
        ex = r.executar_stream(self.falso("sem_resultado"), self.pedido())
        self.assertEqual(ex.status, "erro_harness")
        self.assertIsNone(ex.uso)
        self.assertIsNone(ex.custo_usd)

    def test_recusa_do_harness_vira_diagnostico_fora_do_registro(self):
        ex = r.executar_stream(self.falso("recusa"), self.pedido())
        self.assertEqual(ex.status, "erro_harness")
        self.assertIn("recusado pelo harness", ex.diagnostico)

    @unittest.skipUnless(importlib.util.find_spec("psutil"), "psutil ausente")
    def test_filho_orfao_e_encerrado_e_a_area_pode_ser_apagada(self):
        pedido = self.pedido(timeout=30)
        inicio = time.monotonic()
        ex = r.executar_stream(self.falso("orfao"), pedido)
        self.assertLess(time.monotonic() - inicio, 20, "o pipe herdado pelo órfão não pode segurar a tentativa")
        self.assertEqual(ex.status, "concluida")
        self.assertGreaterEqual(ex.recursos["processos_encerrados"], 1)
        shutil.rmtree(pedido.trabalho)
        self.assertFalse(pedido.trabalho.exists())

    def test_erro_no_evento_result_vira_diagnostico(self):
        ex = r.executar_stream(self.falso("erro_api"), self.pedido())
        self.assertEqual(ex.status, "erro_harness")
        self.assertEqual(ex.resultado_subtipo, "success")
        self.assertIn("Invalid API key", ex.diagnostico)

    def test_timeout_mata_o_processo(self):
        ex = r.executar_stream(self.falso("lento"), self.pedido(timeout=3))
        self.assertEqual(ex.status, "timeout")
        self.assertEqual(ex.ferramentas, ["Read"])

    def test_argv_isola_a_sessao_e_so_libera_o_fastfile_em_b_c(self):
        cc = r.ClaudeCode("claude")
        a = cc.argv(self.pedido(), "mcp.json")
        b = cc.argv(self.pedido(mcp=r.servidor_mcp("B", ["ff"])), "mcp.json")
        for flag in ("--strict-mcp-config", "--no-session-persistence", "--max-budget-usd", "stream-json"):
            self.assertIn(flag, a)
        self.assertNotIn("mcp__fastfile__*", a)
        self.assertIn("mcp__fastfile__*", b)
        self.assertEqual([x for x in a if x != "mcp__fastfile__*"], [x for x in b if x != "mcp__fastfile__*"])

    def test_braco_d_desliga_so_a_busca_nativa(self):
        cc = r.ClaudeCode("claude")
        servidor = r.servidor_mcp("D", ["ff"])
        self.assertEqual(servidor["env"], {}, "D não liga as experimentais")
        b = cc.argv(self.pedido(mcp=r.servidor_mcp("B", ["ff"])), "mcp.json")
        pd = self.pedido(mcp=servidor)
        pd.braco = "D"
        d = cc.argv(pd, "mcp.json")
        self.assertNotIn("--disallowedTools", b)
        i = d.index("--disallowedTools")
        self.assertEqual(d[i + 1:i + 3], ["Glob", "Grep"])
        self.assertIn("Read", d[d.index("--allowedTools"):i], "Edit exige Read")
        r.validar_catalogo("D", ["find_files"])
        with self.assertRaises(r.ErroConfiguracao):
            r.validar_catalogo("D", ["find_files", "validate_plan"])

    def test_sonda_aprova_captura_completa(self):
        class Falso(r.ClaudeCode):
            def __init__(s, argv):
                super().__init__()
                s._argv = argv

            def executar(s, pedido):
                return r.executar_stream(s._argv, pedido)

        relatorio = r.sondar(Falso(self.falso("ok")), dict(self.env), 30, 0.1, None)
        self.assertTrue(relatorio["aprovado"], relatorio)
        relatorio = r.sondar(Falso(self.falso("sem_resultado")), dict(self.env), 30, 0.1, None)
        self.assertFalse(relatorio["aprovado"])
        self.assertIn("uso", relatorio["faltando"])


if __name__ == "__main__":
    unittest.main()


class Volume(unittest.TestCase):
    def test_ponderado_releituras_e_repetidas(self):
        ff = r.PREFIXO_MCP_FF
        nomes = ["Grep", "Read", f"{ff}read_files", "Read", "Grep", "Edit"]
        entradas = [json.dumps(x) for x in (
            {"pattern": "fn x"}, {"file_path": "./src/a.rs"}, {"paths": ["src/a.rs", "src/b.rs"]},
            {"file_path": "src\\b.rs"}, {"pattern": "fn x"}, {"file_path": "src/a.rs"})]
        bytes_ = [100, 10_000, 4_000, 50, 100, None]
        v = r.metricas_volume(nomes, entradas, bytes_)
        self.assertEqual(v["chamadas"], 6)
        self.assertEqual(v["resultados_observados"], 5)
        self.assertEqual(v["bytes_total"], 14_250)
        self.assertEqual(v["bytes_fastfile"], 4_000)
        # bytes x (chamadas restantes + resposta final)
        self.assertEqual(v["bytes_ponderado"], 100 * 6 + 10_000 * 5 + 4_000 * 4 + 50 * 3 + 100 * 2)
        self.assertEqual(v["bytes_por_ferramenta"], {"Grep": 200, "Read": 10_050, f"{ff}read_files": 4_000})
        self.assertEqual(v["respostas_grandes"], 1)
        self.assertEqual(v["arquivos_lidos"], 2)
        self.assertEqual(v["releituras"], 2, "a.rs via read_files e b.rs via Read")
        self.assertEqual(v["chamadas_repetidas"], 1)

    def test_sem_ferramentas(self):
        v = r.metricas_volume([], [], [])
        self.assertEqual((v["bytes_total"], v["bytes_ponderado"], v["releituras"]), (0, 0, 0))

    def test_entrada_invalida_nao_quebra(self):
        v = r.metricas_volume(["Read"], ["{nao json"], [5])
        self.assertEqual((v["bytes_total"], v["arquivos_lidos"]), (5, 0))
