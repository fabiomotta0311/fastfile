//! Localização e validação do snapshot do índice (cold start).
//!
//! O snapshot guarda o índice em disco para que o `ff-indexer` não precise reindexar o volume inteiro a
//! cada início: ele carrega o arquivo e retoma pelo USN Journal. É um índice de **nomes de arquivos** da
//! máquina (dado sensível): fica no diretório de dados do próprio usuário, nunca em local compartilhado.

use std::io;
use std::path::{Path, PathBuf};

/// Diretório de snapshots do usuário atual, sem criá-lo.
///
/// Windows: `%LOCALAPPDATA%\FastFile\snapshots` (herda a ACL do perfil: só o usuário e administradores).
/// Unix: `$XDG_STATE_HOME/fastfile/snapshots` ou `~/.local/state/fastfile/snapshots`.
#[must_use]
pub fn snapshot_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let base = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty())?;
        Some(PathBuf::from(base).join("FastFile").join("snapshots"))
    }
    #[cfg(not(windows))]
    {
        let base = std::env::var_os("XDG_STATE_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .filter(|v| !v.is_empty())
                    .map(|h| PathBuf::from(h).join(".local").join("state"))
            })?;
        Some(base.join("fastfile").join("snapshots"))
    }
}

/// Caminho padrão do snapshot de `target` dentro de `dir`.
///
/// O nome combina o alvo "sanitizado" com um hash do texto original, para que `C:` e `C:\` do mesmo
/// volume coincidam e alvos diferentes nunca colidam.
#[must_use]
pub fn snapshot_path_in(dir: &Path, target: &str) -> PathBuf {
    let canonical = canonical_target(target);
    let readable: String = canonical
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .chars()
        .take(40)
        .collect();
    let readable = if readable.is_empty() {
        "volume".to_string()
    } else {
        readable
    };
    dir.join(format!("{readable}-{:08x}.ffidx", fnv32(&canonical)))
}

/// Caminho padrão do snapshot de `target` no diretório do usuário.
#[must_use]
pub fn default_snapshot_path(target: &str) -> Option<PathBuf> {
    Some(snapshot_path_in(&snapshot_dir()?, target))
}

/// Decide qual snapshot o daemon usa.
///
/// - `--no-snapshot` desativa tudo;
/// - `--snapshot <arquivo>` tem prioridade;
/// - sem nenhum dos dois, usa o padrão do usuário (`default_dir`) — exceto em modo serviço, que roda como
///   outra conta (sem diretório de perfil do usuário e sem ACL adequada para um índice de nomes).
///
/// `default_dir` é `None` onde não há retomada incremental (só o Windows tem), o que desativa o padrão.
#[must_use]
pub fn resolve_snapshot_path(
    explicit: Option<&Path>,
    disabled: bool,
    service: bool,
    target: &str,
    default_dir: Option<&Path>,
) -> Option<PathBuf> {
    if disabled {
        return None;
    }
    if let Some(path) = explicit {
        return Some(path.to_path_buf());
    }
    if service {
        return None;
    }
    default_dir.map(|dir| snapshot_path_in(dir, target))
}

/// Cria o diretório do snapshot, privado ao usuário (`0o700` em Unix; no Windows herda a ACL do perfil).
pub fn ensure_private_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// Dois nomes de volume/alvo designam o mesmo índice (ignora maiúsculas e barras finais).
#[must_use]
pub fn same_target(a: &str, b: &str) -> bool {
    canonical_target(a) == canonical_target(b)
}

fn canonical_target(target: &str) -> String {
    target
        .trim()
        .trim_end_matches(['\\', '/'])
        .replace('/', "\\")
        .to_lowercase()
}

fn fnv32(text: &str) -> u32 {
    let mut h = 0x811c_9dc5u32;
    for b in text.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equivalent_volume_spellings_share_one_snapshot() {
        let dir = Path::new("base");
        let a = snapshot_path_in(dir, "C:");
        assert_eq!(a, snapshot_path_in(dir, "c:\\"));
        assert_eq!(a, snapshot_path_in(dir, "C:/"));
        assert_ne!(a, snapshot_path_in(dir, "D:"));
        assert!(same_target("C:", "c:\\"));
        assert!(!same_target("C:", "D:"));
    }

    #[test]
    fn snapshot_name_is_safe_for_any_target() {
        let dir = Path::new("base");
        for target in [
            r"\\.\C:",
            "/home/user/dir com espaço",
            "../../etc/passwd",
            "",
            &"x".repeat(500),
        ] {
            let p = snapshot_path_in(dir, target);
            assert_eq!(
                p.parent(),
                Some(dir),
                "não pode escapar do diretório: {target}"
            );
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            assert!(name.ends_with(".ffidx") && name.len() < 70, "{name}");
            assert!(name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)));
        }
        assert_ne!(
            snapshot_path_in(dir, "/a/b"),
            snapshot_path_in(dir, "/a_b"),
            "alvos distintos não colidem"
        );
    }

    #[test]
    fn snapshot_choice_follows_flags() {
        let dir = Path::new("base");
        let explicit = Path::new("meu.ffidx");
        // padrão do usuário
        assert_eq!(
            resolve_snapshot_path(None, false, false, "C:", Some(dir)),
            Some(snapshot_path_in(dir, "C:"))
        );
        // explícito vence o padrão
        assert_eq!(
            resolve_snapshot_path(Some(explicit), false, false, "C:", Some(dir)),
            Some(explicit.to_path_buf())
        );
        // --no-snapshot vence tudo
        assert_eq!(
            resolve_snapshot_path(Some(explicit), true, false, "C:", Some(dir)),
            None
        );
        // serviço não usa o padrão do usuário, mas respeita um caminho explícito
        assert_eq!(
            resolve_snapshot_path(None, false, true, "C:", Some(dir)),
            None
        );
        assert_eq!(
            resolve_snapshot_path(Some(explicit), false, true, "C:", Some(dir)),
            Some(explicit.to_path_buf())
        );
        // plataforma sem retomada: sem padrão
        assert_eq!(resolve_snapshot_path(None, false, false, "C:", None), None);
    }

    #[test]
    fn private_dir_is_created_recursively() {
        let base = std::env::temp_dir().join(format!("ff-snapdir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let nested = base.join("a").join("b");
        ensure_private_dir(&nested).unwrap();
        assert!(nested.is_dir());
        ensure_private_dir(&nested).unwrap(); // idempotente
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&nested).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o700);
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
