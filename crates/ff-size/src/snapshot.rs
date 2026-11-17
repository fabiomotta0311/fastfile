//! Módulo de snapshots locais de ocupação e comparação de crescimento (TM-11).
//!
//! Permite capturar snapshots pontuais de ocupação em disco, comparar análises
//! temporais do mesmo escopo, identificar pastas que cresceram, itens novos e
//! removidos, além de manter o controle local de retenção e exclusão do histórico.

use crate::treemap::{TreemapLayout, TreemapSizeMode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Item registrado em um snapshot de ocupação em disco (TM-11).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotEntry {
    /// Caminho relativo ao root_path do snapshot (normalizado com barras '/')
    pub relative_path: String,
    /// Nome do arquivo ou diretório
    pub name: String,
    /// Indica se o item é diretório
    pub is_dir: bool,
    /// Tamanho lógico em bytes
    pub logical_size: u64,
    /// Espaço alocado em bytes (se disponível)
    pub allocated_size: Option<u64>,
    /// Estimativa de recuperação em bytes (se disponível)
    pub reclaimable_size: Option<u64>,
    /// Timestamp Unix da última modificação (mtime)
    pub mtime: i64,
}

/// Snapshot pontual de ocupação em disco para um determinado escopo/diretório (TM-11).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiskUsageSnapshot {
    /// Identificador único do snapshot (ex: 'snap_1700000000_1234')
    pub id: String,
    /// Rótulo ou descrição opcional do usuário
    pub label: String,
    /// Caminho raiz analisado (escopo)
    pub root_path: String,
    /// Timestamp Unix da criação do snapshot
    pub timestamp: i64,
    /// Modo de medição utilizado para os totais nominais
    pub size_mode: TreemapSizeMode,
    /// Se a varredura foi completa ou parcial/cancelada
    pub is_complete: bool,
    /// Caminhos que geraram erro de acesso/permissão durante a varredura
    pub access_errors: Vec<String>,
    /// Total de bytes analisados no modo ativo
    pub total_bytes: u64,
    /// Total de bytes lógicos
    pub total_logical_bytes: u64,
    /// Total de bytes alocados (se disponível)
    pub total_allocated_bytes: Option<u64>,
    /// Total de bytes liberáveis estimados (se disponível)
    pub total_reclaimable_bytes: Option<u64>,
    /// Quantidade de arquivos
    pub total_files: usize,
    /// Quantidade de diretórios
    pub total_dirs: usize,
    /// Entradas individuais mapeadas pelo relative_path normalizado
    pub entries: HashMap<String, SnapshotEntry>,
}

impl DiskUsageSnapshot {
    /// Normaliza caminhos para chaves de busca consistentes entre plataformas (com barras '/').
    #[must_use]
    pub fn normalize_rel_path(path: &str) -> String {
        let norm = path.replace('\\', "/");
        norm.trim_matches('/').to_lowercase()
    }

    /// Cria um snapshot a partir do layout calculado do Treemap e metadados de varredura.
    #[must_use]
    pub fn from_treemap_layout(
        layout: &TreemapLayout,
        root_path: &str,
        is_complete: bool,
        access_errors: Vec<String>,
        label: Option<String>,
    ) -> Self {
        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let id = format!("snap_{}_{:06}", now_unix, rand_simple_suffix(now_unix));
        let root_clean = root_path.replace('\\', "/");
        let root_trimmed = root_clean.trim_end_matches('/');

        let mut entries = HashMap::with_capacity(layout.all_items.len());
        let mut total_files = 0usize;
        let mut total_dirs = 0usize;

        for item in &layout.all_items {
            let item_path_clean = item.path.replace('\\', "/");
            let rel = if let Some(stripped) = item_path_clean.strip_prefix(root_trimmed) {
                stripped.trim_start_matches('/').to_string()
            } else {
                item.name.clone()
            };

            let rel_key = Self::normalize_rel_path(&rel);

            if item.is_dir {
                total_dirs += 1;
            } else {
                total_files += 1;
            }

            entries.insert(
                rel_key.clone(),
                SnapshotEntry {
                    relative_path: rel,
                    name: item.name.clone(),
                    is_dir: item.is_dir,
                    logical_size: item.logical_size,
                    allocated_size: item.allocated_size,
                    reclaimable_size: item.reclaimable_size,
                    mtime: item.mtime,
                },
            );
        }

        let snap_label = label.unwrap_or_else(|| {
            format!(
                "Análise de {} ({})",
                layout.root_name,
                format_snapshot_time(now_unix)
            )
        });

        Self {
            id,
            label: snap_label,
            root_path: root_path.to_string(),
            timestamp: now_unix,
            size_mode: layout.size_mode,
            is_complete,
            access_errors,
            total_bytes: layout.total_size,
            total_logical_bytes: layout.total_logical_size,
            total_allocated_bytes: layout.total_allocated_size,
            total_reclaimable_bytes: layout.total_reclaimable_size,
            total_files,
            total_dirs,
            entries,
        }
    }

