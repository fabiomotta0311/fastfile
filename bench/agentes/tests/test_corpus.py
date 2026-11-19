import datetime as dt
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

AQUI = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(AQUI))
import corpus  # noqa: E402


class Gabarito(unittest.TestCase):
    def test_duas_execucoes_geram_o_mesmo_gabarito(self):
        self.assertEqual(corpus.serializar(corpus.gerar()), corpus.serializar(corpus.gerar()))

    def test_gabarito_versionado_confere_com_o_fixture(self):
        atual = corpus.GABARITOS.read_text(encoding="utf-8")
        self.assertEqual(atual, corpus.serializar(corpus.gerar()))

    def test_digest_do_fixture_congelado(self):
        g = json.loads(corpus.GABARITOS.read_text(encoding="utf-8"))
        self.assertEqual(g["fixture_digest"], corpus.digest_fixture())

    def test_workspace_difere_do_checkout(self):
        c = corpus.gerar()["cenarios"]["02_todos_config_toml"]
        ws = {i["path"] for i in c["workspace"]}
        co = {i["path"] for i in c["checkout"]}
        self.assertEqual(co - ws, {"examples/standalone/Cargo.toml"})
        self.assertNotIn("target/debug/build/Cargo.toml", co)

    def test_janela_de_tempo_usa_relogio_controlado(self):
        base = corpus.relogio_padrao()
        chave = "04_arquivos_alterados_1h"
        no_relogio = corpus.gerar(base)["cenarios"][chave]["arquivos"]
        self.assertEqual(no_relogio, ["crates/ws-core/src/money.rs", "crates/ws-text/src/header.rs"])
        # 1 s depois, header.rs (3599 s) sai da janela
        depois = corpus.gerar(base + dt.timedelta(seconds=1))["cenarios"][chave]["arquivos"]
        self.assertEqual(depois, ["crates/ws-core/src/money.rs"])

    def test_limite_de_1gb_e_estrito_e_segredos_ficam_fora(self):
        g = corpus.gerar()["cenarios"]
        self.assertEqual([a["path"] for a in g["05_arquivos_maior_1gb"]["arquivos"]], ["dados/grande.bin"])
        arqs = {o["arquivo"] for o in g["06_grep_repo_grande"]["ocorrencias"]}
        self.assertNotIn("config/.env", arqs)
        self.assertFalse(any(a.startswith("target/") for a in arqs))
        self.assertIn("docs/notas.md", arqs)

    def test_parse_header_tem_uma_definicao_apesar_das_mencoes(self):
        d = corpus.gerar()["cenarios"]["03_onde_definido_x"]["definicoes"]
        self.assertEqual([x["arquivo"] for x in d], ["crates/ws-text/src/header.rs"])

    def test_tarefas_apontam_para_o_gabarito(self):
        g = json.loads(corpus.GABARITOS.read_text(encoding="utf-8"))["cenarios"]
        arqs = sorted((AQUI / "tarefas").glob("*.json"))
        self.assertEqual(len(arqs), 6)
        for p in arqs:
            t = json.loads(p.read_text(encoding="utf-8"))
            self.assertIn(t["id"], g)
            self.assertEqual(t["gabarito"], f"gabaritos.json#cenarios/{t['id']}")
            self.assertNotIn("ground_truth", t)


class Correcao(unittest.TestCase):
    def resposta_certa(self, cen):
        g = corpus.gerar()["cenarios"][cen]
        return {
            "01_diagnostico_disco": {"pastas": g.get("pastas")},
            "02_todos_config_toml": {"arquivos": g.get("workspace")},
            "03_onde_definido_x": {"definicoes": g.get("definicoes")},
            "04_arquivos_alterados_1h": {"arquivos": g.get("arquivos")},
            "05_arquivos_maior_1gb": {"arquivos": g.get("arquivos")},
            "06_grep_repo_grande": {"ocorrencias": g.get("ocorrencias")},
        }[cen]

    def test_resposta_exata_aprova_nos_seis_cenarios(self):
        for cen in corpus.gerar()["cenarios"]:
            self.assertTrue(corpus.corrigir(cen, self.resposta_certa(cen))["aprovado"], cen)

    def test_arquivo_faltando_reprova(self):
        r = self.resposta_certa("02_todos_config_toml")
        r["arquivos"] = r["arquivos"][:-1]
        v = corpus.corrigir("02_todos_config_toml", r)
        self.assertFalse(v["aprovado"])
        self.assertEqual(len(v["faltando"]), 1)

    def test_arquivo_sobrando_reprova(self):
        r = self.resposta_certa("04_arquivos_alterados_1h")
        r["arquivos"] = r["arquivos"] + ["docs/notas.md"]
        v = corpus.corrigir("04_arquivos_alterados_1h", r)
        self.assertFalse(v["aprovado"])
        self.assertEqual(v["sobrando"], [("docs/notas.md",)])

    def test_checkout_no_lugar_de_workspace_reprova(self):
        g = corpus.gerar()["cenarios"]["02_todos_config_toml"]
        self.assertFalse(corpus.corrigir("02_todos_config_toml", {"arquivos": g["checkout"]})["aprovado"])

    def test_resposta_vazia_so_aprova_quando_o_gabarito_e_vazio(self):
        self.assertFalse(corpus.corrigir("05_arquivos_maior_1gb", {"arquivos": []})["aprovado"])
        vazio = {"cenarios": {"05_arquivos_maior_1gb": {"arquivos": []}}}
        self.assertTrue(corpus.corrigir("05_arquivos_maior_1gb", {"arquivos": []}, vazio)["aprovado"])

    def test_ordem_importa_no_diagnostico_de_disco(self):
        r = self.resposta_certa("01_diagnostico_disco")
        r["pastas"] = list(reversed(r["pastas"]))
        self.assertFalse(corpus.corrigir("01_diagnostico_disco", r)["aprovado"])

    def test_formato_invalido_reprova_sem_excecao(self):
        self.assertFalse(corpus.corrigir("06_grep_repo_grande", {"x": 1})["aprovado"])

    def test_caminhos_normalizados(self):
        r = self.resposta_certa("04_arquivos_alterados_1h")
        r["arquivos"] = ["./" + p.replace("/", "\\") for p in r["arquivos"]]
        self.assertTrue(corpus.corrigir("04_arquivos_alterados_1h", r)["aprovado"])


