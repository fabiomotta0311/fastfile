//! Lista de revisão e simulação de recuperação de espaço em disco (TM-09).
//!
//! Permite selecionar e gerenciar itens candidatos a recuperação sem realizar qualquer
//! ação destrutiva (modo exclusivamente consultivo), deduplicando automaticamente tamanhos
//! entre pastas e descendentes, alertando sobre itens desatualizados ou sensíveis e
//! calculando projeções de recuperação física real.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::cleanup::{analyze_cleanup_opportunity, CleanupAdvice, CleanupCategory};

/// Representa um arquivo ou diretório marcado para simulação de recuperação (TM-09).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewCandidateItem {
    /// Identificador do item no índice ou interface.
    pub id: u32,
    /// Caminho completo no sistema de arquivos.
    pub path: String,
    /// Nome base do item.
    pub name: String,
    /// Indica se o item é um diretório.
    pub is_dir: bool,
    /// Tamanho lógico nominal em bytes.
    pub logical_size: u64,
    /// Espaço alocado em disco em bytes (ou `None` se desconhecido).
    pub allocated_size: Option<u64>,
    /// Estimativa conservadora de espaço efetivamente liberável (TM-03).
    pub reclaimable_size: Option<u64>,
    /// Timestamp Unix de modificação registrado no momento da marcação.
    pub mtime: i64,
    /// Indica se o arquivo é um hard link compartilhado.
    pub is_hardlink: bool,
    /// Categoria explicável de limpeza diagnosticada (TM-07).
    pub category: CleanupCategory,
    /// Motivo e regra do diagnóstico.
    pub advice_reason: String,
    /// Alerta de segurança específico (se houver).
    pub alert: Option<String>,
    /// Se estiver coberto por uma pasta ancestral também selecionada, contém o caminho do ancestral.
    /// Itens cobertos não têm seu tamanho somado novamente ao total (evitando contagem dupla).
    pub covered_by_ancestor: Option<String>,
    /// Indica se o item sofreu modificação no disco após ter sido adicionado à lista.
    pub is_stale: bool,
    /// Indica se o item ainda existe fisicamente no disco.
    pub exists: bool,
}

impl ReviewCandidateItem {
    /// Cria um candidato de revisão a partir de dados básicos e realiza a classificação explicável (TM-07).
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: u32,
        path: String,
        name: String,
        is_dir: bool,
        logical_size: u64,
        allocated_size: Option<u64>,
        reclaimable_size: Option<u64>,
        mtime: i64,
        is_hardlink: bool,
    ) -> Self {
        let advice: CleanupAdvice = analyze_cleanup_opportunity(&path, &name, is_dir, None);

        let alert = match advice.category {
            CleanupCategory::PersonalContent => Some(
                "👤 Conteúdo Pessoal: exclusão irreversível. Nenhuma exclusão automática é recomendada."
                    .to_string(),
            ),
            CleanupCategory::SystemProtected => Some(
                "🛡️ Sistema/Protegido: arquivo crítico do sistema operacional.".to_string(),
            ),
            CleanupCategory::ApplicationData => Some(
                "📦 Gerenciado por Aplicação: gerencie pelo software proprietário.".to_string(),
            ),
            CleanupCategory::ManualReview => {
                Some("🔍 Revisão Manual: não há garantia de descarte seguro.".to_string())
            }
            _ => None,
        };

        Self {
            id,
            path,
            name,
            is_dir,
            logical_size,
            allocated_size,
            reclaimable_size,
            mtime,
            is_hardlink,
            category: advice.category,
            advice_reason: advice.reason,
            alert,
            covered_by_ancestor: None,
            is_stale: false,
            exists: true,
        }
    }
}

/// Resumo agregado da simulação de recuperação de espaço (TM-09).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ReviewSimulationSummary {
    /// Quantidade total de itens na lista de revisão.
    pub total_items: usize,
    /// Quantidade de itens raiz efetivos (cujo tamanho é contabilizado na recuperação).
    pub effective_items: usize,
    /// Quantidade de itens descendentes cobertos por pastas ancestrais (duplicação evitada).
    pub covered_items: usize,
    /// Tamanho lógico total selecionado (nominal), excluindo itens cobertos por ancestrais.
    pub total_selected_logical_size: u64,
    /// Estimativa de espaço físico efetivamente recuperável em disco, deduplicada.
    pub total_estimated_reclaimable_size: u64,
    /// Espaço já recuperado no disco nesta sessão (sempre 0 em modo consultivo).
    pub space_already_reclaimed: u64,
    /// Quantidade de itens desatualizados ou não encontrados no disco.
    pub stale_or_missing_count: usize,
    /// Quantidade de itens marcados identificados como Conteúdo Pessoal.
    pub personal_content_count: usize,
    /// Quantidade de itens marcados identificados como Revisão Manual ou Desconhecidos.
    pub manual_review_count: usize,
}