    /// Retorna um resumo leve do snapshot sem carregar todo o mapa de entradas.
    #[must_use]
    pub fn summary(&self) -> DiskUsageSnapshotSummary {
        DiskUsageSnapshotSummary {
            id: self.id.clone(),
            label: self.label.clone(),
            root_path: self.root_path.clone(),
            timestamp: self.timestamp,
            size_mode: self.size_mode,
            total_bytes: self.total_bytes,
            total_files: self.total_files,
            total_dirs: self.total_dirs,
            is_complete: self.is_complete,
            access_errors_count: self.access_errors.len(),
        }
    }
}

/// Resumo leve de um snapshot para exibição em listas e seletores na UI (TM-11).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiskUsageSnapshotSummary {
    pub id: String,
    pub label: String,
    pub root_path: String,
    pub timestamp: i64,
    pub size_mode: TreemapSizeMode,
    pub total_bytes: u64,
    pub total_files: usize,
    pub total_dirs: usize,
    pub is_complete: bool,
    pub access_errors_count: usize,
}

/// Tipo de variação observada em um item entre dois snapshots (TM-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrowthChangeKind {
    /// Pasta ou arquivo cresceu em tamanho
    Grown,
    /// Pasta ou arquivo diminuiu em tamanho
    Shrunk,
    /// Item novo adicionado
    Added,
    /// Item removido do disco
    Removed,
    /// Tamanho inalterado
    Unchanged,
    /// Item que não pôde ser acessado devido a erro de permissão (não conta como removido!)
    InaccessibleDueToPermission,
}

impl GrowthChangeKind {
    #[must_use]
    pub fn label_pt(&self) -> &'static str {
        match self {
            Self::Grown => "▲ Cresceu",
            Self::Shrunk => "▼ Reduziu",
            Self::Added => "＋ Novo",
            Self::Removed => "− Removido",
            Self::Unchanged => "＝ Inalterado",
            Self::InaccessibleDueToPermission => "⚠️ Sem Acesso (Permissão)",
        }
    }

    #[must_use]
    pub fn label_en(&self) -> &'static str {
        match self {
            Self::Grown => "▲ Grown",
            Self::Shrunk => "▼ Shrunk",
            Self::Added => "＋ Added",
            Self::Removed => "− Removed",
            Self::Unchanged => "＝ Unchanged",
            Self::InaccessibleDueToPermission => "⚠️ Inaccessible (Permission)",
        }
    }

    #[must_use]
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Grown => "📈",
            Self::Shrunk => "📉",
            Self::Added => "✨",
            Self::Removed => "🗑️",
            Self::Unchanged => "⏸️",
            Self::InaccessibleDueToPermission => "🔒",
        }
    }
}

/// Item ranqueado na comparação de crescimento (TM-11).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrowthItem {
    pub relative_path: String,
    pub name: String,
    pub is_dir: bool,
    pub kind: GrowthChangeKind,
    pub base_size: Option<u64>,
    pub current_size: Option<u64>,
    /// Delta de bytes: positivo = crescimento, negativo = redução
    pub delta_bytes: i64,
    pub delta_percentage: Option<f64>,
    pub mtime_base: Option<i64>,
    pub mtime_current: Option<i64>,
    pub had_access_error: bool,
}

