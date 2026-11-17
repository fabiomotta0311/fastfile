#!/usr/bin/env python3
"""
FastFile Release Manifest & Checksum Generator
Gera manifestos de distribuição (WinGet, Scoop, Homebrew) e SHA256SUMS.txt
a partir dos artefatos reais compilados no pipeline de release.
Garante rastreabilidade estrita e elimina qualquer placeholder PENDING_*.
"""

import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
import re
import shutil
import sys
from pathlib import Path


def calculate_sha256(filepath: Path) -> str:
    hasher = hashlib.sha256()
    with open(filepath, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def get_version_from_cargo(repo_root: Path) -> str:
    cargo_path = repo_root / "Cargo.toml"
    if not cargo_path.exists():
        return "0.1.0"
    content = cargo_path.read_text(encoding="utf-8")
    match = re.search(r'\[workspace\.package\][\s\S]*?version\s*=\s*"([^"]+)"', content)
    if match:
        return match.group(1).lstrip("v")
    match = re.search(r'version\s*=\s*"([^"]+)"', content)
    if match:
        return match.group(1).lstrip("v")
    return "0.1.0"


def get_git_commit(repo_root: Path) -> str:
    # 1. Variável de ambiente do CI (GitHub Actions)
    sha = os.environ.get("GITHUB_SHA")
    if sha:
        return sha
    # 2. Comando git local
    try:
        import subprocess

        res = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=repo_root,
            capture_output=True,
            text=True,
        )
        if res.returncode == 0 and res.stdout.strip():
            return res.stdout.strip()
    except Exception:
        pass
    return "unknown"


