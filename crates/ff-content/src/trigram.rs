//! Índice de trigramas incremental para busca ultra-rápida em conteúdo (`fastfile grep`).
//!
//! Mapeia sequências de 3 bytes (trigramas) para listas invertidas de arquivos onde ocorrem.
//! Permite que buscas por literais ou regexes com segmentos literais >= 3 caracteres
//! descartem 99%+ dos arquivos antes de qualquer leitura ou correspondência regex,
//! tornando o grep ordens de grandeza mais rápido (estilo Zoekt/Google CodeSearch).

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, UNIX_EPOCH};

use ignore::WalkBuilder;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// Mágico binário no cabeçalho do arquivo de índice de trigramas.
const MAGIC: &[u8; 4] = b"FFTG";
/// Versão do formato de arquivo do índice.
const FORMAT_VERSION: u32 = 1;

/// Configurações do índice de trigramas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrigramConfig {
    /// Tamanho máximo de arquivo individual a indexar em bytes (padrão: 5 MB).
    pub max_file_size: u64,
    /// Limite total de bytes acumulados dos arquivos indexados (padrão: 200 MB).
    pub max_total_bytes: u64,
    /// Quantidade máxima de arquivos no índice (padrão: 50.000).
    pub max_files: usize,
    /// Respeitar regras de .gitignore e ignorar pastas ocultas.
    pub respect_ignore: bool,
}

impl Default for TrigramConfig {
    fn default() -> Self {
        Self {
            max_file_size: 5 * 1024 * 1024,     // 5 MB
            max_total_bytes: 200 * 1024 * 1024, // 200 MB
            max_files: 50_000,
            respect_ignore: true,
        }
    }
}

/// Metadados de um arquivo individual indexado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexedFile {
    /// Caminho relativo à raiz do índice.
    pub relative_path: String,
    /// Timestamp Unix de modificação (segundos).
    pub mtime_secs: u64,
    /// Tamanho do arquivo em bytes.
    pub size: u64,
}

/// Estatísticas de criação ou atualização do índice.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrigramIndexStats {
    pub root: String,
    pub files_indexed: usize,
    pub unique_trigrams: usize,
    pub total_bytes: u64,
    pub files_added: usize,
    pub files_modified: usize,
    pub files_removed: usize,
    pub elapsed_ms: f64,
}

/// Índice de trigramas em memória com suporte a persistência e atualização incremental.
#[derive(Debug, Clone)]
pub struct TrigramIndex {
    /// Diretório raiz indexado.
    pub root: PathBuf,
    /// Configurações do índice.
    pub config: TrigramConfig,
    /// Lista de arquivos indexados (ID = índice no vetor).
    pub files: Vec<IndexedFile>,
    /// Lista de trigramas únicos presentes em cada arquivo (ID = índice no vetor).
    pub file_trigrams: Vec<Vec<u32>>,
    /// Lista invertida: Trigrama (u32) -> IDs de arquivo onde ocorre (ordenado).
    pub postings: HashMap<u32, Vec<u32>>,
    /// Total de bytes somados dos arquivos indexados.
    pub total_bytes: u64,
}

/// Converte 3 bytes consecutivos em um inteiro `u32` compacto.
#[inline(always)]
pub fn bytes_to_trigram(b: &[u8]) -> u32 {
    (b[0] as u32) | ((b[1] as u32) << 8) | ((b[2] as u32) << 16)
}

/// Reconverte um inteiro `u32` em seus 3 bytes correspondentes.
#[inline(always)]
pub fn trigram_to_bytes(t: u32) -> [u8; 3] {
    [t as u8, (t >> 8) as u8, (t >> 16) as u8]
}

/// Extrai todos os trigramas únicos de um buffer de bytes de texto (case-folded em minúsculas).
/// Retorna vetor vazio se o arquivo for menor que 3 bytes ou for detectado como binário.
pub fn extract_trigrams(content: &[u8]) -> Vec<u32> {
    if content.len() < 3 {
        return Vec::new();
    }

    // Detecção rápida de binário: byte 0x00 nos primeiros 1024 bytes
    let check_len = content.len().min(1024);
    if content[..check_len].contains(&0) {
        return Vec::new();
    }

    // Converte para minúsculas ASCII mantendo integridade UTF-8
    let lower: Vec<u8> = content.iter().map(|&b| b.to_ascii_lowercase()).collect();

    let count = lower.len() - 2;
    let mut trigrams = Vec::with_capacity(count);
    for window in lower.windows(3) {
        trigrams.push(bytes_to_trigram(window));
    }

    trigrams.sort_unstable();
    trigrams.dedup();
    trigrams
}