/// Relatório consolidado de comparação entre dois snapshots (TM-11).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotComparisonReport {
    pub base_snapshot_id: String,
    pub base_label: String,
    pub base_timestamp: i64,
    pub current_snapshot_id: String,
    pub current_label: String,
    pub current_timestamp: i64,
    pub root_path: String,
    /// Avisos de cobertura, escopo ou compatibilidade de medidas
    pub warnings: Vec<String>,
    /// Modo de medição utilizado na comparação
    pub size_mode: TreemapSizeMode,
    /// Indica se as medidas de origem eram 100% compatíveis
    pub measures_compatible: bool,
    /// Indica se ambas as análises tiveram cobertura completa
    pub full_coverage: bool,

    // Totais agregados
    pub total_base_bytes: u64,
    pub total_current_bytes: u64,
    pub total_delta_bytes: i64,
    pub total_delta_percentage: Option<f64>,

    // Contadores de itens
    pub items_grown: usize,
    pub items_shrunk: usize,
    pub items_added: usize,
    pub items_removed: usize,
    pub items_inaccessible: usize,

    // Ranquings de destaque para a interface
    pub top_growing_folders: Vec<GrowthItem>,
    pub top_added_items: Vec<GrowthItem>,
    pub top_removed_items: Vec<GrowthItem>,
    pub all_changes: Vec<GrowthItem>,
}

