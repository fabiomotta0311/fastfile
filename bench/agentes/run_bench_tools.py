#!/usr/bin/env python3
"""
Benchmark Nível 1 — Comparação de Ferramentas CLI
FastFile (§3.6 e Issue #24)

Compara fastfile contra ferramentas padrão (Get-ChildItem/find, Measure-Object/du, ripgrep/grep)
medindo tempo de execução (mediana de 5 rodadas), volume de dados e economia de tokens.
"""

import json
import os
import subprocess
import sys
import time
from typing import Dict, List, Any


def run_command(cmd: List[str], timeout: float = 30.0) -> Dict[str, Any]:
    start = time.perf_counter()
    try:
        proc = subprocess.run(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout,
            shell=(os.name == "nt" and cmd[0].lower() in ["powershell", "powershell.exe"])
        )
        elapsed_ms = (time.perf_counter() - start) * 1000.0
        out_bytes = len(proc.stdout.encode("utf-8"))
        return {
            "success": proc.returncode == 0,
            "elapsed_ms": elapsed_ms,
            "stdout_len": len(proc.stdout),
            "stdout_bytes": out_bytes,
            "estimated_tokens": max(1, out_bytes // 4),
            "exit_code": proc.returncode,
        }
    except Exception as e:
        return {
            "success": False,
            "elapsed_ms": (time.perf_counter() - start) * 1000.0,
            "stdout_len": 0,
            "stdout_bytes": 0,
            "estimated_tokens": 0,
            "exit_code": -1,
            "error": str(e),
        }


def benchmark_cell(cmd: List[str], repetitions: int = 5) -> Dict[str, Any]:
    results = []
    # 1 rodada de aquecimento (warm cache)
    _ = run_command(cmd)

    for _ in range(repetitions):
        res = run_command(cmd)
        if res["success"]:
            results.append(res)

    if not results:
        return {
            "median_ms": 0.0,
            "median_bytes": 0,
            "median_tokens": 0,
            "success": False,
        }

    times = sorted(r["elapsed_ms"] for r in results)
    bytes_list = sorted(r["stdout_bytes"] for r in results)
    tokens_list = sorted(r["estimated_tokens"] for r in results)

    mid = len(times) // 2
    return {
        "median_ms": round(times[mid], 2),
        "median_bytes": bytes_list[mid],
        "median_tokens": tokens_list[mid],
        "success": True,
        "runs": len(results),
    }


def main():
    print("=== FastFile Benchmark Nível 1 (Ferramentas) ===")
    print(f"Sistema Operacional: {sys.platform}")

    benchmarks = []

    # 1. Busca de arquivos por nome
    print("\n[1/3] Testando busca por nome (find)...")
    ff_find = benchmark_cell(["cargo", "run", "-q", "--bin", "fastfile", "--", "find", "Cargo.toml", "--limit", "20", "--json"])
    print(f"  fastfile find: {ff_find['median_ms']} ms ({ff_find['median_tokens']} tokens)")

    if sys.platform == "win32":
        ps_find = benchmark_cell(["powershell", "-NoProfile", "-Command", "Get-ChildItem -Recurse -Filter Cargo.toml | Select-Object FullName, Length"])
        print(f"  PowerShell Get-ChildItem: {ps_find['median_ms']} ms ({ps_find['median_tokens']} tokens)")
        benchmarks.append({
            "tarefa": "Busca Cargo.toml (find)",
            "fastfile": ff_find,
            "padrao": ps_find,
            "ferramenta_padrao": "Get-ChildItem",
        })
    else:
        linux_find = benchmark_cell(["find", ".", "-name", "Cargo.toml"])
        benchmarks.append({
            "tarefa": "Busca Cargo.toml (find)",
            "fastfile": ff_find,
            "padrao": linux_find,
            "ferramenta_padrao": "find",
        })

    # 2. Diagnóstico de disco (du)
    print("\n[2/3] Testando uso de disco (du)...")
    ff_du = benchmark_cell(["cargo", "run", "-q", "--bin", "fastfile", "--", "du", ".", "--top", "5", "--depth", "1", "--json"])
    print(f"  fastfile du: {ff_du['median_ms']} ms ({ff_du['median_tokens']} tokens)")

    if sys.platform == "win32":
        ps_du = benchmark_cell(["powershell", "-NoProfile", "-Command", "Get-ChildItem -Directory | ForEach-Object { [PSCustomObject]@{ Path = $_.Name; Size = (Get-ChildItem $_.FullName -Recurse -File -ErrorAction SilentlyContinue | Measure-Object -Property Length -Sum).Sum } } | Sort-Object Size -Descending | Select-Object -First 5"])
        print(f"  PowerShell du: {ps_du['median_ms']} ms ({ps_du['median_tokens']} tokens)")
        benchmarks.append({
            "tarefa": "Top 5 pastas por tamanho (du)",
            "fastfile": ff_du,
            "padrao": ps_du,
            "ferramenta_padrao": "PowerShell Measure-Object",
        })

    # 3. Busca de conteúdo (grep)
    print("\n[3/3] Testando busca em conteúdo (grep)...")
    ff_grep = benchmark_cell(["cargo", "run", "-q", "--bin", "fastfile", "--", "grep", "SCHEMA_VERSION", "crates/ff-cli/src", "--json"])
    print(f"  fastfile grep: {ff_grep['median_ms']} ms ({ff_grep['median_tokens']} tokens)")

    if sys.platform == "win32":
        ps_grep = benchmark_cell(["powershell", "-NoProfile", "-Command", "Select-String -Path crates\\ff-cli\\src\\*.rs -Pattern SCHEMA_VERSION"])
        print(f"  PowerShell Select-String: {ps_grep['median_ms']} ms ({ps_grep['median_tokens']} tokens)")
        benchmarks.append({
            "tarefa": "Grep 'SCHEMA_VERSION' (grep)",
            "fastfile": ff_grep,
            "padrao": ps_grep,
            "ferramenta_padrao": "Select-String",
        })

    # Salva dados coletados
    out_path = os.path.join(os.path.dirname(__file__), "benchmark_tools_raw.json")
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(benchmarks, f, indent=2, ensure_ascii=False)
    print(f"\nResultados brutos gravados em: {out_path}")


if __name__ == "__main__":
    main()