/// Extrai sequências de trigramas obrigatórios a partir de um padrão de busca (literal ou regex).
/// Retorna `None` se a consulta não contiver segmento literal de pelo menos 3 caracteres
/// (neste caso, a filtragem por trigramas não é aplicável e deve-se varrer todos os arquivos).
pub fn extract_query_trigrams(pattern: &str) -> Option<Vec<u32>> {
    if pattern.len() < 3 {
        return None;
    }

    // Caracteres que delimitam ou formam controle regex
    let is_meta = |c: char| {
        matches!(
            c,
            '^' | '$' | '\\' | '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
        )
    };

    // Se houver alternação regex '|', a união das alternativas exigiria lógica mais complexa;
    // por segurança sem falsos negativos, retornamos None para consulta regex complexa com '|'.
    if pattern.contains('|') {
        return None;
    }

    // Encontra todos os segmentos literais contínuos >= 3 caracteres
    let mut literals = Vec::new();
    let mut current = String::new();

    for c in pattern.chars() {
        if is_meta(c) {
            if current.len() >= 3 {
                literals.push(current.clone());
            }
            current.clear();
        } else {
            current.push(c);
        }
    }
    if current.len() >= 3 {
        literals.push(current);
    }

    if literals.is_empty() {
        return None;
    }

    // Coleta trigramas de todos os segmentos literais
    let mut all_trigrams = Vec::new();
    for lit in literals {
        let tri = extract_trigrams(lit.as_bytes());
        all_trigrams.extend(tri);
    }

    if all_trigrams.is_empty() {
        None
    } else {
        all_trigrams.sort_unstable();
        all_trigrams.dedup();
        Some(all_trigrams)
    }
}

