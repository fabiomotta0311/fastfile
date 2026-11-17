//! Algoritmo de layout Squarified Treemap (§4 e Issue #15).
//!
//! Converte nós hierárquicos de uso de disco em retângulos visualizáveis estilo WizTree / WinDirStat,
//! otimizando a razão de aspecto para ficar próxima de 1 (quadrado) e aplicando descarte de retângulos
//! sub-pixel para não travar a interface egui mesmo com milhões de arquivos.

use ff_index::FileIndex;
use serde::{Deserialize, Serialize};

/// Retângulo no espaço da UI.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TreemapRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl TreemapRect {
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    #[must_use]
    pub fn area(&self) -> f32 {
        self.w * self.h
    }

    #[must_use]
    pub fn aspect_ratio(&self) -> f32 {
        if self.w <= 0.0 || self.h <= 0.0 {
            return f32::MAX;
        }
        (self.w / self.h).max(self.h / self.w)
    }

    #[must_use]
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= (self.x + self.w) && py >= self.y && py <= (self.y + self.h)
    }
}

/// Presets rápidos de recuperação de espaço (TM-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SpaceRecoveryPreset {
    #[default]
    None,
    /// Arquivos de tamanho expressivo (>= 100 MB).
    LargestFiles,
    /// Arquivos na pasta Downloads ou cujo caminho contenha 'download' e idade de modificação > 30 dias.
    OldDownloads,
    /// Instaladores e imagens de disco (.iso, .img, .vmdk, .vhdx, .vdi, .msi, .exe, .pkg, .dmg).
    InstallersAndImages,
    /// Vídeos e gravações grandes (.mp4, .mkv, .mov, .avi, .webm, .wmv, .flv, .m4v >= 20 MB).
    LargeVideos,
    /// Arquivos compactados (.zip, .tar, .gz, .7z, .rar, .tgz, .bz2, .xz, .cab).
    Archives,
    /// Itens cuja data de modificação é superior a 180 dias (~6 meses).
    NotModifiedRecently,
}

impl SpaceRecoveryPreset {
    /// Rótulo curto do preset para exibição em botões e badges.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "Todos",
            Self::LargestFiles => "Maiores Arquivos",
            Self::OldDownloads => "Downloads Antigos",
            Self::InstallersAndImages => "Instaladores e Imagens",
            Self::LargeVideos => "Vídeos Grandes",
            Self::Archives => "Compactados",
            Self::NotModifiedRecently => "Sem Modificação Recente",
        }
    }
}

/// Filtro combinável para análise e recuperação de espaço em disco (TM-06).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SpaceRecoveryFilter {
    /// Preset rápido ativo.
    pub preset: SpaceRecoveryPreset,
    /// Tamanho mínimo em bytes (opcional).
    pub min_size: Option<u64>,
    /// Extensões permitidas em minúsculas sem ponto (ex: ["iso", "zip"]). Se vazio, todas passam.
    pub extensions: Vec<String>,
    /// Idade mínima da data de modificação em segundos (ex: 30 * 86400).
    /// IMPORTANTE: Calculado estritamente com base no mtime do arquivo, não constituindo prova de desuso.
    pub min_mtime_age_secs: Option<i64>,
    /// Substring de caminho ou localização (case-insensitive).
    pub path_pattern: Option<String>,
    /// Se true, inclui exclusivamente arquivos; se false, diretórios também podem ser avaliados.
    pub files_only: bool,
}

impl SpaceRecoveryFilter {
    /// Cria uma configuração de filtro a partir de um preset rápido (TM-06).
    #[must_use]
    pub fn from_preset(preset: SpaceRecoveryPreset) -> Self {
        match preset {
            SpaceRecoveryPreset::None => Self::default(),
            SpaceRecoveryPreset::LargestFiles => Self {
                preset,
                min_size: Some(100 * 1024 * 1024), // 100 MB
                extensions: Vec::new(),
                min_mtime_age_secs: None,
                path_pattern: None,
                files_only: true,
            },
            SpaceRecoveryPreset::OldDownloads => Self {
                preset,
                min_size: None,
                extensions: Vec::new(),
                min_mtime_age_secs: Some(30 * 86400), // 30 dias
                path_pattern: Some("download".to_string()),
                files_only: true,
            },
            SpaceRecoveryPreset::InstallersAndImages => Self {
                preset,
                min_size: None,
                extensions: vec![
                    "iso".to_string(),
                    "img".to_string(),
                    "vmdk".to_string(),
                    "vhdx".to_string(),
                    "vdi".to_string(),
                    "msi".to_string(),
                    "exe".to_string(),
                    "pkg".to_string(),
                    "dmg".to_string(),
                ],
                min_mtime_age_secs: None,
                path_pattern: None,
                files_only: true,
            },
            SpaceRecoveryPreset::LargeVideos => Self {
                preset,
                min_size: Some(20 * 1024 * 1024), // 20 MB
                extensions: vec![
                    "mp4".to_string(),
                    "mkv".to_string(),
                    "mov".to_string(),
                    "avi".to_string(),
                    "webm".to_string(),
                    "wmv".to_string(),
                    "flv".to_string(),
                    "m4v".to_string(),
                ],
                min_mtime_age_secs: None,
                path_pattern: None,
                files_only: true,
            },
            SpaceRecoveryPreset::Archives => Self {
                preset,
                min_size: None,
                extensions: vec![
                    "zip".to_string(),
                    "tar".to_string(),
                    "gz".to_string(),
                    "7z".to_string(),
                    "rar".to_string(),
                    "tgz".to_string(),
                    "bz2".to_string(),
                    "xz".to_string(),
                    "cab".to_string(),
                ],
                min_mtime_age_secs: None,
                path_pattern: None,
                files_only: true,
            },
            SpaceRecoveryPreset::NotModifiedRecently => Self {
                preset,
                min_size: None,
                extensions: Vec::new(),
                min_mtime_age_secs: Some(180 * 86400), // 180 dias
                path_pattern: None,
                files_only: true,
            },
        }
    }

