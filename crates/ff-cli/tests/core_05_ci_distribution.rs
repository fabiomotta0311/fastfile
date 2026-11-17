//! Testes de regressão e validação para CORE-05:
//! Estabilizar CI e distribuição nas plataformas anunciadas.
//!
//! Critérios de aceite:
//! - Validar o pacote recomendado, seus executáveis e instruções.
//! - Gerar checksums e manifests a partir do mesmo build.
//! - Identificar o commit de origem dos artefatos.
//! - Instalação funciona em ambiente limpo, sem Rust instalado.
//! - Instruções apontam para arquivos realmente presentes.
//! - Plataformas ainda não validadas ficam identificadas como experimentais.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

struct TestDirGuard {
    path: PathBuf,
}

impl TestDirGuard {
    fn new(prefix: &str) -> Self {
        let unique = format!(
            "ff_test_{prefix}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        fs::create_dir_all(&path).expect("criar pasta temporária");
        Self { path }
    }
}

impl Drop for TestDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn test_core_05_release_manifest_generator_tracks_commit_and_no_pending() {
    let guard = TestDirGuard::new("dist_gen");
    let base_path = &guard.path;
    let artifacts_dir = base_path.join("artifacts");
    let output_dir = base_path.join("out");
    fs::create_dir_all(&artifacts_dir).unwrap();
    fs::create_dir_all(&output_dir).unwrap();

    // Cria artefato simulado de Windows
    let win_zip_name = "fastfile-v0.2.0-windows-x86_64.zip";
    let win_zip_path = artifacts_dir.join(win_zip_name);
    fs::write(&win_zip_path, b"dummy zip payload 123456").unwrap();

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let script_path = repo_root
        .join("scripts")
        .join("generate_release_manifests.py");

    let test_commit_sha = "abcdef1234567890deadbeefcafebabe12345678";

    // Executa generate_release_manifests.py
    let output = Command::new("python")
        .arg(&script_path)
        .arg("--version")
        .arg("0.2.0")
        .arg("--commit")
        .arg(test_commit_sha)
        .arg("--artifacts-dir")
        .arg(&artifacts_dir)
        .arg("--output-dir")
        .arg(&output_dir)
        .arg("--repo-root")
        .arg(&repo_root)
        .output()
        .expect("executar python generate_release_manifests.py");

    assert!(
        output.status.success(),
        "Gerador de manifestos falhou: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 1. Valida que release-metadata.json foi gerado com o commit informado (CORE-05)
    let meta_path = output_dir.join("release-metadata.json");
    assert!(
        meta_path.exists(),
        "release-metadata.json deve ser gerado no build"
    );
    let meta_content = fs::read_to_string(&meta_path).unwrap();
    assert!(
        meta_content.contains(test_commit_sha),
        "Metadata deve registrar o commit de origem exato dos artefatos: {meta_content}"
    );

    // 2. Valida cabeçalho de SHA256SUMS.txt com identificação do commit
    let sha_path = output_dir.join("SHA256SUMS.txt");
    assert!(sha_path.exists(), "SHA256SUMS.txt deve ser gerado");
    let sha_content = fs::read_to_string(&sha_path).unwrap();
    assert!(
        sha_content.contains(test_commit_sha),
        "SHA256SUMS.txt deve conter o commit SHA no cabeçalho"
    );
    assert!(
        sha_content.contains(win_zip_name),
        "SHA256SUMS.txt deve listar o artefato gerado"
    );

    // 3. Valida que os manifestos Scoop e WinGet não contêm marcadores PENDING_*
    let scoop_path = output_dir.join("fastfile.json");
    if scoop_path.exists() {
        let scoop_content = fs::read_to_string(&scoop_path).unwrap();
        assert!(
            !scoop_content.contains("PENDING"),
            "Manifesto Scoop não pode conter marcadores PENDING: {scoop_content}"
        );
    }

    let winget_path = output_dir.join("fastfile.yaml");
    if winget_path.exists() {
        let winget_content = fs::read_to_string(&winget_path).unwrap();
        assert!(
            !winget_content.contains("PENDING"),
            "Manifesto WinGet não pode conter marcadores PENDING: {winget_content}"
        );
        assert!(
            winget_content.contains("0.2.0"),
            "Manifesto WinGet deve refletir a versão"
        );
    }
}

#[test]
fn test_core_05_instructions_and_installer_scripts_presence() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    // 1. Valida que os scripts indicados nas instruções existem tanto na raiz quanto em scripts/
    assert!(
        repo_root.join("scripts").join("install.ps1").exists(),
        "scripts/install.ps1 deve existir"
    );
    assert!(
        repo_root.join("install.ps1").exists(),
        "install.ps1 deve existir na raiz para conveniência do usuário"
    );
    assert!(
        repo_root.join("scripts").join("uninstall.ps1").exists(),
        "scripts/uninstall.ps1 deve existir"
    );
    assert!(
        repo_root.join("uninstall.ps1").exists(),
        "uninstall.ps1 deve existir na raiz"
    );

    // 2. Valida que o README aponta corretamente para os arquivos e documenta o suporte experimental
    let readme = fs::read_to_string(repo_root.join("README.md")).expect("ler README.md");
    assert!(
        readme.contains("install.ps1"),
        "README deve instruir uso de install.ps1"
    );
    assert!(
        readme.contains("macOS") && readme.contains("Experimental"),
        "Plataforma macOS deve estar explicitamente classificada como experimental no README"
    );
}
