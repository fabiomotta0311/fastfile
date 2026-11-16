//! Detector de arquivos duplicados por tamanho e hash (§3.5 e #28).

use std::collections::HashMap;
use std::fs::File;
use std::hash::Hasher;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::map::is_build_junk_dir;
use crate::schema::{DupeGroup, DupesResult};
use crate::secrets::is_secret_file;
use crate::util::{format_bytes, make_relative_path};

const PREFIX_HASH_SIZE: usize = 8192; // 8 KB
const BUFFER_SIZE: usize = 65536; // 64 KB

/// Configurações para a detecção de duplicados.
#[derive(Debug, Clone)]
pub struct DupesOptions {
    pub target: PathBuf,
    pub min_size: u64,
    pub limit: usize,
    pub relative: bool,
    pub all: bool,
    pub include_secrets: bool,
}

impl Default for DupesOptions {
    fn default() -> Self {
        Self {
            target: PathBuf::from("."),
            min_size: 1,
            limit: 50,
            relative: true,
            all: false,
            include_secrets: false,
        }
    }
}

/// Implementação rápida de FNV-1a 64-bit para streaming de hashes.
#[derive(Debug, Clone)]
struct Fnv1aHasher {
    state: u64,
}

impl Fnv1aHasher {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    fn new() -> Self {
        Self {
            state: Self::OFFSET_BASIS,
        }
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.state ^= b as u64;
            self.state = self.state.wrapping_mul(Self::PRIME);
        }
    }

    fn finish(&self) -> u64 {
        self.state
    }
}

impl Hasher for Fnv1aHasher {
    fn finish(&self) -> u64 {
        self.state
    }

    fn write(&mut self, bytes: &[u8]) {
        self.write_bytes(bytes);
    }
}

/// Calcula o hash dos primeiros bytes de um arquivo.
pub fn hash_file_prefix(path: &Path, prefix_len: usize) -> std::io::Result<u64> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0u8; prefix_len];
    let n = file.read(&mut buffer)?;
    let mut hasher = Fnv1aHasher::new();
    hasher.write_bytes(&buffer[..n]);
    Ok(hasher.finish())
}

/// Calcula o hash completo do arquivo de forma eficiente por blocos.
pub fn hash_file_full(path: &Path) -> std::io::Result<u64> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(BUFFER_SIZE, file);
    let mut buffer = [0u8; BUFFER_SIZE];
    let mut hasher = Fnv1aHasher::new();

    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.write_bytes(&buffer[..n]);
    }

    Ok(hasher.finish())
}

/// Localiza todos os grupos de arquivos duplicados no diretório especificado.
pub fn find_duplicates(options: &DupesOptions) -> Result<DupesResult, String> {
    let canonical_root = options.target.canonicalize().map_err(|e| {
        format!(
            "não foi possível acessar alvo '{}': {e}",
            options.target.display()
        )
    })?;

    let mut builder = WalkBuilder::new(&canonical_root);
    builder
        .standard_filters(!options.all)
        .hidden(!options.all)
        .parents(!options.all);

    let walker = builder.build();

    let mut total_scanned = 0usize;
    let mut files_by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();

    for result in walker {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        if path == canonical_root {
            continue;
        }

        if !options.all {
            if let Ok(rel) = path.strip_prefix(&canonical_root) {
                let has_junk = rel
                    .components()
                    .any(|c| is_build_junk_dir(&c.as_os_str().to_string_lossy()));
                if has_junk {
                    continue;
                }
            }
        }

        if !options.include_secrets {
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if is_secret_file(fname) {
                continue;
            }
        }

        if let Some(ft) = entry.file_type() {
            if ft.is_file() {
                total_scanned += 1;
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                if size >= options.min_size {
                    files_by_size
                        .entry(size)
                        .or_default()
                        .push(path.to_path_buf());
                }
            }
        }
    }

    // Filtrar apenas tamanhos com 2 ou mais arquivos candidatos
    // (Garantia do critério de aceite: não lê arquivos sem par de tamanho!)
    let mut candidate_files_count = 0usize;
    let size_candidates: Vec<(u64, Vec<PathBuf>)> = files_by_size
        .into_iter()
        .filter(|(_, paths)| {
            if paths.len() >= 2 {
                candidate_files_count += paths.len();
                true
            } else {
                false
            }
        })
        .collect();

    // Estágio 1: Hash parcial (primeiros 8 KB)
    let mut partial_hash_groups: HashMap<(u64, u64), Vec<PathBuf>> = HashMap::new();
    for (size, paths) in size_candidates {
        for path in paths {
            if let Ok(hash) = hash_file_prefix(&path, PREFIX_HASH_SIZE) {
                partial_hash_groups
                    .entry((size, hash))
                    .or_default()
                    .push(path);
            }
        }
    }

    // Estágio 2: Hash completo apenas para grupos que ainda têm 2+ arquivos
    let mut confirmed_groups: HashMap<(u64, u64), Vec<PathBuf>> = HashMap::new();

    for ((size, partial_hash), paths) in partial_hash_groups {
        if paths.len() < 2 {
            continue;
        }

        // Se arquivo <= 8 KB, o hash parcial já cobriu o arquivo inteiro
        if size <= PREFIX_HASH_SIZE as u64 {
            confirmed_groups.insert((size, partial_hash), paths);
            continue;
        }

        // Caso contrário, calcula o hash completo nos candidatos
        let mut full_hash_map: HashMap<u64, Vec<PathBuf>> = HashMap::new();
        for path in paths {
            if let Ok(full_hash) = hash_file_full(&path) {
                full_hash_map.entry(full_hash).or_default().push(path);
            }
        }

        for (full_hash, full_paths) in full_hash_map {
            if full_paths.len() >= 2 {
                confirmed_groups.insert((size, full_hash), full_paths);
            }
        }
    }

    // Estágio 3: Consolidação e ordenação por espaço desperdiçado
    let mut dupe_groups = Vec::new();
    let mut total_wasted_bytes = 0u64;
    let mut total_duplicate_files = 0usize;

    let target_str = options.target.to_string_lossy().to_string();

    for ((size, hash), paths) in confirmed_groups {
        let count = paths.len();
        let wasted = size * (count as u64 - 1);
        total_wasted_bytes += wasted;
        total_duplicate_files += count;

        let formatted_paths: Vec<String> = paths
            .into_iter()
            .map(|p| {
                let path_str = p.to_string_lossy().to_string();
                if options.relative {
                    make_relative_path(&path_str, &target_str)
                } else {
                    path_str
                }
            })
            .collect();

        dupe_groups.push(DupeGroup {
            size,
            hash: format!("{hash:016x}"),
            count,
            wasted_bytes: wasted,
            files: formatted_paths,
        });
    }

    // Ordenar do maior desperdício para o menor
    dupe_groups.sort_by_key(|g| std::cmp::Reverse(g.wasted_bytes));

    let total_groups = dupe_groups.len();

    Ok(DupesResult {
        target: target_str,
        total_scanned_files: total_scanned,
        candidate_files: candidate_files_count,
        duplicate_groups: total_groups,
        duplicate_files: total_duplicate_files,
        wasted_bytes: total_wasted_bytes,
        groups: dupe_groups,
    })
}