/// Gerenciador consultivo da lista de revisão e simulação de recuperação (TM-09).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecoverySimulation {
    /// Itens atualmente marcados na lista de revisão.
    pub candidates: Vec<ReviewCandidateItem>,
    /// Espaço já recuperado fisicamente (mantido para clareza e separação de medidas).
    pub space_already_reclaimed: u64,
}

impl RecoverySimulation {
    /// Cria uma nova simulação vazia.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adiciona um item candidato se ele ainda não estiver na lista.
    pub fn add_candidate(&mut self, candidate: ReviewCandidateItem) -> bool {
        if self.contains_path(&candidate.path) {
            return false;
        }
        self.candidates.push(candidate);
        self.recalculate();
        true
    }

    /// Remove um item candidato através do seu caminho completo.
    pub fn remove_candidate(&mut self, path: &str) -> bool {
        let initial_len = self.candidates.len();
        self.candidates
            .retain(|c| !c.path.eq_ignore_ascii_case(path));
        let changed = self.candidates.len() != initial_len;
        if changed {
            self.recalculate();
        }
        changed
    }

    /// Remove um item candidato pelo seu ID.
    pub fn remove_candidate_by_id(&mut self, id: u32) -> bool {
        let initial_len = self.candidates.len();
        self.candidates.retain(|c| c.id != id);
        let changed = self.candidates.len() != initial_len;
        if changed {
            self.recalculate();
        }
        changed
    }

    /// Limpa toda a lista de candidatos.
    pub fn clear(&mut self) {
        self.candidates.clear();
    }

    /// Verifica se um determinado caminho já está na lista de revisão.
    #[must_use]
    pub fn contains_path(&self, path: &str) -> bool {
        self.candidates
            .iter()
            .any(|c| c.path.eq_ignore_ascii_case(path))
    }

    /// Verifica se um determinado ID já está na lista de revisão.
    #[must_use]
    pub fn contains_id(&self, id: u32) -> bool {
        self.candidates.iter().any(|c| c.id == id)
    }

    /// Normaliza os caminhos para detecção de ancestrais independente de barras (`/` ou `\`).
    fn normalize_path(path_str: &str) -> String {
        let mut norm = path_str.replace('/', "\\").to_lowercase();
        while norm.ends_with('\\') && norm.len() > 3 {
            norm.pop();
        }
        norm
    }

    /// Recalcula a cobertura hierárquica entre pastas e descendentes para evitar dupla contagem (TM-09).
    /// Retorna o resumo consolidado das métricas de simulação.
    pub fn recalculate(&mut self) -> ReviewSimulationSummary {
        let n = self.candidates.len();
        let mut covered_by = vec![None; n];

        // 1. Identifica itens cobertos por pastas ancestrais também selecionadas
        for (i, slot) in covered_by.iter_mut().enumerate() {
            let child_path = Self::normalize_path(&self.candidates[i].path);

            for (j, candidate_j) in self.candidates.iter().enumerate() {
                if i == j {
                    continue;
                }
                // Apenas diretórios podem cobrir outros itens
                if !candidate_j.is_dir {
                    continue;
                }

                let parent_path = Self::normalize_path(&candidate_j.path);

                // Se child_path começa com parent_path seguido de barra, é um descendente estrito
                if child_path.len() > parent_path.len()
                    && child_path.starts_with(&parent_path)
                    && (parent_path.ends_with('\\')
                        || child_path.as_bytes()[parent_path.len()] == b'\\')
                {
                    // Guarda o caminho do ancestral direto
                    *slot = Some(candidate_j.path.clone());
                    break;
                }
            }
        }

        // Aplica cobertura calculada
        for (item, cov) in self.candidates.iter_mut().zip(covered_by) {
            item.covered_by_ancestor = cov;
        }

        // 2. Consolidação de métricas
        let mut total_selected_logical_size = 0u64;
        let mut total_estimated_reclaimable_size = 0u64;
        let mut effective_items = 0usize;
        let mut covered_items = 0usize;
        let mut stale_or_missing_count = 0usize;
        let mut personal_content_count = 0usize;
        let mut manual_review_count = 0usize;

        for item in &self.candidates {
            if item.is_stale || !item.exists {
                stale_or_missing_count += 1;
            }
            if item.category == CleanupCategory::PersonalContent {
                personal_content_count += 1;
            }
            if item.category == CleanupCategory::ManualReview {
                manual_review_count += 1;
            }

            if item.covered_by_ancestor.is_some() {
                // Item coberto por ancestral selecionado: NÃO SOMA ao total!
                // Critério de aceite: Selecionar pasta e filho não duplica o total.
                covered_items += 1;
            } else {
                effective_items += 1;
                total_selected_logical_size =
                    total_selected_logical_size.saturating_add(item.logical_size);

                let rec = item
                    .reclaimable_size
                    .or(item.allocated_size)
                    .unwrap_or(item.logical_size);
                total_estimated_reclaimable_size =
                    total_estimated_reclaimable_size.saturating_add(rec);
            }
        }

        ReviewSimulationSummary {
            total_items: n,
            effective_items,
            covered_items,
            total_selected_logical_size,
            total_estimated_reclaimable_size,
            space_already_reclaimed: self.space_already_reclaimed,
            stale_or_missing_count,
            personal_content_count,
            manual_review_count,
        }
    }

