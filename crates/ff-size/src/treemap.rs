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

/// Item de entrada para cálculo de layout do treemap.
#[derive(Debug, Clone)]
pub struct TreemapInputItem {
    pub id: u32,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

/// Item posicionado e renderizável no treemap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreemapItem {
    pub id: u32,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub rect: TreemapRect,
    pub color: (u8, u8, u8),
}

/// Estrutura completa de layout do treemap calculada para uma pasta/volume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreemapLayout {
    pub root_id: u32,
    pub root_name: String,
    pub total_size: u64,
    pub bounds: TreemapRect,
    pub items: Vec<TreemapItem>,
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

/// Constrói o layout de treemap para um diretório específico a partir do `FileIndex`.
#[must_use]
pub fn build_directory_treemap(
    index: &FileIndex,
    dir_id: u32,
    bounds: TreemapRect,
) -> TreemapLayout {
    let size_tree = crate::IncrementalSizeTree::build(index);
    let mut inputs = Vec::new();
    let mut total_size = 0u64;

    let dir_name = index.name(dir_id).unwrap_or("Root").to_string();

    // Encontra todos os filhos imediatos do diretório
    for id in 0..index.entries.len() {
        let entry = &index.entries[id];
        if entry.is_deleted() || entry.parent != dir_id {
            continue;
        }

        let name = match index.name(id as u32) {
            Ok(n) => n.to_string(),
            Err(_) => continue,
        };

        let size = if entry.is_dir() {
            // Agrega tamanho recursivo dos filhos
            size_tree.dir_size(id as u32)
        } else {
            entry.size
        };

        if size > 0 {
            total_size += size;
            inputs.push(TreemapInputItem {
                id: id as u32,
                name,
                size,
                is_dir: entry.is_dir(),
            });
        }
    }

    let items = compute_squarified_treemap(&inputs, bounds, 2.0);

    TreemapLayout {
        root_id: dir_id,
        root_name: dir_name,
        total_size,
        bounds,
        items,
    }
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
            TreemapInputItem {
                id: 1,
                name: "huge.bin".into(),
                size: 600,
                is_dir: false,
            },
            TreemapInputItem {
                id: 2,
                name: "medium.zip".into(),
                size: 300,
                is_dir: false,
            },
            TreemapInputItem {
                id: 3,
                name: "small.rs".into(),
                size: 100,
                is_dir: false,
            },
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
}