    /// Retorna se o filtro possui algum critério ativo que restrinja os itens exibidos.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.preset != SpaceRecoveryPreset::None
            || self.min_size.is_some()
            || !self.extensions.is_empty()
            || self.min_mtime_age_secs.is_some()
            || self.path_pattern.is_some()
    }

    /// Avalia se um item específico satisfaz todos os critérios combinados deste filtro (TM-06).
    #[must_use]
    pub fn matches(
        &self,
        name: &str,
        path: &str,
        size: u64,
        is_dir: bool,
        mtime: i64,
        now_unix: i64,
    ) -> bool {
        if !self.is_active() {
            return true;
        }

        // Se exige apenas arquivos e o item for diretório, não satisfaz
        if self.files_only && is_dir {
            return false;
        }

        // Critério de tamanho mínimo em bytes
        if let Some(min_size) = self.min_size {
            if size < min_size {
                return false;
            }
        }

        // Critério de extensões permitidas
        if !self.extensions.is_empty() {
            let ext = std::path::Path::new(name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if !self
                .extensions
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(&ext))
            {
                return false;
            }
        }

        // Critério de idade da data de modificação (mtime)
        // Nota: apresentado como idade de modificação, não como prova de desuso.
        if let Some(min_age) = self.min_mtime_age_secs {
            if mtime <= 0 {
                return false;
            }
            let age = now_unix.saturating_sub(mtime);
            if age < min_age {
                return false;
            }
        }

        // Critério de localização / caminho
        if let Some(pattern) = &self.path_pattern {
            let pattern_lower = pattern.to_lowercase();
            let path_lower = path.to_lowercase();
            let name_lower = name.to_lowercase();
            if !path_lower.contains(&pattern_lower) && !name_lower.contains(&pattern_lower) {
                return false;
            }
        }

        true
    }
}

/// Modo de medição utilizado para dimensionar o Treemap (TM-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TreemapSizeMode {
    /// Tamanho nominal/lógico dos fluxos de dados.
    #[default]
    Logical,
    /// Espaço alocado em disco em blocos/clusters.
    Allocated,
    /// Estimativa conservadora de espaço liberável em caso de exclusão.
    Reclaimable,
}

/// Item de entrada para cálculo de layout do treemap.
#[derive(Debug, Clone)]
pub struct TreemapInputItem {
    pub id: u32,
    pub name: String,
    pub size: u64,
    pub logical_size: u64,
    pub allocated_size: Option<u64>,
    pub reclaimable_size: Option<u64>,
    pub is_sparse: bool,
    pub is_compressed: bool,
    pub is_reparse: bool,
    pub is_offline: bool,
    pub is_hardlink: bool,
    pub is_dir: bool,
}

impl TreemapInputItem {
    #[must_use]
    pub fn new(id: u32, name: String, size: u64, is_dir: bool) -> Self {
        Self {
            id,
            name,
            size,
            logical_size: size,
            allocated_size: Some(size),
            reclaimable_size: Some(size),
            is_sparse: false,
            is_compressed: false,
            is_reparse: false,
            is_offline: false,
            is_hardlink: false,
            is_dir,
        }
    }
}

/// Item posicionado e renderizável no treemap gráfico.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreemapItem {
    pub id: u32,
    pub name: String,
    pub size: u64,
    pub logical_size: u64,
    pub allocated_size: Option<u64>,
    pub reclaimable_size: Option<u64>,
    pub is_sparse: bool,
    pub is_compressed: bool,
    pub is_reparse: bool,
    pub is_offline: bool,
    pub is_hardlink: bool,
    pub is_dir: bool,
    pub rect: TreemapRect,
    pub color: (u8, u8, u8),
}