impl TrigramIndex {
    /// Cria um novo índice vazio para o diretório raiz indicado.
    pub fn new(root: impl AsRef<Path>, config: TrigramConfig) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            config,
            files: Vec::new(),
            file_trigrams: Vec::new(),
            postings: HashMap::new(),
            total_bytes: 0,
        }
    }

    /// Retorna a quantidade de arquivos indexados.
    #[inline]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Retorna verdadeiro se o índice não contém arquivos.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Reconstrói as listas invertidas (`postings`) a partir de `file_trigrams`.
    pub fn rebuild_postings(&mut self) {
        self.postings.clear();
        for (file_id, trigrams) in self.file_trigrams.iter().enumerate() {
            let fid = file_id as u32;
            for &t in trigrams {
                self.postings.entry(t).or_default().push(fid);
            }
        }
    }

    /// Localiza todos os arquivos candidatos no índice que contêm todos os trigramas fornecidos.
    /// Retorna vetor ordenado de caminhos absolutos dos arquivos candidatos.
    pub fn find_candidates(&self, query_trigrams: &[u32]) -> Vec<PathBuf> {
        if query_trigrams.is_empty() || self.files.is_empty() {
            return Vec::new();
        }

        // Coleta as listas de postagem para cada trigrama da consulta
        let mut posting_lists: Vec<&[u32]> = Vec::with_capacity(query_trigrams.len());
        for &t in query_trigrams {
            match self.postings.get(&t) {
                Some(list) if !list.is_empty() => posting_lists.push(list.as_slice()),
                _ => return Vec::new(), // Se algum trigrama não existe no índice, nenhum arquivo pode coincidir
            }
        }

        // Ordena listas pelo menor tamanho para otimizar interseção
        posting_lists.sort_by_key(|l| l.len());

        let mut candidates: Vec<u32> = posting_lists[0].to_vec();

        // Interseção sucessiva das listas
        for other in &posting_lists[1..] {
            candidates.retain(|fid| other.binary_search(fid).is_ok());
            if candidates.is_empty() {
                return Vec::new();
            }
        }

        candidates
            .into_iter()
            .filter_map(|fid| self.files.get(fid as usize))
            .map(|f| self.root.join(&f.relative_path))
            .collect()
    }

    /// Constrói ou atualiza o índice incrementalmente a partir do sistema de arquivos.
    pub fn build_or_update(&mut self) -> Result<TrigramIndexStats, io::Error> {
        let start = Instant::now();

        let mut builder = WalkBuilder::new(&self.root);
        builder.follow_links(false);
        if self.config.respect_ignore {
            builder
                .hidden(true)
                .ignore(true)
                .git_ignore(true)
                .git_exclude(true);
        } else {
            builder
                .hidden(false)
                .ignore(false)
                .git_ignore(false)
                .git_exclude(false);
        }

        // Mapeamento dos arquivos atualmente no índice: relative_path -> (index, mtime, size)
        let existing_map: HashMap<String, (usize, u64, u64)> = self
            .files
            .iter()
            .enumerate()
            .map(|(idx, f)| (f.relative_path.clone(), (idx, f.mtime_secs, f.size)))
            .collect();

        // Coleta arquivos presentes em disco
        let mut disk_files: Vec<(String, u64, u64, PathBuf)> = Vec::new();
        let mut total_disk_bytes: u64 = 0;

        for entry in builder.build().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                continue;
            }

            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let size = meta.len();
            if size > self.config.max_file_size {
                continue;
            }

            if total_disk_bytes + size > self.config.max_total_bytes {
                break;
            }

            if disk_files.len() >= self.config.max_files {
                break;
            }

            let mtime_secs = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            if let Ok(rel) = path.strip_prefix(&self.root) {
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                total_disk_bytes += size;
                disk_files.push((rel_str, mtime_secs, size, path.to_path_buf()));
            }
        }

        let mut files_added = 0usize;
        let mut files_modified = 0usize;

        // Identifica quais arquivos precisam ser lidos e tokenizados
        let mut files_to_read: Vec<(usize, PathBuf)> = Vec::new();
        let mut new_files = Vec::with_capacity(disk_files.len());
        let mut new_file_trigrams = Vec::with_capacity(disk_files.len());

        for (rel, mtime, size, full_path) in disk_files {
            if let Some(&(old_idx, old_mtime, old_size)) = existing_map.get(&rel) {
                if old_mtime == mtime && old_size == size {
                    // Arquivo inalterado: reutiliza os trigramas já extraídos
                    new_files.push(IndexedFile {
                        relative_path: rel,
                        mtime_secs: mtime,
                        size,
                    });
                    new_file_trigrams.push(self.file_trigrams[old_idx].clone());
                    continue;
                } else {
                    files_modified += 1;
                }
            } else {
                files_added += 1;
            }

            // Novo ou modificado: enfileira para leitura paralela
            let idx = new_files.len();
            new_files.push(IndexedFile {
                relative_path: rel,
                mtime_secs: mtime,
                size,
            });
            new_file_trigrams.push(Vec::new());
            files_to_read.push((idx, full_path));
        }

        // Leitura paralela dos arquivos novos/modificados usando Rayon
        let read_results: Vec<(usize, Vec<u32>)> = files_to_read
            .into_par_iter()
            .filter_map(|(idx, full_path)| {
                let content = fs::read(&full_path).ok()?;
                let trigrams = extract_trigrams(&content);
                Some((idx, trigrams))
            })
            .collect();

        for (idx, trigrams) in read_results {
            new_file_trigrams[idx] = trigrams;
        }

        let files_removed = self
            .files
            .len()
            .saturating_sub(new_files.len() - files_added);

        self.files = new_files;
        self.file_trigrams = new_file_trigrams;
        self.total_bytes = total_disk_bytes;
        self.rebuild_postings();

        let elapsed = start.elapsed();

        Ok(TrigramIndexStats {
            root: self.root.to_string_lossy().to_string(),
            files_indexed: self.files.len(),
            unique_trigrams: self.postings.len(),
            total_bytes: self.total_bytes,
            files_added,
            files_modified,
            files_removed,
            elapsed_ms: (elapsed.as_secs_f64() * 1000.0 * 100.0).round() / 100.0,
        })
    }

    /// Salva o índice em formato binário compacto no caminho especificado.
    pub fn save_to_file(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
            }
        }

        let temp_path = path.with_extension("tmp");
        let file = File::create(&temp_path)?;
        let mut writer = BufWriter::new(file);

        // Cabeçalho
        writer.write_all(MAGIC)?;
        writer.write_all(&FORMAT_VERSION.to_le_bytes())?;

        // Configuração
        writer.write_all(&self.config.max_file_size.to_le_bytes())?;
        writer.write_all(&self.config.max_total_bytes.to_le_bytes())?;
        writer.write_all(&(self.config.max_files as u64).to_le_bytes())?;
        writer.write_all(&[if self.config.respect_ignore { 1 } else { 0 }])?;

        // Total de bytes
        writer.write_all(&self.total_bytes.to_le_bytes())?;

        // Lista de arquivos e seus trigramas
        writer.write_all(&(self.files.len() as u32).to_le_bytes())?;
        for (file, trigrams) in self.files.iter().zip(&self.file_trigrams) {
            let rel_bytes = file.relative_path.as_bytes();
            writer.write_all(&(rel_bytes.len() as u32).to_le_bytes())?;
            writer.write_all(rel_bytes)?;
            writer.write_all(&file.mtime_secs.to_le_bytes())?;
            writer.write_all(&file.size.to_le_bytes())?;

            writer.write_all(&(trigrams.len() as u32).to_le_bytes())?;
            for &t in trigrams {
                writer.write_all(&t.to_le_bytes())?;
            }
        }

        writer.flush()?;
        drop(writer);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600));
        }

        fs::rename(temp_path, path)?;
        Ok(())
    }

    /// Carrega o índice a partir do formato binário persistido.
    pub fn load_from_file(root: impl AsRef<Path>, path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "arquivo de índice de trigramas com assinatura inválida",
            ));
        }

        let mut ver_buf = [0u8; 4];
        reader.read_exact(&mut ver_buf)?;
        let version = u32::from_le_bytes(ver_buf);
        if version != FORMAT_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("versão de índice de trigramas não suportada: {version}"),
            ));
        }

        let mut u64_buf = [0u8; 8];
        reader.read_exact(&mut u64_buf)?;
        let max_file_size = u64::from_le_bytes(u64_buf);
        reader.read_exact(&mut u64_buf)?;
        let max_total_bytes = u64::from_le_bytes(u64_buf);
        reader.read_exact(&mut u64_buf)?;
        let max_files = u64::from_le_bytes(u64_buf) as usize;

        let mut bool_buf = [0u8; 1];
        reader.read_exact(&mut bool_buf)?;
        let respect_ignore = bool_buf[0] != 0;

        reader.read_exact(&mut u64_buf)?;
        let total_bytes = u64::from_le_bytes(u64_buf);

        let mut u32_buf = [0u8; 4];
        reader.read_exact(&mut u32_buf)?;
        let num_files = u32::from_le_bytes(u32_buf) as usize;

        let mut files = Vec::with_capacity(num_files);
        let mut file_trigrams = Vec::with_capacity(num_files);

        for _ in 0..num_files {
            reader.read_exact(&mut u32_buf)?;
            let str_len = u32::from_le_bytes(u32_buf) as usize;
            let mut str_bytes = vec![0u8; str_len];
            reader.read_exact(&mut str_bytes)?;
            let relative_path = String::from_utf8(str_bytes)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

            reader.read_exact(&mut u64_buf)?;
            let mtime_secs = u64::from_le_bytes(u64_buf);

            reader.read_exact(&mut u64_buf)?;
            let size = u64::from_le_bytes(u64_buf);

            reader.read_exact(&mut u32_buf)?;
            let num_tri = u32::from_le_bytes(u32_buf) as usize;
            let mut trigrams = Vec::with_capacity(num_tri);
            for _ in 0..num_tri {
                reader.read_exact(&mut u32_buf)?;
                trigrams.push(u32::from_le_bytes(u32_buf));
            }

            files.push(IndexedFile {
                relative_path,
                mtime_secs,
                size,
            });
            file_trigrams.push(trigrams);
        }

        let mut index = Self {
            root: root.as_ref().to_path_buf(),
            config: TrigramConfig {
                max_file_size,
                max_total_bytes,
                max_files,
                respect_ignore,
            },
            files,
            file_trigrams,
            postings: HashMap::new(),
            total_bytes,
        };

        index.rebuild_postings();
        Ok(index)
    }
}