    /// Verifica no disco se os itens selecionados ainda existem ou foram modificados externamente.
    pub fn check_stale_all(&mut self) {
        for item in &mut self.candidates {
            match std::fs::metadata(Path::new(&item.path)) {
                Ok(meta) => {
                    item.exists = true;
                    let mtime_sec = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);

                    // Arquivos com mtime ou tamanho alterados são sinalizados como stale
                    item.is_stale = (!item.is_dir && meta.len() != item.logical_size)
                        || (mtime_sec != item.mtime);
                }
                Err(_) => {
                    item.exists = false;
                    item.is_stale = true;
                }
            }
        }
        self.recalculate();
    }

    /// Exporta a lista de revisão para formato CSV padronizado (RFC 4180).
    #[must_use]
    pub fn export_csv(&self) -> String {
        let mut csv = String::with_capacity(self.candidates.len() * 128 + 128);
        csv.push_str("Caminho,Nome,Tipo,Tamanho_Logico,Recuperacao_Estimada,Status_Cobertura,Categoria,Alerta,Status_Disco\n");

        for item in &self.candidates {
            let tipo = if item.is_dir { "Pasta" } else { "Arquivo" };
            let cobertura = match &item.covered_by_ancestor {
                Some(ancestor) => format!("Coberto por '{ancestor}'"),
                None => "Efetivo".to_string(),
            };
            let status_disco = if !item.exists {
                "Inexistente"
            } else if item.is_stale {
                "Modificado"
            } else {
                "OK"
            };

            let rec_val = item
                .reclaimable_size
                .or(item.allocated_size)
                .unwrap_or(item.logical_size);

            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{},{}\n",
                escape_csv_field(&item.path),
                escape_csv_field(&item.name),
                tipo,
                item.logical_size,
                rec_val,
                escape_csv_field(&cobertura),
                escape_csv_field(item.category.label_pt()),
                escape_csv_field(item.alert.as_deref().unwrap_or("")),
                status_disco,
            ));
        }

        csv
    }
}