/// Item tabular completo para a lista ordenada e sincronizada com o treemap (TM-04).
///
/// Contém todos os itens filhos do diretório analisado, inclusive aqueles microscópicos
/// ou com tamanho zero que não geraram um retângulo visível no mapa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreemapRowItem {
    pub id: u32,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub logical_size: u64,
    pub allocated_size: Option<u64>,
    pub reclaimable_size: Option<u64>,
    pub percentage: f32,
    pub is_dir: bool,
    pub is_hardlink: bool,
    pub is_sparse: bool,
    pub is_compressed: bool,
    pub is_reparse: bool,
    pub is_offline: bool,
    pub mtime: i64,
    pub has_rect: bool,
    pub color: (u8, u8, u8),
}

/// Estrutura completa de layout do treemap calculada para uma pasta/volume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreemapLayout {
    pub root_id: u32,
    pub root_name: String,
    pub size_mode: TreemapSizeMode,
    pub total_size: u64,
    pub total_logical_size: u64,
    pub total_allocated_size: Option<u64>,
    pub total_reclaimable_size: Option<u64>,
    pub bounds: TreemapRect,
    pub items: Vec<TreemapItem>,
    pub all_items: Vec<TreemapRowItem>,
    /// Tamanho total da pasta inteira sem aplicação de filtros (no modo de medição selecionado).
    #[serde(default)]
    pub total_folder_size: u64,
    /// Tamanho lógico total da pasta inteira.
    #[serde(default)]
    pub total_folder_logical_size: u64,
    /// Espaço alocado total da pasta inteira.
    #[serde(default)]
    pub total_folder_allocated_size: Option<u64>,
    /// Estimativa de recuperação total da pasta inteira.
    #[serde(default)]
    pub total_folder_reclaimable_size: Option<u64>,
    /// Quantidade total de itens na pasta inteira antes da filtragem.
    #[serde(default)]
    pub total_folder_items_count: usize,
    /// Filtro de recuperação de espaço ativo no cálculo deste layout (TM-06).
    #[serde(default)]
    pub active_filter: Option<SpaceRecoveryFilter>,
}

impl TreemapLayout {
    /// Encontra o item sob o cursor na posição (x, y), útil para hover tooltip e cliques.
    #[must_use]
    pub fn find_at(&self, x: f32, y: f32) -> Option<&TreemapItem> {
        self.items.iter().find(|item| item.rect.contains(x, y))
    }
}

/// Retorna uma cor RGB harmoniosa baseada na extensão ou tipo do item.
#[must_use]
pub fn color_for_name_and_type(name: &str, is_dir: bool) -> (u8, u8, u8) {
    if is_dir {
        return (70, 80, 95); // ardósia azulada para pastas
    }

    let ext = std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        // Código e configurações
        "rs" => (222, 100, 38),         // Laranja Rust
        "py" => (53, 114, 165),         // Azul Python
        "js" | "jsx" => (241, 224, 90), // Amarelo JS
        "ts" | "tsx" => (49, 120, 198), // Azul TS
        "go" => (0, 173, 216),          // Ciano Go
        "c" | "cpp" | "h" => (100, 120, 200),
        "json" | "toml" | "yaml" | "yml" => (200, 140, 60),

        // Compactados e imagens de disco
        "zip" | "tar" | "gz" | "7z" | "rar" => (160, 80, 200),
        "iso" | "vhdx" | "vmdk" => (140, 60, 180),

        // Mídia e áudio/vídeo
        "mp4" | "mkv" | "avi" | "mov" => (46, 204, 113),
        "mp3" | "flac" | "wav" => (39, 174, 96),
        "png" | "jpg" | "jpeg" | "webp" | "svg" => (26, 188, 156),

        // Executáveis e bibliotecas
        "exe" | "dll" | "so" => (231, 76, 60),

        // Documentos
        "pdf" => (192, 57, 43),
        "docx" | "xlsx" | "pptx" => (41, 128, 185),

        _ => {
            // Hash determinístico simples para extensões desconhecidas
            let mut h = 0u32;
            for b in ext.bytes() {
                h = h.wrapping_mul(31).wrapping_add(b as u32);
            }
            let r = 80 + ((h & 0x7F) as u8);
            let g = 80 + (((h >> 7) & 0x7F) as u8);
            let b = 80 + (((h >> 14) & 0x7F) as u8);
            (r, g, b)
        }
    }
}