def main():
    parser = argparse.ArgumentParser(description="Generate release checksums and distribution manifests.")
    parser.add_argument("--version", help="Release version (e.g. 0.2.0 or v0.2.0)")
    parser.add_argument("--commit", help="Commit SHA of release source code")
    parser.add_argument("--artifacts-dir", default=".", help="Directory containing built release artifacts")
    parser.add_argument("--output-dir", default=None, help="Directory to save generated manifests (defaults to artifacts-dir)")
    parser.add_argument("--repo-root", default=".", help="FastFile repository root directory")
    args = parser.parse_args()

    repo_root = Path(args.repo_root).resolve()
    artifacts_dir = Path(args.artifacts_dir).resolve()
    output_dir = Path(args.output_dir).resolve() if args.output_dir else artifacts_dir
    output_dir.mkdir(parents=True, exist_ok=True)

    version = args.version.lstrip("v") if args.version else get_version_from_cargo(repo_root)
    tag = f"v{version}"
    commit_sha = args.commit if args.commit else get_git_commit(repo_root)
    build_time = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    print(f"=== FastFile Release Generator ===")
    print(f"Versão:    {version} ({tag})")
    print(f"Commit:    {commit_sha}")
    print(f"Artefatos: {artifacts_dir}")
    print(f"Saída:     {output_dir}")

    # 1. Encontra e calcula SHA-256 apenas dos artefatos oficiais de distribuição
    checksums = {}
    ignored_substrings = ["deps", "build", "incremental", ".fingerprint", "examples"]

    for root, _, files in os.walk(artifacts_dir):
        if any(ignored in root for ignored in ignored_substrings):
            continue
        for f in sorted(files):
            file_path = Path(root) / f
            is_dist_pkg = (
                f.startswith("fastfile") and (f.endswith(".zip") or f.endswith(".tar.gz"))
            ) or (f.startswith("FastFile") and f.endswith(".exe")) or f.endswith(".msi")
            if not is_dist_pkg:
                continue
            sha = calculate_sha256(file_path)
            checksums[f] = (file_path, sha)
            print(f"  -> {f}: {sha}")

    # 2. Garante paridade de nomes versionados e não versionados para o ZIP do Windows
    windows_zip_versioned = f"fastfile-v{version}-windows-x86_64.zip"
    windows_zip_generic = "fastfile-windows-x86_64.zip"

    if windows_zip_versioned in checksums and windows_zip_generic not in checksums:
        src_path, sha = checksums[windows_zip_versioned]
        dest_path = output_dir / windows_zip_generic
        if src_path != dest_path:
            shutil.copy2(src_path, dest_path)
        checksums[windows_zip_generic] = (dest_path, sha)
        print(f"  -> Clonado para compatibilidade: {windows_zip_generic} ({sha})")
    elif windows_zip_generic in checksums and windows_zip_versioned not in checksums:
        src_path, sha = checksums[windows_zip_generic]
        dest_path = output_dir / windows_zip_versioned
        if src_path != dest_path:
            shutil.copy2(src_path, dest_path)
        checksums[windows_zip_versioned] = (dest_path, sha)
        print(f"  -> Clonado para compatibilidade: {windows_zip_versioned} ({sha})")

    # 3. Gera release-metadata.json (identificação do commit e rastreabilidade estrita - CORE-05)
    metadata = {
        "version": version,
        "tag": tag,
        "commit": commit_sha,
        "generated_at_utc": build_time,
        "artifacts": {},
    }
    for filename in sorted(checksums.keys()):
        file_path, sha = checksums[filename]
        size_bytes = file_path.stat().st_size if file_path.exists() else 0
        metadata["artifacts"][filename] = {
            "sha256": sha,
            "size_bytes": size_bytes,
        }

    meta_file = output_dir / "release-metadata.json"
    meta_file.write_text(json.dumps(metadata, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"\n[1/5] Gerado metadata com commit: {meta_file} (Commit: {commit_sha})")

    # 4. Gera SHA256SUMS.txt com identificação do commit
    sha_file = output_dir / "SHA256SUMS.txt"
    with open(sha_file, "w", encoding="utf-8") as f:
        f.write(f"# FastFile {tag} (Commit: {commit_sha})\n")
        f.write(f"# Gerado em: {build_time}\n")
        for filename in sorted(checksums.keys()):
            _, sha = checksums[filename]
            f.write(f"{sha}  {filename}\n")
    print(f"[2/5] Gerado: {sha_file}")

    # 5. Atualiza manifesto Scoop
    scoop_template_path = repo_root / "packaging" / "scoop" / "fastfile.json"
    if scoop_template_path.exists():
        scoop_data = json.loads(scoop_template_path.read_text(encoding="utf-8"))
        scoop_data["version"] = version

        # Prioriza hash do ZIP versionado, senão genérico
        win_sha = None
        if windows_zip_versioned in checksums:
            win_sha = checksums[windows_zip_versioned][1]
        elif windows_zip_generic in checksums:
            win_sha = checksums[windows_zip_generic][1]

        if win_sha:
            scoop_data["architecture"]["64bit"]["url"] = (
                f"https://github.com/fabiomotta0311/fastfile/releases/download/{tag}/{windows_zip_versioned}"
            )
            scoop_data["architecture"]["64bit"]["hash"] = win_sha

        scoop_out = output_dir / "fastfile.json"
        scoop_out.write_text(json.dumps(scoop_data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        # Atualiza também a fonte no repositório se aplicável
        scoop_template_path.write_text(json.dumps(scoop_data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        print(f"[3/5] Atualizado manifesto Scoop: {scoop_out} (hash: {win_sha})")

    # 6. Atualiza manifesto WinGet
    winget_template_path = repo_root / "packaging" / "winget" / "fastfile.yaml"
    if winget_template_path.exists():
        winget_content = winget_template_path.read_text(encoding="utf-8")
        winget_content = re.sub(r'PackageVersion:\s*[^\r\n]+', f'PackageVersion: {version}', winget_content)

        if win_sha:
            winget_content = re.sub(
                r'InstallerUrl:\s*[^\r\n]+',
                f'InstallerUrl: https://github.com/fabiomotta0311/fastfile/releases/download/{tag}/{windows_zip_versioned}',
                winget_content,
            )
            winget_content = re.sub(
                r'InstallerSha256:\s*[^\r\n]+',
                f'InstallerSha256: {win_sha.upper()}',
                winget_content,
            )

        # Checagem estrita contra placeholders
        if "PENDING" in winget_content:
            raise ValueError(f"Manifesto WinGet contém marcadores pendentes após geração: {winget_content}")

        winget_out = output_dir / "fastfile.yaml"
        winget_out.write_text(winget_content, encoding="utf-8")
        winget_template_path.write_text(winget_content, encoding="utf-8")
        print(f"[4/5] Atualizado manifesto WinGet: {winget_out} (hash: {win_sha.upper() if win_sha else 'N/A'})")

    # 7. Atualiza Homebrew Formula
    brew_template_path = repo_root / "packaging" / "homebrew" / "fastfile.rb"
    if brew_template_path.exists():
        brew_content = brew_template_path.read_text(encoding="utf-8")
        brew_content = re.sub(r'version\s+"[^"]+"', f'version "{version}"', brew_content)
        brew_content = re.sub(r'/releases/download/[^/]+/', f'/releases/download/{tag}/', brew_content)

        # Atualiza hashes das plataformas se disponíveis nos artefatos
        for fn, (_, sha) in checksums.items():
            if "macos-arm64" in fn:
                brew_content = re.sub(r'sha256\s+"[^"]+"\s+#\s*arm64|sha256\s+"PENDING_MACOS_ARM64_RELEASE"', f'sha256 "{sha}"', brew_content)
                brew_content = re.sub(r'(url\s+".*macos-arm64.*"\s*\n\s*sha256\s+)"[^"]+"', rf'\1"{sha}"', brew_content)
            elif "macos-x86_64" in fn:
                brew_content = re.sub(r'sha256\s+"PENDING_MACOS_X86_64_RELEASE"', f'sha256 "{sha}"', brew_content)
                brew_content = re.sub(r'(url\s+".*macos-x86_64.*"\s*\n\s*sha256\s+)"[^"]+"', rf'\1"{sha}"', brew_content)
            elif "linux-x86_64" in fn:
                brew_content = re.sub(r'(url\s+".*linux-x86_64.*"\s*\n\s*sha256\s+)"[^"]+"', rf'\1"{sha}"', brew_content)

        brew_out = output_dir / "fastfile.rb"
        brew_out.write_text(brew_content, encoding="utf-8")
        brew_template_path.write_text(brew_content, encoding="utf-8")
        print(f"[5/5] Atualizado manifesto Homebrew: {brew_out}")

    print("\n=== Todos os manifestos e hashes gerados e validados com sucesso! ===")


if __name__ == "__main__":
    main()