/// Escapa campos para CSV RFC 4180.
fn escape_csv_field(val: &str) -> String {
    if val.contains(',') || val.contains('"') || val.contains('\n') || val.contains('\r') {
        format!("\"{}\"", val.replace('"', "\"\""))
    } else {
        val.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selecting_parent_and_child_does_not_duplicate_total() {
        // Critério de aceite: Selecionar pasta e filho não duplica o total
        let mut sim = RecoverySimulation::new();

        // 1. Adiciona a pasta 'target' com 1 GB de tamanho
        let parent = ReviewCandidateItem::new(
            1,
            "C:\\Projects\\fastfile\\target".to_string(),
            "target".to_string(),
            true,
            1_073_741_824, // 1 GB
            Some(1_073_741_824),
            Some(1_073_741_824),
            100,
            false,
        );
        sim.add_candidate(parent);

        let sum1 = sim.recalculate();
        assert_eq!(sum1.total_items, 1);
        assert_eq!(sum1.effective_items, 1);
        assert_eq!(sum1.covered_items, 0);
        assert_eq!(sum1.total_selected_logical_size, 1_073_741_824);
        assert_eq!(sum1.total_estimated_reclaimable_size, 1_073_741_824);

        // 2. Adiciona o executável 'target\debug\ff.exe' de 50 MB (descendente direto)
        let child = ReviewCandidateItem::new(
            2,
            "C:\\Projects\\fastfile\\target\\debug\\ff.exe".to_string(),
            "ff.exe".to_string(),
            false,
            52_428_800, // 50 MB
            Some(52_428_800),
            Some(52_428_800),
            100,
            false,
        );
        sim.add_candidate(child);

        let sum2 = sim.recalculate();
        // O total de itens é 2, mas 1 está coberto pelo pai
        assert_eq!(sum2.total_items, 2);
        assert_eq!(sum2.effective_items, 1);
        assert_eq!(sum2.covered_items, 1);

        // GARANTIA: O total NÃO é duplicado! Permanece 1 GB
        assert_eq!(sum2.total_selected_logical_size, 1_073_741_824);
        assert_eq!(sum2.total_estimated_reclaimable_size, 1_073_741_824);

        // O filho possui a indicação explícita de cobertura
        assert_eq!(
            sim.candidates[1].covered_by_ancestor,
            Some("C:\\Projects\\fastfile\\target".to_string())
        );

        // 3. Adiciona um arquivo independente fora da pasta target
        let other = ReviewCandidateItem::new(
            3,
            "C:\\Projects\\fastfile\\README.md".to_string(),
            "README.md".to_string(),
            false,
            10_000,
            Some(12_288),
            Some(12_288),
            100,
            false,
        );
        sim.add_candidate(other);

        let sum3 = sim.recalculate();
        assert_eq!(sum3.total_items, 3);
        assert_eq!(sum3.effective_items, 2);
        assert_eq!(sum3.covered_items, 1);
        assert_eq!(sum3.total_selected_logical_size, 1_073_741_824 + 10_000);
        assert_eq!(
            sum3.total_estimated_reclaimable_size,
            1_073_741_824 + 12_288
        );
    }

    #[test]
    fn test_unknown_and_stale_items_are_flagged() {
        // Critério de aceite: Itens desconhecidos ou desatualizados são sinalizados
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_item.tmp");
        std::fs::write(&file_path, "conteudo inicial").unwrap();

        let path_str = file_path.to_string_lossy().to_string();
        let mut sim = RecoverySimulation::new();

        let item = ReviewCandidateItem::new(
            1,
            path_str.clone(),
            "test_item.tmp".to_string(),
            false,
            16,
            Some(4096),
            Some(4096),
            100,
            false,
        );
        sim.add_candidate(item);

        // Verifica status no disco
        sim.check_stale_all();
        // mtime no disco é diferente de 100, portanto deve ser sinalizado como stale
        assert!(sim.candidates[0].is_stale);
        assert!(sim.candidates[0].exists);

        // Testa item ausente/removido
        std::fs::remove_file(&file_path).unwrap();
        sim.check_stale_all();
        assert!(!sim.candidates[0].exists);
        assert!(sim.candidates[0].is_stale);

        let sum = sim.recalculate();
        assert_eq!(sum.stale_or_missing_count, 1);
    }

    #[test]
    fn test_simulation_remains_strictly_advisory_and_distinguishes_metrics() {
        // Critério de aceite: A lista permanece exclusivamente consultiva
        // Critério de aceite: Valores exibidos distinguem seleção, estimativa e espaço já recuperado
        let mut sim = RecoverySimulation::new();

        let item1 = ReviewCandidateItem::new(
            1,
            "C:\\Data\\file1.bin".to_string(),
            "file1.bin".to_string(),
            false,
            5000,
            Some(8192),
            Some(8192),
            100,
            false,
        );
        sim.add_candidate(item1);

        // Espaço já recuperado é explicitamente distinguido e inicia em zero
        assert_eq!(sim.space_already_reclaimed, 0);

        let sum = sim.recalculate();
        assert_eq!(sum.total_selected_logical_size, 5000);
        assert_eq!(sum.total_estimated_reclaimable_size, 8192);
        assert_eq!(sum.space_already_reclaimed, 0);

        // Exportação CSV mantém todos os campos necessários
        let csv = sim.export_csv();
        assert!(csv.contains("Caminho,Nome,Tipo,Tamanho_Logico,Recuperacao_Estimada"));
        assert!(csv.contains("file1.bin"));
        assert!(csv.contains("5000"));
        assert!(csv.contains("8192"));
    }
}
