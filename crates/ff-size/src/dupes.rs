//! Detecção visual de arquivos duplicados no Treemap com suporte a cancelamento,
//! identificação de hard links, preservação consciente e revalidação de integridade (TM-08).

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::hash::Hasher;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

use crate::cleanup::{analyze_cleanup_opportunity, CleanupCategory};

const PREFIX_HASH_SIZE: usize = 8192; // 8 KB
const BUFFER_SIZE: usize = 65536; // 64 KB

/// Configurações para a detecção visual de duplicados.
#[derive(Debug, Clone)]
pub struct VisualDupeOptions {
    /// Tamanho mínimo em bytes para considerar um arquivo na análise.
    pub min_size: u64,
    /// Se verdadeiro, inclui arquivos ocultos e pastas de build/junk.
    pub all: bool,
    /// Se verdadeiro, inclui arquivos de segredos (.env, chaves privadas, etc.).
    pub include_secrets: bool,
}

impl Default for VisualDupeOptions {
    fn default() -> Self {
        Self {
            min_size: 1,
            all: false,
            include_secrets: false,
        }
    }
}

/// Estado de progresso e cancelamento sob demanda para varredura de duplicados (TM-08).
#[derive(Debug, Default)]
pub struct DupeScanProgress {
    /// Quantidade total de arquivos examinados na árvore.
    pub files_scanned: AtomicU64,
    /// Quantidade de arquivos com tamanho compartilhado candidatos a hash.
    pub candidate_files: AtomicU64,
    /// Quantidade de bytes lidos durante o hashing de conteúdo.
    pub hashed_bytes: AtomicU64,
    /// Caminho do arquivo sendo processado atualmente.
    pub current_file: std::sync::Mutex<String>,
    /// Sinalizador de término da varredura.
    pub is_done: AtomicBool,
    /// Sinalizador de cancelamento solicitado pelo usuário.
    pub is_cancelled: AtomicBool,
}

impl DupeScanProgress {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_current_file(&self, file_path: &str) {
        if let Ok(mut lock) = self.current_file.lock() {
            *lock = file_path.to_string();
        }
    }

    #[must_use]
    pub fn get_current_file(&self) -> String {
        self.current_file
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }
}

/// Representação de uma cópia de arquivo dentro de um grupo de duplicados (TM-08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualDupeFile {
    /// Caminho absoluto do arquivo no disco.
    pub path: String,
    /// Nome base do arquivo com extensão.
    pub name: String,
    /// Tamanho em bytes.
    pub size: u64,
    /// Timestamp Unix de modificação em segundos.
    pub mtime: i64,
    /// Indica se o arquivo é um hard link (compartilha nós/clusters com outro arquivo).
    pub is_hardlink: bool,
    /// Identificador físico único de arquivo no sistema de arquivos (FRN/FileID no Windows, i-node no Unix).
    pub file_id: Option<u64>,
    /// Identifica se o arquivo é conteúdo pessoal do usuário (fotos, documentos, mídias).
    pub is_personal: bool,
    /// Indica se o usuário marcou esta cópia específica para preservação consciente.
    pub is_preserved: bool,
    /// Indica se o arquivo foi verificado e ainda existe fisicamente no disco.
    pub exists: bool,
}

/// Grupo de arquivos confirmados como duplicados por tamanho e conteúdo idêntico (TM-08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualDupeGroup {
    /// Identificador sequencial do grupo na listagem.
    pub id: usize,
    /// Hash de conteúdo FNV-1a de 64 bits.
    pub hash: u64,
    /// Tamanho individual de cada arquivo do grupo em bytes.
    pub size: u64,
    /// Lista de cópias pertencentes a este grupo.
    pub files: Vec<VisualDupeFile>,
    /// Desperdício de espaço nominal em bytes: `size * (cópias - 1)`.
    pub wasted_bytes: u64,
    /// Desperdício físico real recuperável no disco, deduplicando hard links (TM-03 / TM-08).
    pub reclaimable_physical_bytes: u64,
    /// Indica se algum arquivo sofreu alteração externa após a análise (invalidação em tempo real).
    pub is_stale: bool,
}

