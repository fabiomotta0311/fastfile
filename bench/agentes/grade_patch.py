#!/usr/bin/env python3
"""Avalia um diff do corpus B01. Somente o harness confiável chama este script."""

import argparse
import hashlib
import json
import os
import re
import subprocess
import tempfile
import threading
import uuid
from collections import Counter
from pathlib import Path

import corpus

MAX_PATCH = 256 * 1024
MAX_OUTPUT = 512 * 1024
CONTROL_TIMEOUT = 15
IMAGE_ID = re.compile(r"sha256:[a-f0-9]{64}\Z")
TEST = re.compile(rb"^test (\S+) \.\.\. (ok|FAILED|ignored)(?:,.*)?$", re.M)
SUMMARY = re.compile(rb"^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;", re.M)


class Stop(Exception):
    def __init__(self, code, reason):
        self.code, self.reason = code, reason


def git_apply(root, patch, *opts):
    # Não usar repositório, índice, configuração ou filtros do operador.
    env = {k: v for k, v in os.environ.items() if not k.upper().startswith("GIT_")}
    env.update(GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull,
               GIT_CEILING_DIRECTORIES=str(root.parent.parent))
    return subprocess.run(["git", "apply", "-p1", *opts, "-"], input=patch, cwd=root,
                          env=env, capture_output=True, timeout=CONTROL_TIMEOUT)


def apply(root, patch, scope):
    if not patch.strip():
        raise Stop(1, "empty_patch")
    stats = git_apply(root, patch, "--numstat", "-z")
    summary = git_apply(root, patch, "--summary")
    if stats.returncode or summary.returncode or not stats.stdout:
        raise Stop(1, "invalid_patch")
    if summary.stdout:
        raise Stop(1, "scope_violation")
    allowed = {x.encode("utf-8") for x in scope}
    for record in stats.stdout.rstrip(b"\0").split(b"\0"):
        fields = record.split(b"\t", 2)
        if len(fields) != 3 or fields[2] not in allowed or not all(x.isdigit() for x in fields[:2]):
            raise Stop(1, "scope_violation")
    before = snapshot(root)
    if git_apply(root, patch, "--whitespace=nowarn").returncode:
        raise Stop(1, "patch_does_not_apply")
    after = snapshot(root)
    if before.keys() != after.keys():
        raise Stop(1, "scope_violation")
    paths = sorted(p for p in before if before[p] != after[p])
    if not paths:
        raise Stop(1, "empty_change")
    if not set(paths) <= set(scope):
        raise Stop(1, "scope_violation")
    # No B01 os testes unitários em arquivos editáveis ocupam o sufixo do arquivo.
    for path in paths:
        marker = b"\n#[cfg(test)]\n"
        if before[path][1].partition(marker)[1:] != after[path][1].partition(marker)[1:]:
            raise Stop(1, "preserved_tests_modified")
    return paths


def snapshot(root):
    files = {}
    for p in root.rglob("*"):
        if p.is_symlink() or not (p.is_file() or p.is_dir()):
            raise Stop(1, "scope_violation")
        if p.is_file():
            files[p.relative_to(root).as_posix()] = (p.stat().st_mode, p.read_bytes())
    return files


def stage_oracle(root, task):
    oracle = task["teste_discriminante"]
    target = root / oracle["arquivo"]
    target.parent.mkdir(parents=True, exist_ok=True)
    with target.open("xb") as dest:
        dest.write(oracle["conteudo"].encode("utf-8"))
    lock = corpus.CORRECOES.with_suffix(".lock").read_bytes()
    with (root / "Cargo.lock").open("xb") as dest:
        dest.write(lock)
    # O fixture de descoberta inclui um .env sintético que não é necessário ao Cargo.
    (root / "config" / ".env").unlink()


def control(argv):
    return subprocess.run(argv, stdin=subprocess.DEVNULL, capture_output=True, timeout=CONTROL_TIMEOUT)


def local_docker(scratch):
    if os.environ.get("DOCKER_CONTEXT") or not os.environ.get("DOCKER_HOST"):
        result = control(["docker", "context", "inspect", "--format", "{{.Endpoints.docker.Host}}"])
        if result.returncode:
            raise Stop(3, "sandbox_context_failed")
        endpoint = result.stdout.decode().strip()
    else:
        endpoint = os.environ["DOCKER_HOST"]
    if not endpoint.startswith(("unix:///", "npipe:////./pipe/")):
        raise Stop(3, "sandbox_not_local")
    config = scratch / "docker-config"
    config.mkdir()
    return ["docker", "--config", str(config), "--host", endpoint]


