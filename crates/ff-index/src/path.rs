//! Utilitários para normalização e validação de pertencimento a árvores de diretórios.
//!
//! Garante resolução determinística e multiplataforma de caminhos para o FastFile,
//! substituindo comparações ingênuas de prefixo textual por validações de componentes
//! de sistema de arquivos (CORE-01).

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

/// Remove prefixos UNC estendidos do Windows (como `\\?\` ou `\\?\UNC\`).
#[must_use]
pub fn clean_unc_prefix(path_str: &str) -> &str {
    if let Some(stripped) = path_str.strip_prefix(r"\\?\UNC\") {
        // Para UNC, converte \\?\UNC\servidor\share para \\servidor\share
        // Como retornamos &str da string original, se for \\?\UNC\ não dá para apenas fatiar com \\ prefixo,
        // mas na maioria esmagadora é \\?\C:\
        stripped
    } else if let Some(stripped) = path_str.strip_prefix(r"\\?\") {
        stripped
    } else {
        path_str
    }
}

/// Limpa e normaliza um caminho em um `PathBuf` padronizado.
#[must_use]
pub fn clean_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    let cleaned_str = clean_unc_prefix(&s);
    let p = Path::new(cleaned_str);

    let mut comps = Vec::new();
    let mut has_prefix = false;
    let mut has_root = false;

    for comp in p.components() {
        match comp {
            Component::Prefix(_) => {
                has_prefix = true;
                comps.push(comp);
            }
            Component::RootDir => {
                has_root = true;
                comps.push(comp);
            }
            Component::CurDir => {
                // Descarta '.' sintático
            }
            Component::ParentDir => {
                // Volta um nível se possível, desde que não seja a raiz ou prefixo
                if let Some(Component::Normal(_)) = comps.last() {
                    comps.pop();
                }
            }
            Component::Normal(_) => {
                comps.push(comp);
            }
        }
    }

    let mut result = PathBuf::new();
    for c in &comps {
        match c {
            Component::Prefix(pr) => {
                result.push(pr.as_os_str());
            }
            Component::RootDir => {
                result.push(std::path::MAIN_SEPARATOR.to_string());
            }
            Component::Normal(n) => {
                result.push(n);
            }
            _ => {}
        }
    }

    if result.as_os_str().is_empty() {
        if has_prefix || has_root {
            PathBuf::from(std::path::MAIN_SEPARATOR.to_string())
        } else {
            PathBuf::from(".")
        }
    } else {
        result
    }
}