class Correcoes(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tarefas = corpus.carregar_correcoes()

    def test_dez_tarefas_com_ids_unicos(self):
        ids = [t["id"] for t in self.tarefas]
        self.assertEqual(len(ids), 10)
        self.assertEqual(len(set(ids)), 10)

    def test_cobre_homonimos_teste_em_outro_modulo_e_impacto_entre_crates(self):
        cats = {c for t in self.tarefas for c in t["categorias"]}
        self.assertTrue({"homonimos", "teste_outro_modulo", "impacto_entre_crates"} <= cats)

    def test_cada_tarefa_tem_revisao_teste_discriminante_e_regressoes(self):
        for t in self.tarefas:
            with self.subTest(t["id"]):
                self.assertEqual(len(t["estado_inicial_digest"]), 64)
                d = t["teste_discriminante"]
                self.assertIn(d["funcao"], d["conteudo"])
                self.assertEqual(t["falhas_esperadas_antes"], [d["funcao"]])
                self.assertTrue(t["regressoes"] and t["escopo_esperado"])

    def test_revisao_reproduz_o_mesmo_checkout(self):
        for t in self.tarefas:
            with self.subTest(t["id"]), tempfile.TemporaryDirectory() as d:
                self.assertEqual(corpus.materializar(t, Path(d) / "co"), t["estado_inicial_digest"])

    def test_checkout_nao_expoe_solucao_nem_historico(self):
        for t in self.tarefas:
            with self.subTest(t["id"]), tempfile.TemporaryDirectory() as d:
                co = Path(d) / "co"
                corpus.materializar(t, co)
                nomes = {p.name for p in co.rglob("*")}
                self.assertTrue(nomes.isdisjoint({".git", "avaliador", "gabaritos.json", "target"}))
                self.assertFalse((co / t["teste_discriminante"]["arquivo"]).exists())

    def test_visao_do_agente_nao_vaza_campos_privados(self):
        for t in self.tarefas:
            v = corpus.visao_agente(t)
            self.assertTrue(set(corpus.CAMPOS_PRIVADOS).isdisjoint(v))
            texto = json.dumps(v)
            for m in t["mutacoes"]:
                self.assertNotIn(m["de"], texto)
                self.assertNotIn(m["para"], texto)

    def test_holdout_separado_por_familia(self):
        fams = [t["familia"] for t in self.tarefas]
        d = corpus.divisao_por_familia(fams)
        self.assertTrue(d["holdout"] and d["piloto"])
        self.assertTrue(set(d["holdout"]).isdisjoint(d["piloto"]))
        self.assertEqual(sorted(d["holdout"] + d["piloto"]), sorted(set(fams)))
        self.assertEqual(d, corpus.divisao_por_familia(list(reversed(fams))))

    def test_mutacao_ambigua_e_rejeitada(self):
        t = {**self.tarefas[0], "mutacoes": [{"arquivo": "Cargo.toml", "de": "e", "para": "y"}]}
        with tempfile.TemporaryDirectory() as d, self.assertRaises(ValueError):
            corpus.materializar(t, Path(d) / "co")

    @unittest.skipUnless(os.environ.get("FF_BENCH_CARGO") == "1", "defina FF_BENCH_CARGO=1 (usa cargo)")
    def test_testes_discriminam_com_cargo_real(self):
        import verificar_correcoes

        for t in self.tarefas:
            with self.subTest(t["id"]):
                self.assertEqual(verificar_correcoes.verificar(t), [])


if __name__ == "__main__":
    unittest.main()