impl VisualDupeGroup {
    /// Recalcula as métricas de desperdício nominal e físico do grupo.
    pub fn recalculate_metrics(&mut self) {
        let total_files = self.files.len();
        if total_files <= 1 {
            self.wasted_bytes = 0;
            self.reclaimable_physical_bytes = 0;
            return;
        }

        self.wasted_bytes = self
            .size
            .saturating_mul((total_files as u64).saturating_sub(1));

        // Para recuperação física: hard links que compartilham o mesmo `file_id` não são
        // cópias físicas independentes; contam apenas uma vez no espaço de disco ocupado.
        let mut distinct_identities = HashSet::new();
        let mut unindexed_copies = 0u64;

        for file in &self.files {
            if let Some(fid) = file.file_id {
                distinct_identities.insert(fid);
            } else {
                unindexed_copies += 1;
            }
        }

        let distinct_physical_copies = (distinct_identities.len() as u64) + unindexed_copies;
        if distinct_physical_copies > 1 {
            self.reclaimable_physical_bytes = self
                .size
                .saturating_mul(distinct_physical_copies.saturating_sub(1));
        } else {
            // Se todas as cópias forem hard links para o mesmo nó físico, a remoção
            // de links adicionais não libera espaço em disco (liberação física = 0).
            self.reclaimable_physical_bytes = 0;
        }
    }

    /// Verifica se houve alteração externa de tamanho, modificação ou remoção em algum arquivo.
    pub fn check_stale(&mut self) -> bool {
        for file in &mut self.files {
            match std::fs::metadata(&file.path) {
                Ok(meta) => {
                    file.exists = true;
                    if meta.len() != file.size {
                        self.is_stale = true;
                        return true;
                    }
                    let mtime_sec = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    if mtime_sec != file.mtime {
                        self.is_stale = true;
                        return true;
                    }
                }
                Err(_) => {
                    file.exists = false;
                    self.is_stale = true;
                    return true;
                }
            }
        }
        self.is_stale = false;
        false
    }

    /// Revalida os arquivos do grupo relendo seus metadados e recalculando o hash de conteúdo.
    /// Retorna `true` se o grupo continuou íntegro e duplicado após a releitura.
    pub fn revalidate(&mut self) -> bool {
        let mut current_hashes = Vec::new();
        let mut all_valid = true;

        for file in &mut self.files {
            match std::fs::metadata(&file.path) {
                Ok(meta) => {
                    file.exists = true;
                    file.size = meta.len();
                    file.mtime = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    let (fid, is_hl) = query_file_identity_and_hardlink(&PathBuf::from(&file.path));
                    file.file_id = fid;
                    file.is_hardlink = is_hl;

                    match hash_file_full(&PathBuf::from(&file.path)) {
                        Ok(h) => current_hashes.push(h),
                        Err(_) => {
                            all_valid = false;
                            break;
                        }
                    }
                }
                Err(_) => {
                    file.exists = false;
                    all_valid = false;
                    break;
                }
            }
        }

        if all_valid && !current_hashes.is_empty() {
            let first_hash = current_hashes[0];
            let all_match = current_hashes.iter().all(|&h| h == first_hash);
            if all_match {
                self.hash = first_hash;
                if let Some(f) = self.files.first() {
                    self.size = f.size;
                }
                self.is_stale = false;
                self.recalculate_metrics();
                return true;
            }
        }

        self.is_stale = true;
        false
    }
}

/// Implementação rápida de FNV-1a 64-bit para streaming de hashes.
#[derive(Debug, Clone)]
pub struct Fnv1aHasher {
    state: u64,
}

impl Fnv1aHasher {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Self::OFFSET_BASIS,
        }
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.state ^= b as u64;
            self.state = self.state.wrapping_mul(Self::PRIME);
        }
    }

    #[must_use]
    pub fn finish_hash(&self) -> u64 {
        self.state
    }
}

impl Default for Fnv1aHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher for Fnv1aHasher {
    fn finish(&self) -> u64 {
        self.finish_hash()
    }

    fn write(&mut self, bytes: &[u8]) {
        self.write_bytes(bytes);
    }
}

/// Calcula o hash FNV-1a dos primeiros bytes de um arquivo (estágio de prefixo).
pub fn hash_file_prefix(path: &Path, prefix_len: usize) -> std::io::Result<u64> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0u8; prefix_len];
    let n = file.read(&mut buffer)?;
    let mut hasher = Fnv1aHasher::new();
    hasher.write_bytes(&buffer[..n]);
    Ok(hasher.finish_hash())
}