/// Calcula o particionamento Squarified Treemap de Bruls, Huizing e van Wijk.
///
/// Garante que retângulos tenham razão de aspecto próxima de 1 e ignora
/// retângulos com dimensão menor que `min_pixel_size` (evita gargalo na UI).
#[must_use]
pub fn compute_squarified_treemap(
    inputs: &[TreemapInputItem],
    bounds: TreemapRect,
    min_pixel_size: f32,
) -> Vec<TreemapItem> {
    if inputs.is_empty() || bounds.w <= 0.0 || bounds.h <= 0.0 {
        return Vec::new();
    }

    // 1. Filtra itens com tamanho > 0 e ordena decrescente
    let mut valid_items: Vec<&TreemapInputItem> = inputs.iter().filter(|i| i.size > 0).collect();
    valid_items.sort_by_key(|a| std::cmp::Reverse(a.size));

    let total_size: u64 = valid_items.iter().map(|i| i.size).sum();
    if total_size == 0 {
        return Vec::new();
    }

    let total_area = bounds.area();
    let scale = total_area / (total_size as f32);

    let mut result = Vec::with_capacity(valid_items.len());
    let mut current_rect = bounds;
    let mut current_row: Vec<(&TreemapInputItem, f32)> = Vec::new();

    for item in valid_items {
        let item_area = (item.size as f32) * scale;
        if item_area <= 0.0 {
            continue;
        }

        if current_row.is_empty() {
            current_row.push((item, item_area));
        } else {
            let shorter_side = current_rect.w.min(current_rect.h);
            let worst_before = worst_aspect_ratio(&current_row, shorter_side);

            let mut proposed_row = current_row.clone();
            proposed_row.push((item, item_area));
            let worst_after = worst_aspect_ratio(&proposed_row, shorter_side);

            if worst_after <= worst_before {
                current_row.push((item, item_area));
            } else {
                // Finaliza a linha atual e avança o retângulo
                layout_row(&current_row, &mut current_rect, &mut result, min_pixel_size);
                current_row.clear();
                current_row.push((item, item_area));
            }
        }
    }

    if !current_row.is_empty() {
        layout_row(&current_row, &mut current_rect, &mut result, min_pixel_size);
    }

    result
}

fn worst_aspect_ratio(row: &[(&TreemapInputItem, f32)], length: f32) -> f32 {
    if length <= 0.0 || row.is_empty() {
        return f32::MAX;
    }
    let row_area: f32 = row.iter().map(|(_, a)| *a).sum();
    let row_width = row_area / length;
    if row_width <= 0.0 {
        return f32::MAX;
    }

    let mut worst = 0.0f32;
    for &(_, area) in row {
        let item_height = area / row_width;
        let aspect = if item_height > 0.0 && row_width > 0.0 {
            (row_width / item_height).max(item_height / row_width)
        } else {
            f32::MAX
        };
        if aspect > worst {
            worst = aspect;
        }
    }

    worst
}

fn layout_row(
    row: &[(&TreemapInputItem, f32)],
    rect: &mut TreemapRect,
    output: &mut Vec<TreemapItem>,
    min_pixel_size: f32,
) {
    if row.is_empty() || rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }

    let row_area: f32 = row.iter().map(|(_, a)| *a).sum();
    let is_horizontal = rect.w >= rect.h;

    if is_horizontal {
        let row_w = (row_area / rect.h).min(rect.w);
        let mut y_offset = rect.y;

        for &(item, area) in row {
            let item_h = if row_w > 0.0 { area / row_w } else { 0.0 };
            if row_w >= min_pixel_size && item_h >= min_pixel_size {
                output.push(TreemapItem {
                    id: item.id,
                    name: item.name.clone(),
                    size: item.size,
                    logical_size: item.logical_size,
                    allocated_size: item.allocated_size,
                    reclaimable_size: item.reclaimable_size,
                    is_sparse: item.is_sparse,
                    is_compressed: item.is_compressed,
                    is_reparse: item.is_reparse,
                    is_offline: item.is_offline,
                    is_hardlink: item.is_hardlink,
                    is_dir: item.is_dir,
                    rect: TreemapRect::new(rect.x, y_offset, row_w, item_h),
                    color: color_for_name_and_type(&item.name, item.is_dir),
                });
            }
            y_offset += item_h;
        }

        rect.x += row_w;
        rect.w = (rect.w - row_w).max(0.0);
    } else {
        let row_h = (row_area / rect.w).min(rect.h);
        let mut x_offset = rect.x;

        for &(item, area) in row {
            let item_w = if row_h > 0.0 { area / row_h } else { 0.0 };
            if item_w >= min_pixel_size && row_h >= min_pixel_size {
                output.push(TreemapItem {
                    id: item.id,
                    name: item.name.clone(),
                    size: item.size,
                    logical_size: item.logical_size,
                    allocated_size: item.allocated_size,
                    reclaimable_size: item.reclaimable_size,
                    is_sparse: item.is_sparse,
                    is_compressed: item.is_compressed,
                    is_reparse: item.is_reparse,
                    is_offline: item.is_offline,
                    is_hardlink: item.is_hardlink,
                    is_dir: item.is_dir,
                    rect: TreemapRect::new(x_offset, rect.y, item_w, row_h),
                    color: color_for_name_and_type(&item.name, item.is_dir),
                });
            }
            x_offset += item_w;
        }

        rect.y += row_h;
        rect.h = (rect.h - row_h).max(0.0);
    }
}