def verify_image(docker, image):
    info = control([*docker, "image", "inspect", "--format", "{{json .}}", image])
    if info.returncode:
        raise Stop(3, "sandbox_image_unavailable")
    spec = json.loads(info.stdout)
    if spec["Id"] != image or spec["Os"] != "linux" or spec["Config"].get("Volumes"):
        raise Stop(3, "unsupported_sandbox_image")


def attach(docker, name, timeout):
    # Lê no máximo um bloco além do teto; finalmente o daemon remove todo o container.
    output = []
    with subprocess.Popen([*docker, "start", "--attach", name], stdin=subprocess.DEVNULL,
                          stdout=subprocess.PIPE, stderr=subprocess.DEVNULL) as proc:
        def read():
            output.append(proc.stdout.read(MAX_OUTPUT + 1))

        reader = threading.Thread(target=read, daemon=True)
        reader.start()
        try:
            reader.join(timeout)
            if reader.is_alive():
                raise Stop(2, "sandbox_timeout")
            if len(output) != 1:
                raise Stop(3, "sandbox_output_failed")
            if len(output[0]) > MAX_OUTPUT:
                raise Stop(1, "sandbox_output_limit")
            proc.wait(timeout=CONTROL_TIMEOUT)
        finally:
            if proc.poll() is None:
                proc.kill()
            reader.join()
    # start --attach pode relatar erro do cliente; o daemon confirma o estado/exit code.
    status = control([*docker, "inspect", "--format", "{{json .State}}", name])
    if status.returncode:
        raise Stop(3, "sandbox_status_failed")
    state = json.loads(status.stdout)
    if state["Status"] != "exited" or state["Error"] or state["ExitCode"] in (125, 126, 127):
        raise Stop(3, "sandbox_start_failed")
    return state["ExitCode"], output[0].replace(b"\r\n", b"\n")


def run(docker, root, image, argv, timeout):
    name = f"fastfile-grade-{uuid.uuid4().hex}"
    create = [*docker, "create", "--name", name, "--label", "fastfile.cpst.grade",
              "--pull=never", "--network=none", "--read-only", "--user=65534:65534",
              "--cap-drop=ALL", "--security-opt=no-new-privileges", "--ipc=none",
              "--pids-limit=128", "--memory=768m", "--memory-swap=768m", "--cpus=2",
              "--ulimit=nofile=256:256", "--ulimit=core=0:0", "--log-driver=none",
              "--tmpfs=/tmp:rw,exec,nosuid,nodev,size=256m,mode=1777",
              "--mount", f"type=bind,source={root.as_posix()},target=/fixture,readonly",
              "--workdir=/fixture", "--entrypoint=/usr/bin/env", image, "-i",
              "PATH=/usr/local/cargo/bin:/usr/local/bin:/usr/bin:/bin", "HOME=/tmp",
              "CARGO_HOME=/tmp/cargo", "RUSTUP_HOME=/usr/local/rustup", "TMPDIR=/tmp",
              "CARGO_TARGET_DIR=/tmp/target", "CARGO_INCREMENTAL=0", "CARGO_BUILD_JOBS=2",
              "CARGO_TERM_COLOR=never", "LC_ALL=C", *argv]
    # Remover também se o pedido de create ficou sem resposta; nunca podar outras áreas.
    created = False
    try:
        result = control(create)
        if result.returncode:
            raise Stop(3, "sandbox_create_failed")
        created = True
        return attach(docker, name, timeout)
    finally:
        try:
            result = control([*docker, "rm", "--force", "--volumes", name])
        except (subprocess.TimeoutExpired, OSError) as e:
            raise Stop(3, "sandbox_cleanup_failed") from e
        if result.returncode and (created or b"No such container" not in result.stderr):
            raise Stop(3, "sandbox_cleanup_failed")


def suites(rc, output, discriminant):
    rows = Counter(TEST.findall(output))
    summaries = [tuple(map(int, m)) for m in SUMMARY.findall(output)]
    counts = tuple(sum(n for (_, state), n in rows.items() if state == s)
                   for s in (b"ok", b"FAILED", b"ignored"))
    complete = bool(summaries) and tuple(map(sum, zip(*summaries))) == (*counts, 0, 0)
    inventory = Counter()
    for (name, _), n in rows.items():
        inventory[name] += n
    return rows, inventory, complete, {
        "exit_code": rc, "passed": counts[0], "failed": counts[1], "ignored": counts[2],
        "discriminant_passed": rows[discriminant, b"ok"] == 1,
    }