/// Calcula o hash FNV-1a completo do arquivo com suporte a cancelamento e progresso.
pub fn hash_file_full_cancellable(
    path: &Path,
    cancel_token: Option<&AtomicBool>,
    progress_bytes: Option<&AtomicU64>,
) -> std::io::Result<u64> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(BUFFER_SIZE, file);
    let mut buffer = [0u8; BUFFER_SIZE];
    let mut hasher = Fnv1aHasher::new();

    loop {
        if let Some(token) = cancel_token {
            if token.load(Ordering::Relaxed) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    "operação cancelada pelo usuário",
                ));
            }
        }

        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.write_bytes(&buffer[..n]);

        if let Some(pb) = progress_bytes {
            pb.fetch_add(n as u64, Ordering::Relaxed);
        }
    }

    Ok(hasher.finish_hash())
}

/// Calcula o hash FNV-1a completo de um arquivo.
pub fn hash_file_full(path: &Path) -> std::io::Result<u64> {
    hash_file_full_cancellable(path, None, None)
}

/// Obtém a identidade física única e indicador de hard link do arquivo no sistema de arquivos.
pub fn query_file_identity_and_hardlink(path: &Path) -> (Option<u64>, bool) {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        if let Ok(file) = File::open(path) {
            let handle = file.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
            let mut info = std::mem::MaybeUninit::<
                windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION,
            >::uninit();
            // SAFETY: `handle` pertence a `file`, aberto e vivo; `info` é um buffer local para escrita.
            let ret = unsafe {
                windows_sys::Win32::Storage::FileSystem::GetFileInformationByHandle(
                    handle,
                    info.as_mut_ptr(),
                )
            };
            if ret != 0 {
                // SAFETY: `GetFileInformationByHandle` retornou sucesso, então `info` foi inicializado.
                let info = unsafe { info.assume_init() };
                let fid = ((info.nFileIndexHigh as u64) << 32) | (info.nFileIndexLow as u64);
                let is_hl = info.nNumberOfLinks > 1;
                return (Some(fid), is_hl);
            }
        }
        (None, false)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Ok(meta) = std::fs::metadata(path) {
            (Some(meta.ino()), meta.nlink() > 1)
        } else {
            (None, false)
        }
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = path;
        (None, false)
    }
}

/// Verifica se um arquivo é considerado potencialmente sensível (segredo).
#[must_use]
pub fn is_secret_file_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    if lower == ".env" || lower.starts_with(".env.") {
        return true;
    }
    if matches!(
        lower.as_str(),
        "id_rsa"
            | "id_rsa.pub"
            | "id_ed25519"
            | "id_ed25519.pub"
            | "id_dsa"
            | "id_ecdsa"
            | ".git-credentials"
    ) {
        return true;
    }
    if lower.ends_with(".pem")
        || lower.ends_with(".key")
        || lower.ends_with(".pkcs12")
        || lower.ends_with(".pfx")
        || lower.ends_with(".p12")
        || lower.ends_with(".jwt")
        || lower.ends_with(".har")
        || lower.ends_with(".kdbx")
    {
        return true;
    }
    if lower == "credentials.json"
        || lower == "token.json"
        || (lower.starts_with("service-account") && lower.ends_with(".json"))
    {
        return true;
    }
    false
}

/// Verifica se uma pasta corresponde a artefatos de build comuns.
#[must_use]
pub fn is_build_junk_dir_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    matches!(
        lower.as_str(),
        "target"
            | "node_modules"
            | ".git"
            | ".svn"
            | ".hg"
            | "obj"
            | "bin"
            | "dist"
            | "build"
            | ".next"
            | ".nuxt"
            | ".cache"
            | "venv"
            | ".venv"
            | "__pycache__"
    )
}