/// Constrói o layout de treemap para um diretório específico com modo de medição e filtro de recuperação de espaço (TM-06).
#[must_use]
pub fn build_directory_treemap_filtered(
    index: &FileIndex,
    dir_id: u32,
    bounds: TreemapRect,
    mode: TreemapSizeMode,
    filter: Option<&SpaceRecoveryFilter>,
) -> TreemapLayout {
    let size_tree = crate::IncrementalSizeTree::build(index);
    let mut all_inputs = Vec::new();
    let mut total_folder_logical_size = 0u64;

    let dir_name = index.name(dir_id).unwrap_or("Root").to_string();
    let parent_path = index.build_path(dir_id).unwrap_or_default();
    let has_trailing_sep = parent_path.ends_with('\\') || parent_path.ends_with('/');

    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // 1. Encontra todos os filhos imediatos do diretório e agrega métricas da pasta completa
    for id in 0..index.entries.len() {
        let entry = &index.entries[id];
        if entry.is_deleted() || entry.parent != dir_id {
            continue;
        }

        let name = match index.name(id as u32) {
            Ok(n) => n.to_string(),
            Err(_) => continue,
        };

        let metrics = if entry.is_dir() {
            size_tree.dir_metrics(id as u32)
        } else {
            crate::file_metrics(index, id as u32)
        };

        total_folder_logical_size += metrics.logical_size;

        let display_size = match mode {
            TreemapSizeMode::Logical => metrics.logical_size,
            TreemapSizeMode::Allocated => metrics.allocated_size.unwrap_or(metrics.logical_size),
            TreemapSizeMode::Reclaimable => metrics.reclaimable_size.unwrap_or(0),
        };

        all_inputs.push((
            TreemapInputItem {
                id: id as u32,
                name,
                size: display_size,
                logical_size: metrics.logical_size,
                allocated_size: metrics.allocated_size,
                reclaimable_size: metrics.reclaimable_size,
                is_sparse: entry.is_sparse(),
                is_compressed: entry.is_compressed(),
                is_reparse: entry.is_reparse(),
                is_offline: entry.is_offline(),
                is_hardlink: entry.is_hardlink(),
                is_dir: entry.is_dir(),
            },
            entry.mtime,
        ));
    }

    let total_folder_items_count = all_inputs.len();

    let (total_folder_allocated_size, total_folder_reclaimable_size) =
        if (dir_id as usize) < index.entries.len() {
            (
                size_tree.dir_allocated_size(dir_id),
                size_tree.dir_reclaimable_size(dir_id),
            )
        } else {
            (None, None)
        };

    let total_folder_size = match mode {
        TreemapSizeMode::Logical => total_folder_logical_size,
        TreemapSizeMode::Allocated => {
            total_folder_allocated_size.unwrap_or(total_folder_logical_size)
        }
        TreemapSizeMode::Reclaimable => total_folder_reclaimable_size.unwrap_or(0),
    };

    // 2. Aplica o filtro de recuperação de espaço (TM-06)
    let is_filter_active = filter.is_some_and(|f| f.is_active());
    let mut filtered_inputs = Vec::new();
    let mut filtered_logical_size = 0u64;
    let mut filtered_allocated_sum = 0u64;
    let mut has_some_allocated = false;
    let mut filtered_reclaimable_sum = 0u64;
    let mut has_some_reclaimable = false;

    for (input, mtime) in &all_inputs {
        let path = if parent_path.is_empty() {
            input.name.clone()
        } else if has_trailing_sep {
            format!("{parent_path}{}", input.name)
        } else {
            format!("{parent_path}\\{}", input.name)
        };

        let matches = match filter {
            Some(f) if f.is_active() => f.matches(
                &input.name,
                &path,
                input.logical_size,
                input.is_dir,
                *mtime,
                now_unix,
            ),
            _ => true,
        };

        if matches {
            filtered_logical_size += input.logical_size;
            if let Some(alloc) = input.allocated_size {
                filtered_allocated_sum += alloc;
                has_some_allocated = true;
            }
            if let Some(rec) = input.reclaimable_size {
                filtered_reclaimable_sum += rec;
                has_some_reclaimable = true;
            }
            filtered_inputs.push(input.clone());
        }
    }

    let (total_size, total_logical_size, total_allocated_size, total_reclaimable_size) =
        if is_filter_active {
            let filtered_allocated_size = if has_some_allocated {
                Some(filtered_allocated_sum)
            } else {
                None
            };
            let filtered_reclaimable_size = if has_some_reclaimable {
                Some(filtered_reclaimable_sum)
            } else {
                None
            };
            let tot = match mode {
                TreemapSizeMode::Logical => filtered_logical_size,
                TreemapSizeMode::Allocated => {
                    filtered_allocated_size.unwrap_or(filtered_logical_size)
                }
                TreemapSizeMode::Reclaimable => filtered_reclaimable_size.unwrap_or(0),
            };
            (
                tot,
                filtered_logical_size,
                filtered_allocated_size,
                filtered_reclaimable_size,
            )
        } else {
            (
                total_folder_size,
                total_folder_logical_size,
                total_folder_allocated_size,
                total_folder_reclaimable_size,
            )
        };

    // 3. Calcula os retângulos geométricos Squarified Treemap para os itens filtrados
    let items = compute_squarified_treemap(&filtered_inputs, bounds, 2.0);

    // 4. Constrói os itens tabulares completos para a lista sincronizada (TM-04 / TM-06)
    // O mapa e a lista mostram consistentemente o conjunto filtrado!
    let rect_ids: std::collections::HashSet<u32> = items.iter().map(|it| it.id).collect();
    let mut all_items = Vec::with_capacity(filtered_inputs.len());

    for item in &filtered_inputs {
        let entry = &index.entries[item.id as usize];
        let has_rect = rect_ids.contains(&item.id);
        let percentage = if total_size > 0 {
            (item.size as f64 / total_size as f64 * 100.0) as f32
        } else {
            0.0
        };

        let path = if parent_path.is_empty() {
            item.name.clone()
        } else if has_trailing_sep {
            format!("{parent_path}{}", item.name)
        } else {
            let is_unix =
                parent_path.contains('/') || (cfg!(not(windows)) && !parent_path.contains(':'));
            let sep = if is_unix { '/' } else { '\\' };
            format!("{parent_path}{sep}{}", item.name)
        };

        all_items.push(TreemapRowItem {
            id: item.id,
            name: item.name.clone(),
            path,
            size: item.size,
            logical_size: item.logical_size,
            allocated_size: item.allocated_size,
            reclaimable_size: item.reclaimable_size,
            percentage,
            is_dir: item.is_dir,
            is_hardlink: item.is_hardlink,
            is_sparse: item.is_sparse,
            is_compressed: item.is_compressed,
            is_reparse: item.is_reparse,
            is_offline: item.is_offline,
            mtime: entry.mtime,
            has_rect,
            color: color_for_name_and_type(&item.name, item.is_dir),
        });
    }

    TreemapLayout {
        root_id: dir_id,
        root_name: dir_name,
        size_mode: mode,
        total_size,
        total_logical_size,
        total_allocated_size,
        total_reclaimable_size,
        bounds,
        items,
        all_items,
        total_folder_size,
        total_folder_logical_size,
        total_folder_allocated_size,
        total_folder_reclaimable_size,
        total_folder_items_count,
        active_filter: if is_filter_active {
            filter.cloned()
        } else {
            None
        },
    }
}