/// Compara dois snapshots e gera o relatório completo de crescimento e variações (TM-11).
#[must_use]
pub fn compare_snapshots(
    base: &DiskUsageSnapshot,
    current: &DiskUsageSnapshot,
) -> SnapshotComparisonReport {
    let mut warnings = Vec::new();

    // 1. Verificação de Escopo
    let norm_base_root = DiskUsageSnapshot::normalize_rel_path(&base.root_path);
    let norm_curr_root = DiskUsageSnapshot::normalize_rel_path(&current.root_path);
    if norm_base_root != norm_curr_root {
        warnings.push(format!(
            "⚠️ Aviso de Escopo: Os snapshots possuem raízes diferentes ('{}' vs '{}'). Os resultados comparam estruturas relativas distintas.",
            base.root_path, current.root_path
        ));
    }

    // 2. Verificação de Cobertura Incompleta (Critério de Aceite)
    let full_coverage = base.is_complete
        && current.is_complete
        && base.access_errors.is_empty()
        && current.access_errors.is_empty();

    if !base.is_complete || !current.is_complete {
        warnings.push(
            "⚠️ Aviso de Cobertura Incompleta: Uma ou ambas as análises foram parciais ou canceladas antes do término. Variações calculadas podem refletir interrupção da varredura."
                .to_string(),
        );
    }

    if !current.access_errors.is_empty() {
        warnings.push(format!(
            "⚠️ Aviso de Permissão: A análise atual registrou {} pasta(s) sem permissão de acesso. Elas foram protegidas e NÃO serão interpretadas como espaço liberado.",
            current.access_errors.len()
        ));
    }

    // 3. Verificação de Compatibilidade de Medidas (Critério de Aceite)
    let measures_compatible = base.size_mode == current.size_mode;
    if !measures_compatible {
        warnings.push(format!(
            "⚠️ Incompatibilidade de Medida: O snapshot base utilizou '{:?}' e o atual utilizou '{:?}'. O cálculo foi alinhado sobre Tamanho Lógico para garantir comparação justa.",
            base.size_mode, current.size_mode
        ));
    }

    // Alinhamento da medida: se incompatíveis, usa tamanho lógico
    let effective_mode = if measures_compatible {
        current.size_mode
    } else {
        TreemapSizeMode::Logical
    };

    let extract_entry_size = |entry: &SnapshotEntry, mode: TreemapSizeMode| -> u64 {
        match mode {
            TreemapSizeMode::Logical => entry.logical_size,
            TreemapSizeMode::Allocated => entry.allocated_size.unwrap_or(entry.logical_size),
            TreemapSizeMode::Reclaimable => entry.reclaimable_size.unwrap_or(0),
        }
    };

    // Prepara normalização dos erros de acesso do snapshot atual relativizando se forem caminhos absolutos
    let curr_root_norm = DiskUsageSnapshot::normalize_rel_path(&current.root_path);
    let base_root_norm = DiskUsageSnapshot::normalize_rel_path(&base.root_path);
    let norm_current_access_errors: Vec<String> = current
        .access_errors
        .iter()
        .map(|p| {
            let norm = DiskUsageSnapshot::normalize_rel_path(p);
            if !curr_root_norm.is_empty() {
                if let Some(stripped) = norm.strip_prefix(&curr_root_norm) {
                    return stripped.trim_start_matches('/').to_string();
                }
            }
            if !base_root_norm.is_empty() {
                if let Some(stripped) = norm.strip_prefix(&base_root_norm) {
                    return stripped.trim_start_matches('/').to_string();
                }
            }
            norm
        })
        .collect();

    let is_covered_by_access_error = |rel_key: &str| -> bool {
        norm_current_access_errors.iter().any(|err_path| {
            rel_key == err_path
                || rel_key.starts_with(&format!("{err_path}/"))
                || err_path.starts_with(&format!("{rel_key}/"))
        })
    };

    let mut all_changes = Vec::new();
    let mut items_grown = 0usize;
    let mut items_shrunk = 0usize;
    let mut items_added = 0usize;
    let mut items_removed = 0usize;
    let mut items_inaccessible = 0usize;

    // Processa itens do snapshot base
    for (rel_key, base_entry) in &base.entries {
        let b_size = extract_entry_size(base_entry, effective_mode);

        if let Some(curr_entry) = current.entries.get(rel_key) {
            let c_size = extract_entry_size(curr_entry, effective_mode);
            let delta = (c_size as i64) - (b_size as i64);

            let kind = if delta > 0 {
                items_grown += 1;
                GrowthChangeKind::Grown
            } else if delta < 0 {
                items_shrunk += 1;
                GrowthChangeKind::Shrunk
            } else {
                GrowthChangeKind::Unchanged
            };

            let delta_pct = if b_size > 0 {
                Some((delta as f64 / b_size as f64) * 100.0)
            } else if c_size > 0 {
                Some(100.0)
            } else {
                Some(0.0)
            };

            if kind != GrowthChangeKind::Unchanged {
                all_changes.push(GrowthItem {
                    relative_path: curr_entry.relative_path.clone(),
                    name: curr_entry.name.clone(),
                    is_dir: curr_entry.is_dir,
                    kind,
                    base_size: Some(b_size),
                    current_size: Some(c_size),
                    delta_bytes: delta,
                    delta_percentage: delta_pct,
                    mtime_base: Some(base_entry.mtime),
                    mtime_current: Some(curr_entry.mtime),
                    had_access_error: false,
                });
            }
        } else {
            // Item existia no base e sumiu no atual:
            // CRITÉRIO DE ACEITE: Falta de permissão não é interpretada automaticamente como espaço liberado!
            if is_covered_by_access_error(rel_key) {
                items_inaccessible += 1;
                all_changes.push(GrowthItem {
                    relative_path: base_entry.relative_path.clone(),
                    name: base_entry.name.clone(),
                    is_dir: base_entry.is_dir,
                    kind: GrowthChangeKind::InaccessibleDueToPermission,
                    base_size: Some(b_size),
                    current_size: None,
                    delta_bytes: 0, // Protegido contra falso espaço liberado!
                    delta_percentage: None,
                    mtime_base: Some(base_entry.mtime),
                    mtime_current: None,
                    had_access_error: true,
                });
            } else {
                items_removed += 1;
                let delta = -(b_size as i64);
                all_changes.push(GrowthItem {
                    relative_path: base_entry.relative_path.clone(),
                    name: base_entry.name.clone(),
                    is_dir: base_entry.is_dir,
                    kind: GrowthChangeKind::Removed,
                    base_size: Some(b_size),
                    current_size: Some(0),
                    delta_bytes: delta,
                    delta_percentage: Some(-100.0),
                    mtime_base: Some(base_entry.mtime),
                    mtime_current: None,
                    had_access_error: false,
                });
            }
        }
    }

    // Processa itens novos no snapshot atual que não existiam no base
    for (rel_key, curr_entry) in &current.entries {
        if !base.entries.contains_key(rel_key) {
            items_added += 1;
            let c_size = extract_entry_size(curr_entry, effective_mode);
            let delta = c_size as i64;

            all_changes.push(GrowthItem {
                relative_path: curr_entry.relative_path.clone(),
                name: curr_entry.name.clone(),
                is_dir: curr_entry.is_dir,
                kind: GrowthChangeKind::Added,
                base_size: Some(0),
                current_size: Some(c_size),
                delta_bytes: delta,
                delta_percentage: Some(100.0),
                mtime_base: None,
                mtime_current: Some(curr_entry.mtime),
                had_access_error: false,
            });
        }
    }

    // Totais consolidados
    let total_base_bytes = match effective_mode {
        TreemapSizeMode::Logical => base.total_logical_bytes,
        TreemapSizeMode::Allocated => base
            .total_allocated_bytes
            .unwrap_or(base.total_logical_bytes),
        TreemapSizeMode::Reclaimable => base.total_reclaimable_bytes.unwrap_or(0),
    };

    let total_current_bytes = match effective_mode {
        TreemapSizeMode::Logical => current.total_logical_bytes,
        TreemapSizeMode::Allocated => current
            .total_allocated_bytes
            .unwrap_or(current.total_logical_bytes),
        TreemapSizeMode::Reclaimable => current.total_reclaimable_bytes.unwrap_or(0),
    };

    let total_delta_bytes = (total_current_bytes as i64) - (total_base_bytes as i64);
    let total_delta_percentage = if total_base_bytes > 0 {
        Some((total_delta_bytes as f64 / total_base_bytes as f64) * 100.0)
    } else if total_current_bytes > 0 {
        Some(100.0)
    } else {
        Some(0.0)
    };

    // Ranquings de destaque:
    // 1. Pastas que mais cresceram (maior delta_bytes positivo)
    let mut top_growing_folders: Vec<GrowthItem> = all_changes
        .iter()
        .filter(|it| it.is_dir && it.delta_bytes > 0)
        .cloned()
        .collect();
    top_growing_folders.sort_by_key(|a| std::cmp::Reverse(a.delta_bytes));
    top_growing_folders.truncate(15);

    // 2. Maiores itens novos adicionados
    let mut top_added_items: Vec<GrowthItem> = all_changes
        .iter()
        .filter(|it| it.kind == GrowthChangeKind::Added)
        .cloned()
        .collect();
    top_added_items.sort_by_key(|a| std::cmp::Reverse(a.current_size.unwrap_or(0)));
    top_added_items.truncate(15);

    // 3. Maiores itens removidos
    let mut top_removed_items: Vec<GrowthItem> = all_changes
        .iter()
        .filter(|it| it.kind == GrowthChangeKind::Removed)
        .cloned()
        .collect();
    top_removed_items.sort_by_key(|a| std::cmp::Reverse(a.base_size.unwrap_or(0)));
    top_removed_items.truncate(15);

    // Ordenação geral das mudanças por magnitude do delta absoluto
    all_changes.sort_by_key(|a| std::cmp::Reverse(a.delta_bytes.abs()));

    SnapshotComparisonReport {
        base_snapshot_id: base.id.clone(),
        base_label: base.label.clone(),
        base_timestamp: base.timestamp,
        current_snapshot_id: current.id.clone(),
        current_label: current.label.clone(),
        current_timestamp: current.timestamp,
        root_path: current.root_path.clone(),
        warnings,
        size_mode: effective_mode,
        measures_compatible,
        full_coverage,
        total_base_bytes,
        total_current_bytes,
        total_delta_bytes,
        total_delta_percentage,
        items_grown,
        items_shrunk,
        items_added,
        items_removed,
        items_inaccessible,
        top_growing_folders,
        top_added_items,
        top_removed_items,
        all_changes,
    }
}