/// Localiza todos os grupos de arquivos duplicados no diretório especificado
/// de maneira assíncrona/progressiva, com proteção contra cancelamento e suporte a hard links (TM-08).
pub fn find_visual_duplicates(
    target: &Path,
    options: &VisualDupeOptions,
    progress: Option<&DupeScanProgress>,
    cancel_token: Option<&AtomicBool>,
) -> Result<Vec<VisualDupeGroup>, String> {
    let canonical_root = target
        .canonicalize()
        .map_err(|e| format!("não foi possível acessar alvo '{}': {e}", target.display()))?;

    let mut builder = WalkBuilder::new(&canonical_root);
    builder
        .standard_filters(!options.all)
        .hidden(!options.all)
        .parents(!options.all);

    let walker = builder.build();
    let mut files_by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();

    for result in walker {
        if let Some(token) = cancel_token {
            if token.load(Ordering::Relaxed) {
                if let Some(p) = progress {
                    p.is_cancelled.store(true, Ordering::Relaxed);
                }
                return Err("Varredura de duplicados cancelada".to_string());
            }
        }

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
                    .any(|c| is_build_junk_dir_name(&c.as_os_str().to_string_lossy()));
                if has_junk {
                    continue;
                }
            }
        }

        if !options.include_secrets {
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if is_secret_file_name(fname) {
                continue;
            }
        }

        if let Some(ft) = entry.file_type() {
            if ft.is_file() {
                if let Some(p) = progress {
                    p.files_scanned.fetch_add(1, Ordering::Relaxed);
                    p.set_current_file(&path.to_string_lossy());
                }
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
    // (Garantia do critério de aceite: mesmo nome sem mesmo conteúdo ou mesmo tamanho nunca é agrupado)
    let size_candidates: Vec<(u64, Vec<PathBuf>)> = files_by_size
        .into_iter()
        .filter(|(_, paths)| paths.len() >= 2)
        .collect();

    let total_candidates: u64 = size_candidates.iter().map(|(_, p)| p.len() as u64).sum();
    if let Some(p) = progress {
        p.candidate_files.store(total_candidates, Ordering::Relaxed);
    }

    // Estágio 1: Hash parcial de prefixo (8 KB)
    let mut partial_hash_groups: HashMap<(u64, u64), Vec<PathBuf>> = HashMap::new();
    for (size, paths) in size_candidates {
        for path in paths {
            if let Some(token) = cancel_token {
                if token.load(Ordering::Relaxed) {
                    if let Some(p) = progress {
                        p.is_cancelled.store(true, Ordering::Relaxed);
                    }
                    return Err("Varredura de duplicados cancelada".to_string());
                }
            }

            if let Some(p) = progress {
                p.set_current_file(&path.to_string_lossy());
            }

            if let Ok(hash) = hash_file_prefix(&path, PREFIX_HASH_SIZE) {
                partial_hash_groups
                    .entry((size, hash))
                    .or_default()
                    .push(path);
            }
        }
    }

    // Estágio 2: Hash completo para candidatos que ainda tenham 2+ arquivos
    let mut confirmed_groups: HashMap<(u64, u64), Vec<PathBuf>> = HashMap::new();
    let hashed_bytes_atomic = progress.map(|p| &p.hashed_bytes);

    for ((size, partial_hash), paths) in partial_hash_groups {
        if paths.len() < 2 {
            continue;
        }

        if let Some(token) = cancel_token {
            if token.load(Ordering::Relaxed) {
                if let Some(p) = progress {
                    p.is_cancelled.store(true, Ordering::Relaxed);
                }
                return Err("Varredura de duplicados cancelada".to_string());
            }
        }

        // Se o arquivo <= 8 KB, o hash de prefixo já cobriu todo o arquivo
        if size <= PREFIX_HASH_SIZE as u64 {
            confirmed_groups.insert((size, partial_hash), paths);
            continue;
        }

        let mut full_hash_map: HashMap<u64, Vec<PathBuf>> = HashMap::new();
        for path in paths {
            if let Some(p) = progress {
                p.set_current_file(&path.to_string_lossy());
            }

            if let Ok(full_hash) =
                hash_file_full_cancellable(&path, cancel_token, hashed_bytes_atomic)
            {
                full_hash_map.entry(full_hash).or_default().push(path);
            }
        }

        for (full_hash, full_paths) in full_hash_map {
            if full_paths.len() >= 2 {
                confirmed_groups.insert((size, full_hash), full_paths);
            }
        }
    }

    // Estágio 3: Construção dos grupos visuais com diferenciação de hard links e conteúdo pessoal
    let mut visual_groups = Vec::new();

    for (next_group_id, ((size, hash), paths)) in (1usize..).zip(confirmed_groups) {
        let mut visual_files = Vec::new();

        for p in paths {
            let path_str = p.to_string_lossy().to_string();
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            let (mtime, is_hardlink, file_id) = match std::fs::metadata(&p) {
                Ok(meta) => {
                    let mtime_sec = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    let (fid, hl) = query_file_identity_and_hardlink(&p);
                    (mtime_sec, hl, fid)
                }
                Err(_) => (0, false, None),
            };

            // Critério de aceite: A aplicação não escolhe automaticamente arquivos pessoais para remoção.
            // Reconhece conteúdo pessoal através do motor de regras de oportunidade de limpeza (TM-07).
            let advice = analyze_cleanup_opportunity(&path_str, &name, false, None);
            let is_personal = advice.category == CleanupCategory::PersonalContent;

            visual_files.push(VisualDupeFile {
                path: path_str,
                name,
                size,
                mtime,
                is_hardlink,
                file_id,
                is_personal,
                is_preserved: false, // Usuário deve ter controle consciente; nenhuma exclusão automática
                exists: true,
            });
        }

        let mut group = VisualDupeGroup {
            id: next_group_id,
            hash,
            size,
            files: visual_files,
            wasted_bytes: 0,
            reclaimable_physical_bytes: 0,
            is_stale: false,
        };

        group.recalculate_metrics();
        visual_groups.push(group);
    }

    // Ordenar grupos por espaço físico recuperável decrescente (e depois tamanho)
    visual_groups.sort_by(|a, b| {
        b.reclaimable_physical_bytes
            .cmp(&a.reclaimable_physical_bytes)
            .then_with(|| b.wasted_bytes.cmp(&a.wasted_bytes))
            .then_with(|| b.size.cmp(&a.size))
    });

    if let Some(p) = progress {
        p.is_done.store(true, Ordering::Relaxed);
    }

    Ok(visual_groups)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_name_different_content_does_not_form_duplicate_group() {
        // Critério de aceite: Mesmo nome não é tratado como duplicação confirmada
        let temp_dir = tempfile::tempdir().unwrap();
        let sub1 = temp_dir.path().join("sub1");
        let sub2 = temp_dir.path().join("sub2");
        std::fs::create_dir_all(&sub1).unwrap();
        std::fs::create_dir_all(&sub2).unwrap();

        let file1 = sub1.join("report.txt");
        let file2 = sub2.join("report.txt");

        std::fs::write(&file1, "Relatorio financeiro de 2025").unwrap();
        std::fs::write(&file2, "Relatorio juridico de 2026 com conteudo diferente").unwrap();

        let options = VisualDupeOptions::default();
        let dupes = find_visual_duplicates(temp_dir.path(), &options, None, None).unwrap();

        assert!(
            dupes.is_empty(),
            "Arquivos com mesmo nome e conteúdos distintos não devem formar grupo"
        );
    }

    #[test]
    fn test_same_size_different_content_does_not_form_duplicate_group() {
        // Critério de aceite: Mesmo tamanho com conteúdo diferente não forma grupo de duplicados
        let temp_dir = tempfile::tempdir().unwrap();
        let file1 = temp_dir.path().join("file_a.bin");
        let file2 = temp_dir.path().join("file_b.bin");

        let content_a = vec![0xAA; 16384];
        let content_b = vec![0xBB; 16384];

        std::fs::write(&file1, &content_a).unwrap();
        std::fs::write(&file2, &content_b).unwrap();

        let options = VisualDupeOptions::default();
        let dupes = find_visual_duplicates(temp_dir.path(), &options, None, None).unwrap();

        assert!(
            dupes.is_empty(),
            "Arquivos com mesmo tamanho mas conteúdo diferente não devem ser duplicados"
        );
    }

    #[test]
    fn test_identical_content_forms_duplicate_group() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file1 = temp_dir.path().join("copy1.txt");
        let file2 = temp_dir.path().join("copy2.txt");

        let content = b"Conteudo exatamente identico para teste de deduplicacao.";
        std::fs::write(&file1, content).unwrap();
        std::fs::write(&file2, content).unwrap();

        let options = VisualDupeOptions::default();
        let dupes = find_visual_duplicates(temp_dir.path(), &options, None, None).unwrap();

        assert_eq!(dupes.len(), 1);
        assert_eq!(dupes[0].files.len(), 2);
        assert_eq!(dupes[0].size, content.len() as u64);
        assert_eq!(dupes[0].wasted_bytes, content.len() as u64);
    }

    #[test]
    fn test_hardlinks_distinguished_and_physical_reclaim_deduplicated() {
        // Critério de aceite: Hard links são diferenciados de cópias independentes
        let mut group = VisualDupeGroup {
            id: 1,
            hash: 12345,
            size: 1000,
            files: vec![
                VisualDupeFile {
                    path: "C:\\a.dat".to_string(),
                    name: "a.dat".to_string(),
                    size: 1000,
                    mtime: 100,
                    is_hardlink: true,
                    file_id: Some(999), // Mesmo inode/FRN
                    is_personal: false,
                    is_preserved: false,
                    exists: true,
                },
                VisualDupeFile {
                    path: "C:\\b_hl.dat".to_string(),
                    name: "b_hl.dat".to_string(),
                    size: 1000,
                    mtime: 100,
                    is_hardlink: true,
                    file_id: Some(999), // Mesmo inode/FRN
                    is_personal: false,
                    is_preserved: false,
                    exists: true,
                },
            ],
            wasted_bytes: 0,
            reclaimable_physical_bytes: 0,
            is_stale: false,
        };

        group.recalculate_metrics();

        // Desperdício nominal: 1000 bytes
        assert_eq!(group.wasted_bytes, 1000);
        // Recuperação física: 0 bytes! Ambas as cópias apontam para os mesmos blocos em disco
        assert_eq!(group.reclaimable_physical_bytes, 0);

        // Se adicionarmos uma terceira cópia independente (file_id diferente):
        group.files.push(VisualDupeFile {
            path: "C:\\c_independent.dat".to_string(),
            name: "c_independent.dat".to_string(),
            size: 1000,
            mtime: 100,
            is_hardlink: false,
            file_id: Some(1001), // Inode independente
            is_personal: false,
            is_preserved: false,
            exists: true,
        });

        group.recalculate_metrics();
        // Desperdício nominal: 2 * 1000 = 2000 bytes
        assert_eq!(group.wasted_bytes, 2000);
        // Recuperação física: temos 2 instâncias físicas (999 e 1001). Apenas 1 pode ser liberada = 1000 bytes
        assert_eq!(group.reclaimable_physical_bytes, 1000);
    }

    #[test]
    fn test_personal_content_not_automatically_selected_for_removal() {
        // Critério de aceite: A aplicação não escolhe automaticamente arquivos pessoais para remoção
        let temp_dir = tempfile::tempdir().unwrap();
        let file1 = temp_dir.path().join("photo1.jpg");
        let file2 = temp_dir.path().join("photo2.jpg");

        let content = vec![0x12; 4096];
        std::fs::write(&file1, &content).unwrap();
        std::fs::write(&file2, &content).unwrap();

        let options = VisualDupeOptions::default();
        let dupes = find_visual_duplicates(temp_dir.path(), &options, None, None).unwrap();

        assert_eq!(dupes.len(), 1);
        for file in &dupes[0].files {
            assert!(
                file.is_personal,
                "Fotos pessoais devem ser reconhecidas como conteúdo pessoal"
            );
            assert!(
                !file.is_preserved,
                "Não deve haver pré-seleção ou marcação automática que induza exclusão"
            );
        }
    }

    #[test]
    fn test_external_modifications_invalidate_and_revalidate_conclusion() {
        // Critério de aceite: Alterações ocorridas durante a análise invalidam ou revalidam a conclusão
        let temp_dir = tempfile::tempdir().unwrap();
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");

        std::fs::write(&file1, "Texto duplicado inicial").unwrap();
        std::fs::write(&file2, "Texto duplicado inicial").unwrap();

        let options = VisualDupeOptions::default();
        let mut dupes = find_visual_duplicates(temp_dir.path(), &options, None, None).unwrap();

        assert_eq!(dupes.len(), 1);
        let group = &mut dupes[0];
        assert!(!group.is_stale);

        // Modifica externamente file2
        std::fs::write(&file2, "Texto modificado externamente que quebra o hash").unwrap();

        // Detecção de stale
        let is_stale = group.check_stale();
        assert!(is_stale);
        assert!(group.is_stale);

        // Revalidação falha porque conteúdos não coincidem mais
        let revalidated = group.revalidate();
        assert!(!revalidated);
        assert!(group.is_stale);

        // Corrige file2 de volta para coincidir com file1
        std::fs::write(&file2, "Texto duplicado inicial").unwrap();
        let revalidated_ok = group.revalidate();
        assert!(revalidated_ok);
        assert!(!group.is_stale);
    }
}