def grade(task, patch_path, image, timeout, evidence):
    with patch_path.open("rb") as f:
        patch = f.read(MAX_PATCH + 1)
    if len(patch) > MAX_PATCH:
        raise Stop(1, "patch_too_large")
    evidence["patch_sha256"] = hashlib.sha256(patch).hexdigest()
    with tempfile.TemporaryDirectory(prefix="fastfile-grade-") as directory:
        scratch = Path(directory)
        baseline, candidate, reference = (scratch / n for n in ("baseline", "candidate", "reference"))
        evidence["phase"] = "patch"
        for tree in (baseline, candidate):
            if corpus.materializar(task, tree) != task["estado_inicial_digest"]:
                raise Stop(3, "baseline_digest_mismatch")
        evidence["changed_paths"] = apply(candidate, patch, task["escopo_esperado"])
        corpus.materializar({**task, "mutacoes": []}, reference)
        for tree in (baseline, reference, candidate):
            stage_oracle(tree, task)
        evidence["phase"] = "baseline"
        docker = local_docker(scratch)
        verify_image(docker, image)
        toolchains = task["regressoes"]
        if toolchains != [["cargo", "test", "--workspace", "--no-fail-fast"]]:
            raise Stop(3, "unsupported_corpus_command")
        argv = [*toolchains[0], "--offline", "--locked", "--", "--format=pretty", "--test-threads=1"]
        disc = task["teste_discriminante"]["funcao"].encode()
        expected_fails = Counter(n.encode() for n in task["falhas_esperadas_antes"])
        for tree in (baseline, reference, candidate):
            phase = tree.name
            evidence["phase"] = phase
            try:
                rows, inventory, complete, record = suites(*run(docker, tree, image, argv, timeout), disc)
            except Stop as e:
                if phase != "candidate" and e.code == 1:
                    raise Stop(3, e.reason) from e
                raise
            evidence["runs"][phase] = record
            if phase == "baseline":
                fails = Counter({n: count for (n, s), count in rows.items() if s == b"FAILED"})
                if not complete or record["exit_code"] != 101 or fails != expected_fails or inventory[disc] != 1 or record["ignored"]:
                    raise Stop(3, "baseline_not_discriminating")
                baseline_inventory = inventory
            else:
                passed = (complete and inventory == baseline_inventory and record["exit_code"] == 0
                          and not record["failed"] and not record["ignored"] and record["discriminant_passed"])
                if not passed:
                    if phase == "reference":
                        raise Stop(3, "reference_not_green")
                    raise Stop(1, "candidate_tests_failed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("task", help="ID da tarefa B01 (P01–P10)")
    parser.add_argument("patch", type=Path, help="diff Git relativo à raiz do fixture")
    parser.add_argument("--image", required=True, help="ID sha256:... de imagem Rust local e confiável")
    parser.add_argument("--timeout", type=float, default=30, help="segundos por suíte (0 < n <= 300)")
    args = parser.parse_args()
    evidence = {"schema": "fastfile-grade/1", "phase": "input", "runs": {}}
    try:
        if not IMAGE_ID.fullmatch(args.image) or not 0 < args.timeout <= 300:
            raise Stop(3, "invalid_sandbox_configuration")
        if not re.fullmatch(r"P(?:0[1-9]|10)", args.task):
            raise Stop(3, "unknown_task")
        evidence.update(task=args.task, sandbox_image=args.image)
        task = next((t for t in corpus.carregar_correcoes() if t["id"] == args.task), None)
        if task is None:
            raise Stop(3, "unknown_task")
        evidence["baseline_sha256"] = task["estado_inicial_digest"]
        grade(task, args.patch, args.image, args.timeout, evidence)
    except Stop as e:
        code, reason = e.code, e.reason
    except subprocess.TimeoutExpired:
        code, reason = 3, "control_timeout"
    except (OSError, ValueError, KeyError, TypeError):
        code, reason = 3, "infrastructure_error"
    else:
        code, reason = 0, "tests_passed"
    print(json.dumps({**evidence, "exit_code": code, "reason": reason}, sort_keys=True))
    return code


if __name__ == "__main__":
    raise SystemExit(main())