/// Gerenciador de armazenamento local, persistência e retenção de snapshots (TM-11).
#[derive(Debug, Clone)]
pub struct SnapshotStorage {
    base_dir: PathBuf,
}

impl Default for SnapshotStorage {
    fn default() -> Self {
        Self::new(None)
    }
}

impl SnapshotStorage {
    /// Diretório padrão onde o histórico local é guardado com privacidade.
    #[must_use]
    pub fn default_storage_dir() -> PathBuf {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            PathBuf::from(local_app_data)
                .join("fastfile")
                .join("snapshots")
        } else if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
            PathBuf::from(xdg_data).join("fastfile").join("snapshots")
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("fastfile")
                .join("snapshots")
        } else {
            std::env::temp_dir().join("fastfile-snapshots")
        }
    }

    /// Cria uma nova instância de armazenamento.
    #[must_use]
    pub fn new(custom_dir: Option<PathBuf>) -> Self {
        let base_dir = custom_dir.unwrap_or_else(Self::default_storage_dir);
        Self { base_dir }
    }

    /// Retorna o diretório base configurado.
    #[must_use]
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Salva um snapshot localmente em arquivo JSON seguro.
    pub fn save(&self, snapshot: &DiskUsageSnapshot) -> io::Result<PathBuf> {
        fs::create_dir_all(&self.base_dir)?;
        let file_path = self.base_dir.join(format!("{}.json", snapshot.id));
        let json = serde_json::to_string_pretty(snapshot)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(&file_path, json.as_bytes())?;
        Ok(file_path)
    }

    /// Lista todos os snapshots salvos localmente, opcionalmente filtrando por caminho raiz.
    pub fn list(&self, filter_root: Option<&str>) -> io::Result<Vec<DiskUsageSnapshotSummary>> {
        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        let norm_filter = filter_root.map(DiskUsageSnapshot::normalize_rel_path);

        for entry in fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(snap) = serde_json::from_str::<DiskUsageSnapshot>(&content) {
                        let matches = match &norm_filter {
                            Some(f) => DiskUsageSnapshot::normalize_rel_path(&snap.root_path) == *f,
                            None => true,
                        };
                        if matches {
                            results.push(snap.summary());
                        }
                    }
                }
            }
        }

        // Ordena do mais recente para o mais antigo
        results.sort_by_key(|a| std::cmp::Reverse(a.timestamp));
        Ok(results)
    }

    /// Carrega um snapshot completo a partir do seu ID.
    pub fn load(&self, id: &str) -> io::Result<DiskUsageSnapshot> {
        let file_path = self.base_dir.join(format!("{id}.json"));
        let content = fs::read_to_string(&file_path)?;
        serde_json::from_str(&content).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Remove um snapshot individual do disco (Critério: Histórico pode ser removido).
    pub fn delete(&self, id: &str) -> io::Result<bool> {
        let file_path = self.base_dir.join(format!("{id}.json"));
        if file_path.exists() {
            fs::remove_file(file_path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Limpa todo o histórico de snapshots localmente (ou apenas para uma pasta raiz).
    pub fn clear_history(&self, filter_root: Option<&str>) -> io::Result<usize> {
        let items = self.list(filter_root)?;
        let mut count = 0usize;
        for it in items {
            if self.delete(&it.id)? {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Aplica política de retenção mantendo apenas os N snapshots mais recentes do escopo.
    pub fn enforce_retention(&self, root_path: &str, max_keep: usize) -> io::Result<usize> {
        let items = self.list(Some(root_path))?;
        if items.len() <= max_keep {
            return Ok(0);
        }

        let to_remove = &items[max_keep..];
        let mut count = 0usize;
        for it in to_remove {
            if self.delete(&it.id)? {
                count += 1;
            }
        }
        Ok(count)
    }
}

fn rand_simple_suffix(seed: i64) -> u32 {
    let mut h = seed as u64;
    h = h.wrapping_mul(6364136223846793005).wrapping_add(1);
    (h % 900000 + 100000) as u32
}

fn format_snapshot_time(unix: i64) -> String {
    let dt = std::time::UNIX_EPOCH + std::time::Duration::from_secs(unix.max(0) as u64);
    if let Ok(duration) = dt.duration_since(std::time::UNIX_EPOCH) {
        let secs = duration.as_secs();
        let days = secs / 86400;
        let rem_secs = secs % 86400;
        let hours = rem_secs / 3600;
        let mins = (rem_secs % 3600) / 60;
        format!("{days}d {hours:02}:{mins:02}")
    } else {
        "agora".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tm_11_inaccessible_due_to_permission_not_counted_as_removed() {
        // Critério de aceite: Falta de permissão não é interpretada automaticamente como espaço liberado!
        let mut base_entries = HashMap::new();
        base_entries.insert(
            "documents/secret.dat".to_string(),
            SnapshotEntry {
                relative_path: "documents/secret.dat".to_string(),
                name: "secret.dat".to_string(),
                is_dir: false,
                logical_size: 1_000_000,
                allocated_size: Some(1_000_000),
                reclaimable_size: Some(1_000_000),
                mtime: 1700000000,
            },
        );

        let base_snap = DiskUsageSnapshot {
            id: "snap_base".to_string(),
            label: "Base".to_string(),
            root_path: "C:\\Projects".to_string(),
            timestamp: 1700000000,
            size_mode: TreemapSizeMode::Logical,
            is_complete: true,
            access_errors: Vec::new(),
            total_bytes: 1_000_000,
            total_logical_bytes: 1_000_000,
            total_allocated_bytes: Some(1_000_000),
            total_reclaimable_bytes: Some(1_000_000),
            total_files: 1,
            total_dirs: 0,
            entries: base_entries,
        };

        // No snapshot atual, o arquivo sumiu das entradas porque a pasta 'documents' deu erro de acesso
        let current_snap = DiskUsageSnapshot {
            id: "snap_curr".to_string(),
            label: "Atual".to_string(),
            root_path: "C:\\Projects".to_string(),
            timestamp: 1700086400,
            size_mode: TreemapSizeMode::Logical,
            is_complete: false,
            access_errors: vec!["documents".to_string()],
            total_bytes: 0,
            total_logical_bytes: 0,
            total_allocated_bytes: Some(0),
            total_reclaimable_bytes: Some(0),
            total_files: 0,
            total_dirs: 0,
            entries: HashMap::new(),
        };

        let report = compare_snapshots(&base_snap, &current_snap);

        // Não pode haver itens contados como removidos (espaço liberado falso)
        assert_eq!(report.items_removed, 0);
        assert_eq!(report.items_inaccessible, 1);
        assert_eq!(report.all_changes.len(), 1);
        assert_eq!(
            report.all_changes[0].kind,
            GrowthChangeKind::InaccessibleDueToPermission
        );
        assert_eq!(report.all_changes[0].delta_bytes, 0);
        assert!(report.all_changes[0].had_access_error);

        // Deve emitir aviso de permissão e cobertura incompleta
        assert!(!report.warnings.is_empty());
        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("Aviso de Permissão")));
    }

    #[test]
    fn test_tm_11_growth_ranking_new_and_removed_items() {
        let mut base_entries = HashMap::new();
        base_entries.insert(
            "target".to_string(),
            SnapshotEntry {
                relative_path: "target".to_string(),
                name: "target".to_string(),
                is_dir: true,
                logical_size: 500_000_000,
                allocated_size: Some(500_000_000),
                reclaimable_size: Some(500_000_000),
                mtime: 1700000000,
            },
        );
        base_entries.insert(
            "old_installer.iso".to_string(),
            SnapshotEntry {
                relative_path: "old_installer.iso".to_string(),
                name: "old_installer.iso".to_string(),
                is_dir: false,
                logical_size: 2_000_000_000,
                allocated_size: Some(2_000_000_000),
                reclaimable_size: Some(2_000_000_000),
                mtime: 1700000000,
            },
        );

        let base_snap = DiskUsageSnapshot {
            id: "snap_base".to_string(),
            label: "Base".to_string(),
            root_path: "C:\\Projects".to_string(),
            timestamp: 1700000000,
            size_mode: TreemapSizeMode::Logical,
            is_complete: true,
            access_errors: Vec::new(),
            total_bytes: 2_500_000_000,
            total_logical_bytes: 2_500_000_000,
            total_allocated_bytes: Some(2_500_000_000),
            total_reclaimable_bytes: Some(2_500_000_000),
            total_files: 1,
            total_dirs: 1,
            entries: base_entries,
        };

        let mut curr_entries = HashMap::new();
        // target cresceu de 500MB para 1.5GB (+1GB)
        curr_entries.insert(
            "target".to_string(),
            SnapshotEntry {
                relative_path: "target".to_string(),
                name: "target".to_string(),
                is_dir: true,
                logical_size: 1_500_000_000,
                allocated_size: Some(1_500_000_000),
                reclaimable_size: Some(1_500_000_000),
                mtime: 1700100000,
            },
        );
        // old_installer.iso foi removido (não está em curr_entries)
        // novo arquivo adicionado: dataset.tar.gz (800MB)
        curr_entries.insert(
            "dataset.tar.gz".to_string(),
            SnapshotEntry {
                relative_path: "dataset.tar.gz".to_string(),
                name: "dataset.tar.gz".to_string(),
                is_dir: false,
                logical_size: 800_000_000,
                allocated_size: Some(800_000_000),
                reclaimable_size: Some(800_000_000),
                mtime: 1700100000,
            },
        );

        let current_snap = DiskUsageSnapshot {
            id: "snap_curr".to_string(),
            label: "Atual".to_string(),
            root_path: "C:\\Projects".to_string(),
            timestamp: 1700100000,
            size_mode: TreemapSizeMode::Logical,
            is_complete: true,
            access_errors: Vec::new(),
            total_bytes: 2_300_000_000,
            total_logical_bytes: 2_300_000_000,
            total_allocated_bytes: Some(2_300_000_000),
            total_reclaimable_bytes: Some(2_300_000_000),
            total_files: 1,
            total_dirs: 1,
            entries: curr_entries,
        };

        let report = compare_snapshots(&base_snap, &current_snap);

        assert_eq!(report.items_grown, 1);
        assert_eq!(report.items_added, 1);
        assert_eq!(report.items_removed, 1);

        // Ranking de pastas que cresceram
        assert_eq!(report.top_growing_folders.len(), 1);
        assert_eq!(report.top_growing_folders[0].name, "target");
        assert_eq!(report.top_growing_folders[0].delta_bytes, 1_000_000_000);

        // Maiores adicionados
        assert_eq!(report.top_added_items.len(), 1);
        assert_eq!(report.top_added_items[0].name, "dataset.tar.gz");
        assert_eq!(report.top_added_items[0].current_size, Some(800_000_000));

        // Maiores removidos
        assert_eq!(report.top_removed_items.len(), 1);
        assert_eq!(report.top_removed_items[0].name, "old_installer.iso");
        assert_eq!(report.top_removed_items[0].base_size, Some(2_000_000_000));
    }

    #[test]
    fn test_tm_11_measures_incompatible_warning_and_retention() {
        let temp_dir = tempfile::tempdir().unwrap();
        let storage = SnapshotStorage::new(Some(temp_dir.path().to_path_buf()));

        // Snapshot com Logical
        let snap1 = DiskUsageSnapshot {
            id: "snap_1".to_string(),
            label: "Snap 1".to_string(),
            root_path: "C:\\Projects".to_string(),
            timestamp: 1700000000,
            size_mode: TreemapSizeMode::Logical,
            is_complete: true,
            access_errors: Vec::new(),
            total_bytes: 1000,
            total_logical_bytes: 1000,
            total_allocated_bytes: Some(4096),
            total_reclaimable_bytes: Some(1000),
            total_files: 1,
            total_dirs: 0,
            entries: HashMap::new(),
        };

        // Snapshot com Allocated
        let snap2 = DiskUsageSnapshot {
            id: "snap_2".to_string(),
            label: "Snap 2".to_string(),
            root_path: "C:\\Projects".to_string(),
            timestamp: 1700010000,
            size_mode: TreemapSizeMode::Allocated,
            is_complete: true,
            access_errors: Vec::new(),
            total_bytes: 8192,
            total_logical_bytes: 2000,
            total_allocated_bytes: Some(8192),
            total_reclaimable_bytes: Some(2000),
            total_files: 1,
            total_dirs: 0,
            entries: HashMap::new(),
        };

        storage.save(&snap1).unwrap();
        storage.save(&snap2).unwrap();

        let list = storage.list(Some("C:\\Projects")).unwrap();
        assert_eq!(list.len(), 2);

        // Comparação acusa incompatibilidade de medidas
        let report = compare_snapshots(&snap1, &snap2);
        assert!(!report.measures_compatible);
        assert!(report
            .warnings
            .iter()
            .any(|w| w.contains("Incompatibilidade de Medida")));

        // Retenção: reter apenas 1 snapshot
        let pruned = storage.enforce_retention("C:\\Projects", 1).unwrap();
        assert_eq!(pruned, 1);
        let list_after = storage.list(Some("C:\\Projects")).unwrap();
        assert_eq!(list_after.len(), 1);
        assert_eq!(list_after[0].id, "snap_2"); // Manteve o mais recente

        // Limpeza total do histórico (Critério: Histórico pode ser removido)
        let cleared = storage.clear_history(Some("C:\\Projects")).unwrap();
        assert_eq!(cleared, 1);
        assert!(storage.list(Some("C:\\Projects")).unwrap().is_empty());
    }
}