/// Diretório padrão onde índices de trigramas são persistidos com ACL restrita ao usuário.
pub fn default_cache_dir() -> PathBuf {
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        PathBuf::from(local_app_data)
            .join("fastfile")
            .join("trigrams")
    } else if let Ok(xdg_cache) = std::env::var("XDG_CACHE_HOME") {
        PathBuf::from(xdg_cache).join("fastfile").join("trigrams")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".cache")
            .join("fastfile")
            .join("trigrams")
    } else {
        std::env::temp_dir().join("fastfile-trigrams")
    }
}

/// Gera caminho de arquivo persistido determinístico e seguro para uma pasta raiz.
pub fn default_index_path(root: &Path) -> PathBuf {
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let path_str = canonical.to_string_lossy();

    // Hash FNV-1a simples e rápido
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in path_str.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }

    let sanitized_name: String = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("root")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();

    let file_name = format!("{sanitized_name}_{hash:016x}.idx");
    default_cache_dir().join(file_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_bytes_trigram_roundtrip() {
        let b = b"xyz";
        let tri = bytes_to_trigram(b);
        let back = trigram_to_bytes(tri);
        assert_eq!(&back, b);
    }

    #[test]
    fn test_extract_trigrams() {
        let text = b"hello world";
        let trigrams = extract_trigrams(text);
        // "hel", "ell", "llo", "lo ", "o w", " wo", "wor", "orl", "rld" = 9 trigramas únicos
        assert_eq!(trigrams.len(), 9);

        // Binário é ignorado
        let bin = b"foo\0bar";
        assert!(extract_trigrams(bin).is_empty());

        // Texto < 3 bytes
        assert!(extract_trigrams(b"ab").is_empty());
    }

    #[test]
    fn test_extract_query_trigrams() {
        let q = "SearchEngine";
        let tris = extract_query_trigrams(q).unwrap();
        assert!(!tris.is_empty());

        // Literal com regex simples
        let q_regex = "struct.*Engine";
        let tris_regex = extract_query_trigrams(q_regex).unwrap();
        // Contém trigramas de "struct" e "Engine"
        assert!(!tris_regex.is_empty());

        // Alternação regex '|' retorna None (por segurança)
        assert!(extract_query_trigrams("foo|bar").is_none());

        // Consulta curta < 3 chars retorna None
        assert!(extract_query_trigrams("fn").is_none());
    }

    #[test]
    fn test_trigram_index_build_query_persistence() {
        let tmp = std::env::temp_dir().join(format!("ff_tri_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("src")).unwrap();

        fs::write(
            tmp.join("src/lib.rs"),
            "pub struct SearchEngine { pub id: usize }\n",
        )
        .unwrap();
        fs::write(
            tmp.join("src/main.rs"),
            "fn main() { println!(\"FastFile\"); }\n",
        )
        .unwrap();
        fs::write(
            tmp.join("README.md"),
            "# FastFile project\nFastest file search tool\n",
        )
        .unwrap();

        let mut idx = TrigramIndex::new(&tmp, TrigramConfig::default());
        let stats = idx.build_or_update().unwrap();
        assert_eq!(stats.files_indexed, 3);
        assert!(stats.unique_trigrams > 10);

        // Busca por "SearchEngine"
        let q_tris = extract_query_trigrams("SearchEngine").unwrap();
        let candidates = idx.find_candidates(&q_tris);
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].ends_with("lib.rs"));

        // Busca por "FastFile"
        let q_ff = extract_query_trigrams("FastFile").unwrap();
        let candidates_ff = idx.find_candidates(&q_ff);
        assert_eq!(candidates_ff.len(), 2); // main.rs e README.md

        // Busca por termo inexistente
        let q_none = extract_query_trigrams("InexistentTerm").unwrap();
        let candidates_none = idx.find_candidates(&q_none);
        assert!(candidates_none.is_empty());

        // Teste de persistência e recarregamento
        let idx_path = tmp.join("test_index.idx");
        idx.save_to_file(&idx_path).unwrap();

        let loaded = TrigramIndex::load_from_file(&tmp, &idx_path).unwrap();
        assert_eq!(loaded.files.len(), 3);
        let loaded_candidates = loaded.find_candidates(&q_tris);
        assert_eq!(loaded_candidates.len(), 1);
        assert!(loaded_candidates[0].ends_with("lib.rs"));

        // Limpeza
        let _ = fs::remove_dir_all(&tmp);
    }
}