/// Constrói o layout de treemap para um diretório específico com modo de medição configurável (TM-03).
#[must_use]
pub fn build_directory_treemap_with_mode(
    index: &FileIndex,
    dir_id: u32,
    bounds: TreemapRect,
    mode: TreemapSizeMode,
) -> TreemapLayout {
    build_directory_treemap_filtered(index, dir_id, bounds, mode, None)
}

/// Constrói o layout de treemap para um diretório específico a partir do `FileIndex` (modo padrão Lógico).
#[must_use]
pub fn build_directory_treemap(
    index: &FileIndex,
    dir_id: u32,
    bounds: TreemapRect,
) -> TreemapLayout {
    build_directory_treemap_with_mode(index, dir_id, bounds, TreemapSizeMode::Logical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_treemap_aspect_ratio_and_contains() {
        let r = TreemapRect::new(10.0, 20.0, 100.0, 50.0);
        assert_eq!(r.aspect_ratio(), 2.0);
        assert!(r.contains(50.0, 30.0));
        assert!(!r.contains(5.0, 30.0));
        assert!(!r.contains(50.0, 80.0));
    }

    #[test]
    fn test_squarified_treemap_layout() {
        let inputs = vec![
            TreemapInputItem::new(1, "huge.bin".into(), 600, false),
            TreemapInputItem::new(2, "medium.zip".into(), 300, false),
            TreemapInputItem::new(3, "small.rs".into(), 100, false),
        ];

        let bounds = TreemapRect::new(0.0, 0.0, 1000.0, 1000.0);
        let items = compute_squarified_treemap(&inputs, bounds, 1.0);

        assert_eq!(items.len(), 3);
        // O maior item deve ter a maior área
        assert!(items[0].rect.area() > items[1].rect.area());
        assert!(items[1].rect.area() > items[2].rect.area());

        // Soma das áreas deve ser muito próxima de 1000 * 1000 = 1_000_000
        let sum_area: f32 = items.iter().map(|i| i.rect.area()).sum();
        assert!((sum_area - 1_000_000.0).abs() < 100.0);
    }

    #[test]
    fn test_color_for_name_and_type() {
        let rust_color = color_for_name_and_type("main.rs", false);
        let py_color = color_for_name_and_type("script.py", false);
        let dir_color = color_for_name_and_type("my_folder", true);

        assert_ne!(rust_color, py_color);
        assert_eq!(dir_color, (70, 80, 95));
    }

    #[test]
    fn test_treemap_all_items_includes_zero_and_micro_sizes() {
        use ff_index::entry::FLAG_DIRECTORY;

        let mut index = FileIndex::new();
        let root = index
            .add_entry(1, ff_index::entry::NO_PARENT, "C:", FLAG_DIRECTORY, 0, 1000)
            .unwrap();

        // 1 item grande
        let huge = index
            .add_entry(2, root, "huge.dat", 0, 10_000_000, 1010)
            .unwrap();
        // 1 item microscópico (1 byte num canvas minúsculo vai ser menor que min_pixel_size)
        let micro = index.add_entry(3, root, "micro.txt", 0, 1, 1020).unwrap();
        // 1 item vazio (0 bytes)
        let empty = index.add_entry(4, root, "empty.log", 0, 0, 1030).unwrap();

        // Canvas muito pequeno para forçar o descarte do micro no cálculo de retângulos
        let bounds = TreemapRect::new(0.0, 0.0, 10.0, 10.0);
        let layout =
            build_directory_treemap_with_mode(&index, root, bounds, TreemapSizeMode::Logical);

        // all_items deve conter todos os 3 itens!
        assert_eq!(layout.all_items.len(), 3);

        let micro_item = layout.all_items.iter().find(|i| i.id == micro).unwrap();
        assert_eq!(micro_item.name, "micro.txt");
        assert_eq!(micro_item.size, 1);
        assert_eq!(micro_item.mtime, 1020);

        let empty_item = layout.all_items.iter().find(|i| i.id == empty).unwrap();
        assert_eq!(empty_item.name, "empty.log");
        assert_eq!(empty_item.size, 0);
        assert_eq!(empty_item.percentage, 0.0);
        assert_eq!(empty_item.mtime, 1030);

        let huge_item = layout.all_items.iter().find(|i| i.id == huge).unwrap();
        assert_eq!(huge_item.name, "huge.dat");
        assert!(huge_item.percentage > 99.0);
    }

    #[test]
    fn test_tm_06_space_recovery_filters_presets_and_combinations() {
        use ff_index::entry::FLAG_DIRECTORY;

        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let mut index = FileIndex::new();
        let root = index
            .add_entry(
                1,
                ff_index::entry::NO_PARENT,
                "C:\\Test",
                FLAG_DIRECTORY,
                0,
                now_unix,
            )
            .unwrap();

        // 1. Arquivo enorme (150 MB, modificado hoje)
        let f_huge = index
            .add_entry(2, root, "database.bak", 0, 150 * 1024 * 1024, now_unix)
            .unwrap();
        // 2. Download antigo (20 MB, na pasta 'downloads', modificado há 40 dias)
        let mtime_40d = now_unix - (40 * 86400);
        let f_old_dl = index
            .add_entry(
                3,
                root,
                "downloads_setup.exe",
                0,
                20 * 1024 * 1024,
                mtime_40d,
            )
            .unwrap();
        // 3. Imagem ISO (4 GB, modificada há 10 dias)
        let f_iso = index
            .add_entry(
                4,
                root,
                "ubuntu.iso",
                0,
                4 * 1024 * 1024 * 1024,
                now_unix - (10 * 86400),
            )
            .unwrap();
        // 4. Vídeo grande (50 MB .mp4, modificado há 5 dias)
        let f_video = index
            .add_entry(
                5,
                root,
                "recording.mp4",
                0,
                50 * 1024 * 1024,
                now_unix - (5 * 86400),
            )
            .unwrap();
        // 5. Arquivo compactado (80 MB .zip, modificado há 200 dias)
        let mtime_200d = now_unix - (200 * 86400);
        let f_archive = index
            .add_entry(6, root, "backup.zip", 0, 80 * 1024 * 1024, mtime_200d)
            .unwrap();
        // 6. Arquivo pequeno recente (1 KB .txt)
        let _f_small = index
            .add_entry(7, root, "notes.txt", 0, 1024, now_unix)
            .unwrap();

        let bounds = TreemapRect::new(0.0, 0.0, 800.0, 600.0);

        // A. Sem filtro: Todos os 6 itens devem estar presentes
        let layout_all =
            build_directory_treemap_filtered(&index, root, bounds, TreemapSizeMode::Logical, None);
        assert_eq!(layout_all.all_items.len(), 6);
        assert_eq!(layout_all.total_folder_items_count, 6);
        assert_eq!(layout_all.total_size, layout_all.total_folder_size);

        // B. Preset: LargestFiles (>= 100 MB) -> database.bak (150MB) e ubuntu.iso (4GB)
        let f_largest = SpaceRecoveryFilter::from_preset(SpaceRecoveryPreset::LargestFiles);
        let layout_largest = build_directory_treemap_filtered(
            &index,
            root,
            bounds,
            TreemapSizeMode::Logical,
            Some(&f_largest),
        );
        assert_eq!(layout_largest.all_items.len(), 2);
        assert!(layout_largest.all_items.iter().any(|i| i.id == f_huge));
        assert!(layout_largest.all_items.iter().any(|i| i.id == f_iso));
        // Total da pasta permanece 6, mas total filtrado é 2
        assert_eq!(layout_largest.total_folder_items_count, 6);
        assert_eq!(layout_largest.all_items.len(), layout_largest.items.len());

        // C. Preset: OldDownloads -> downloads_setup.exe (contém 'download' e idade > 30d)
        let f_dl = SpaceRecoveryFilter::from_preset(SpaceRecoveryPreset::OldDownloads);
        let layout_dl = build_directory_treemap_filtered(
            &index,
            root,
            bounds,
            TreemapSizeMode::Logical,
            Some(&f_dl),
        );
        assert_eq!(layout_dl.all_items.len(), 1);
        assert_eq!(layout_dl.all_items[0].id, f_old_dl);

        // D. Preset: InstallersAndImages -> downloads_setup.exe (.exe) e ubuntu.iso (.iso)
        let f_inst = SpaceRecoveryFilter::from_preset(SpaceRecoveryPreset::InstallersAndImages);
        let layout_inst = build_directory_treemap_filtered(
            &index,
            root,
            bounds,
            TreemapSizeMode::Logical,
            Some(&f_inst),
        );
        assert_eq!(layout_inst.all_items.len(), 2);
        assert!(layout_inst.all_items.iter().any(|i| i.id == f_old_dl));
        assert!(layout_inst.all_items.iter().any(|i| i.id == f_iso));

        // E. Preset: LargeVideos -> recording.mp4
        let f_vid = SpaceRecoveryFilter::from_preset(SpaceRecoveryPreset::LargeVideos);
        let layout_vid = build_directory_treemap_filtered(
            &index,
            root,
            bounds,
            TreemapSizeMode::Logical,
            Some(&f_vid),
        );
        assert_eq!(layout_vid.all_items.len(), 1);
        assert_eq!(layout_vid.all_items[0].id, f_video);

        // F. Preset: Archives -> backup.zip
        let f_arch = SpaceRecoveryFilter::from_preset(SpaceRecoveryPreset::Archives);
        let layout_arch = build_directory_treemap_filtered(
            &index,
            root,
            bounds,
            TreemapSizeMode::Logical,
            Some(&f_arch),
        );
        assert_eq!(layout_arch.all_items.len(), 1);
        assert_eq!(layout_arch.all_items[0].id, f_archive);

        // G. Preset: NotModifiedRecently (> 180 dias) -> backup.zip (200 dias)
        let f_old = SpaceRecoveryFilter::from_preset(SpaceRecoveryPreset::NotModifiedRecently);
        let layout_old = build_directory_treemap_filtered(
            &index,
            root,
            bounds,
            TreemapSizeMode::Logical,
            Some(&f_old),
        );
        assert_eq!(layout_old.all_items.len(), 1);
        assert_eq!(layout_old.all_items[0].id, f_archive);

        // H. Combinação customizada: Extensão .iso + tamanho mínimo de 1 GB
        let custom = SpaceRecoveryFilter {
            extensions: vec!["iso".to_string()],
            min_size: Some(1024 * 1024 * 1024),
            ..Default::default()
        };
        let layout_custom = build_directory_treemap_filtered(
            &index,
            root,
            bounds,
            TreemapSizeMode::Logical,
            Some(&custom),
        );
        assert_eq!(layout_custom.all_items.len(), 1);
        assert_eq!(layout_custom.all_items[0].id, f_iso);
        assert_eq!(layout_custom.total_folder_items_count, 6);
    }
}