/// Compara dois componentes de sistema de arquivos respeitando a sensibilidade
/// a maiúsculas/minúsculas da plataforma (Windows: case-insensitive; Linux: case-sensitive).
#[must_use]
pub fn component_equal(a: &OsStr, b: &OsStr) -> bool {
    #[cfg(windows)]
    {
        let sa = a.to_string_lossy();
        let sb = b.to_string_lossy();
        sa.to_lowercase() == sb.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

/// Compara dois componentes forçando case-insensitive (útil para testes multiplataforma no Windows/APFS).
#[must_use]
pub fn component_equal_ignore_case(a: &OsStr, b: &OsStr) -> bool {
    let sa = a.to_string_lossy();
    let sb = b.to_string_lossy();
    sa.to_lowercase() == sb.to_lowercase()
}

/// Determina se `candidate` é o próprio `target_dir` ou um descendente hierárquico
/// dentro da árvore de diretórios de `target_dir`.
///
/// Substitui comparação de prefixo textual para evitar falsos positivos
/// (ex: `C:\foo` NÃO inclui `C:\foobar`).
#[must_use]
pub fn is_in_directory_tree(candidate: &Path, target_dir: &Path) -> bool {
    let clean_cand = clean_path(candidate);
    let clean_target = clean_path(target_dir);

    let target_comps: Vec<Component> = clean_target.components().collect();
    let cand_comps: Vec<Component> = clean_cand.components().collect();

    if cand_comps.len() < target_comps.len() {
        return false;
    }

    for (t, c) in target_comps.iter().zip(cand_comps.iter()) {
        match (t, c) {
            (Component::Prefix(tp), Component::Prefix(cp)) => {
                if !component_equal(tp.as_os_str(), cp.as_os_str()) {
                    return false;
                }
            }
            (Component::RootDir, Component::RootDir) => {}
            (Component::Normal(tn), Component::Normal(cn)) => {
                if !component_equal(tn, cn) {
                    return false;
                }
            }
            _ => return false,
        }
    }

    true
}

/// Variante que permite especificar se a comparação de componentes deve ser case-sensitive ou não.
#[must_use]
pub fn is_in_directory_tree_with_case(
    candidate: &Path,
    target_dir: &Path,
    case_sensitive: bool,
) -> bool {
    let clean_cand = clean_path(candidate);
    let clean_target = clean_path(target_dir);

    let target_comps: Vec<Component> = clean_target.components().collect();
    let cand_comps: Vec<Component> = clean_cand.components().collect();

    if cand_comps.len() < target_comps.len() {
        return false;
    }

    for (t, c) in target_comps.iter().zip(cand_comps.iter()) {
        match (t, c) {
            (Component::Prefix(tp), Component::Prefix(cp)) => {
                let eq = if case_sensitive {
                    tp.as_os_str() == cp.as_os_str()
                } else {
                    component_equal_ignore_case(tp.as_os_str(), cp.as_os_str())
                };
                if !eq {
                    return false;
                }
            }
            (Component::RootDir, Component::RootDir) => {}
            (Component::Normal(tn), Component::Normal(cn)) => {
                let eq = if case_sensitive {
                    tn == cn
                } else {
                    component_equal_ignore_case(tn, cn)
                };
                if !eq {
                    return false;
                }
            }
            _ => return false,
        }
    }

    true
}

/// Verifica se um alvo de caminho está contido no volume coberto pelo daemon.
///
/// Exemplo: `C:\Projetos` pertence ao volume `"C:"`, mas `D:\Projetos` não pertence.
#[must_use]
pub fn is_target_in_volume(target: &Path, volume: &str) -> bool {
    let clean_tgt = clean_path(target);
    let clean_vol = volume.trim().trim_end_matches(['\\', '/']);

    // Extrai prefixo de drive do target, se houver (ex: 'C:')
    for comp in clean_tgt.components() {
        if let Component::Prefix(pr) = comp {
            let pr_str = pr.as_os_str().to_string_lossy();
            let drive_target = pr_str.trim_end_matches(['\\', '/']);
            return drive_target.eq_ignore_ascii_case(clean_vol)
                || drive_target
                    .trim_end_matches(':')
                    .eq_ignore_ascii_case(clean_vol.trim_end_matches(':'));
        }
    }

    // Se o target não tem prefixo de drive:
    // No Windows, caminhos relativos devem ser resolvidos antes de chegar aqui.
    // Em Linux/Unix, compara se começa com o ponto de montagem do volume.
    #[cfg(windows)]
    {
        // Se não possui prefixo no Windows e o volume é um drive como "C:",
        // só é coberto se for explicitamente relativo ou vazio (o que o cliente deve resolver).
        clean_vol.is_empty() || clean_vol == "."
    }
    #[cfg(not(windows))]
    {
        if clean_vol == "/" || clean_vol.is_empty() {
            return true;
        }
        let vol_path = Path::new(clean_vol);
        is_in_directory_tree(&clean_tgt, vol_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Convert synthetic Windows fixture paths to equivalent native absolute paths.
    fn fixture(path: &str) -> std::path::PathBuf {
        if cfg!(windows) {
            path.into()
        } else {
            format!("/{}", path.strip_prefix("C:\\").unwrap().replace('\\', "/")).into()
        }
    }

    #[test]
    fn test_directory_tree_belonging_vs_string_prefix() {
        // C:\foo vs C:\foobar - caso crítico de aceite de CORE-01
        let parent = fixture(r"C:\foo");
        let valid_child = fixture(r"C:\foo\bar.txt");
        let valid_subchild = fixture(r"C:\foo\subdir\arquivo.txt");
        let exact_match = fixture(r"C:\foo");
        let false_positive = fixture(r"C:\foobar");
        let false_positive_child = fixture(r"C:\foobar\baz.txt");

        assert!(is_in_directory_tree(&valid_child, &parent));
        assert!(is_in_directory_tree(&valid_subchild, &parent));
        assert!(is_in_directory_tree(&exact_match, &parent));

        // CRUCIAL: C:\foo NÃO deve conter C:\foobar nem C:\foobar\baz.txt
        assert!(!is_in_directory_tree(&false_positive, &parent));
        assert!(!is_in_directory_tree(&false_positive_child, &parent));
    }

    #[test]
    fn test_case_sensitivity_rules() {
        let parent = fixture(r"C:\Foo\Bar");
        let candidate = fixture(r"C:\foo\bar\sub\file.txt");

        // No Windows (case-insensitive) deve casar
        assert!(is_in_directory_tree_with_case(&candidate, &parent, false));

        // Se case-sensitive (ex: Linux), deve falhar
        assert!(!is_in_directory_tree_with_case(&candidate, &parent, true));
    }

    #[test]
    fn test_spaces_and_unicode_paths() {
        let parent = fixture(r"C:\Meus Documentos\Café Expresso");
        let child = fixture(r"C:\Meus Documentos\Café Expresso\Relatório Anual 2026.pdf");
        let different_dir = fixture(r"C:\Meus Documentos\Café Expresso e Cia\outro.txt");

        assert!(is_in_directory_tree(&child, &parent));
        assert!(!is_in_directory_tree(&different_dir, &parent));

        // Teste de Unicode com cirílico
        let parent_cyrillic = fixture(r"C:\Проекты\быстрый файл");
        let child_cyrillic = fixture(r"C:\Проекты\быстрый файл\документ.txt");
        let diff_cyrillic = fixture(r"C:\Проекты\быстрый\документ.txt");

        assert!(is_in_directory_tree(&child_cyrillic, &parent_cyrillic));
        assert!(!is_in_directory_tree(&diff_cyrillic, &parent_cyrillic));
    }

    #[cfg(windows)]
    #[test]
    fn test_mixed_separators() {
        // Barras normais e invertidas misturadas
        let p1 = Path::new("C:/foo/bar/sub");
        let p2 = Path::new(r"C:\foo\bar");
        assert!(is_in_directory_tree(p1, p2));

        let p3 = Path::new(r"C:\foo\bar\sub\arquivo.txt");
        let p4 = Path::new("C:/foo/bar");
        assert!(is_in_directory_tree(p3, p4));
    }

    #[cfg(windows)]
    #[test]
    fn test_volume_coverage() {
        let path_c = Path::new(r"C:\Projetos\fastfile");
        let path_d = Path::new(r"D:\Projetos\fastfile");

        assert!(is_target_in_volume(path_c, "C:"));
        assert!(is_target_in_volume(path_c, "c:"));
        assert!(is_target_in_volume(path_c, "C"));

        assert!(!is_target_in_volume(path_d, "C:"));
        assert!(is_target_in_volume(path_d, "D:"));
    }

    #[test]
    fn test_clean_unc_prefix() {
        assert_eq!(clean_unc_prefix(r"\\?\C:\foo\bar"), r"C:\foo\bar");
        assert_eq!(clean_unc_prefix(r"C:\foo\bar"), r"C:\foo\bar");
    }
}