/// Formata o resultado para exibição humana no terminal.
#[must_use]
pub fn format_dupes_human(res: &DupesResult, truncated: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!("FastFile Duplicates Report: {}\n", res.target));
    out.push_str(&format!(
        "Total de arquivos varridos: {} | Candidatos por tamanho: {}\n",
        res.total_scanned_files, res.candidate_files
    ));
    out.push_str(&format!(
        "Duplicados encontrados: {} arquivos em {} grupos\n",
        res.duplicate_files, res.duplicate_groups
    ));
    out.push_str(&format!(
        "Espaço desperdiçado recuperável: {}\n\n",
        format_bytes(res.wasted_bytes)
    ));

    if res.groups.is_empty() {
        out.push_str("Nenhum arquivo duplicado encontrado.\n");
        return out;
    }

    out.push_str("Maiores grupos de duplicados:\n");
    for (i, g) in res.groups.iter().enumerate() {
        out.push_str(&format!(
            "  [{}] Tamanho: {} ({} cópias) | Desperdício: {} | Hash: {}\n",
            i + 1,
            format_bytes(g.size),
            g.count,
            format_bytes(g.wasted_bytes),
            &g.hash[..8]
        ));
        for file in &g.files {
            out.push_str(&format!("      ↳ {file}\n"));
        }
        out.push('\n');
    }

    if truncated {
        out.push_str("⚠️ Saída compactada para respeitar o limite (--budget ou --limit).\n");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_fnv1a_hasher() {
        let mut h1 = Fnv1aHasher::new();
        h1.write_bytes(b"hello world");
        let hash1 = h1.finish();

        let mut h2 = Fnv1aHasher::new();
        h2.write_bytes(b"hello world");
        let hash2 = h2.finish();

        assert_eq!(hash1, hash2);

        let mut h3 = Fnv1aHasher::new();
        h3.write_bytes(b"different text");
        assert_ne!(hash1, h3.finish());
    }

    #[test]
    fn test_find_synthetic_duplicates() {
        let temp_dir = std::env::temp_dir().join(format!("ff_test_dupes_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).unwrap();

        // Criar arquivos idênticos A1 e A2 (100 bytes)
        let content_a = vec![0x42u8; 100];
        fs::write(temp_dir.join("a1.bin"), &content_a).unwrap();
        fs::write(temp_dir.join("a2.bin"), &content_a).unwrap();

        // Criar arquivo único B (100 bytes mas conteúdo diferente)
        let content_b = vec![0x99u8; 100];
        fs::write(temp_dir.join("b_diff.bin"), &content_b).unwrap();

        // Criar arquivo único C (50 bytes - tamanho único, nunca deve ter hash calculado)
        let content_c = vec![0x11u8; 50];
        fs::write(temp_dir.join("c_unique.bin"), &content_c).unwrap();

        let options = DupesOptions {
            target: temp_dir.clone(),
            min_size: 1,
            limit: 10,
            relative: true,
            all: true,
            include_secrets: true,
        };

        let res = find_duplicates(&options).unwrap();
        assert_eq!(res.duplicate_groups, 1);
        assert_eq!(res.duplicate_files, 2);
        assert_eq!(res.wasted_bytes, 100);

        let group = &res.groups[0];
        assert_eq!(group.size, 100);
        assert_eq!(group.count, 2);
        assert!(group.files.iter().any(|f| f.ends_with("a1.bin")));
        assert!(group.files.iter().any(|f| f.ends_with("a2.bin")));
        assert!(!group.files.iter().any(|f| f.ends_with("b_diff.bin")));
        assert!(!group.files.iter().any(|f| f.ends_with("c_unique.bin")));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
