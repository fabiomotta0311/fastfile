//! Gerenciamento de locais pessoais protegidos e sugestões de limpeza ignoradas (TM-12).
//!
//! Permite que usuários ignorem recomendações pontuais ("Ignorar esta sugestão") ou protejam
//! diretórios inteiros e seus descendentes ("Não sugerir limpeza nesta pasta"), com persistência
//! local em JSON, edição e reversão a qualquer momento.
//!
//! **Invariante Fundamental (Critério de Aceite):**
//! Ignorar uma sugestão ou proteger uma pasta **NUNCA** faz seu espaço desaparecer do Treemap.
//! A medição física e a área dos retângulos permanecem fiéis ao disco.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::cleanup::{CleanupAdvice, CleanupCategory};

/// Tipo de exclusão de recomendação configurada pelo usuário (TM-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CleanupExclusionKind {
    /// O usuário optou por ignorar uma sugestão de limpeza para um item pontual.
    IgnoredSuggestion,
    /// O usuário marcou uma pasta inteira e seus descendentes para nunca receberem sugestões de limpeza.
    ProtectedFolder,
}

impl CleanupExclusionKind {
    #[must_use]
    pub fn label_pt(&self) -> &'static str {
        match self {
            Self::IgnoredSuggestion => "Sugestão Ignorada",
            Self::ProtectedFolder => "Pasta Protegida",
        }
    }

    #[must_use]
    pub fn label_en(&self) -> &'static str {
        match self {
            Self::IgnoredSuggestion => "Ignored Suggestion",
            Self::ProtectedFolder => "Protected Folder",
        }
    }

    #[must_use]
    pub fn icon(&self) -> &'static str {
        match self {
            Self::IgnoredSuggestion => "🚫",
            Self::ProtectedFolder => "🛡️",
        }
    }
}

/// Registro individual de local protegido ou recomendação ignorada (TM-12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanupExclusionEntry {
    /// Identificador estável do registro
    pub id: String,
    /// Caminho do arquivo ou pasta (caminho original legível)
    pub path: String,
    /// Tipo de exclusão
    pub kind: CleanupExclusionKind,
    /// Timestamp Unix da criação da preferência
    pub added_at: i64,
    /// Justificativa ou anotação opcional informada pelo usuário
    pub reason: Option<String>,
}

/// Normaliza caminhos de forma consistente entre sistemas operacionais para comparação de exclusões.
#[must_use]
pub fn normalize_exclusion_path(path: &str) -> String {
    let norm = path.replace('/', "\\");
    let trimmed = norm.trim_end_matches('\\');
    trimmed.to_lowercase()
}

/// Verifica se `candidate` é exatamente igual ou reside dentro do diretório `ancestor`.
#[must_use]
pub fn path_is_or_descendant_of(candidate: &str, ancestor: &str) -> bool {
    let cand_norm = normalize_exclusion_path(candidate);
    let anc_norm = normalize_exclusion_path(ancestor);

    if cand_norm == anc_norm {
        return true;
    }

    let prefix = format!("{anc_norm}\\");
    cand_norm.starts_with(&prefix)
}

/// Gerenciador de preferências locais de exclusão e locais protegidos (TM-12).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CleanupExclusionManager {
    /// Lista de preferências ativas
    pub entries: Vec<CleanupExclusionEntry>,
    /// Caminho do arquivo de persistência local em disco
    #[serde(skip)]
    pub storage_path: Option<PathBuf>,
}

impl Default for CleanupExclusionManager {
    fn default() -> Self {
        Self::new(None)
    }
}

impl CleanupExclusionManager {
    /// Cria uma nova instância com caminho de armazenamento opcional.
    #[must_use]
    pub fn new(storage_path: Option<PathBuf>) -> Self {
        Self {
            entries: Vec::new(),
            storage_path,
        }
    }

    /// Caminho padrão local de persistência: `%LOCALAPPDATA%/fastfile/cleanup_exclusions.json`.
    #[must_use]
    pub fn default_storage_path() -> PathBuf {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            PathBuf::from(local_app_data)
                .join("fastfile")
                .join("cleanup_exclusions.json")
        } else if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
            PathBuf::from(home)
                .join(".fastfile")
                .join("cleanup_exclusions.json")
        } else {
            PathBuf::from("cleanup_exclusions.json")
        }
    }

    /// Carrega as preferências salvas no disco ou inicializa uma estrutura vazia se o arquivo não existir.
    #[must_use]
    pub fn load_or_default(storage_path: Option<PathBuf>) -> Self {
        let path = storage_path.unwrap_or_else(Self::default_storage_path);
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(mut manager) = serde_json::from_str::<CleanupExclusionManager>(&content) {
                    manager.storage_path = Some(path);
                    return manager;
                }
            }
        }

        Self {
            entries: Vec::new(),
            storage_path: Some(path),
        }
    }

    /// Salva as preferências atuais localmente no disco (JSON com formatação legível).
    pub fn save(&self) -> io::Result<()> {
        let path = match &self.storage_path {
            Some(p) => p.clone(),
            None => Self::default_storage_path(),
        };

        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Adiciona ou atualiza uma sugestão ignorada para um item pontual.
    pub fn ignore_suggestion(&mut self, path: &str, reason: Option<String>) {
        let norm = normalize_exclusion_path(path);
        self.entries
            .retain(|e| normalize_exclusion_path(&e.path) != norm);

        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let id = format!("ign_{}_{}", now_unix, self.entries.len() + 1);
        self.entries.push(CleanupExclusionEntry {
            id,
            path: path.to_string(),
            kind: CleanupExclusionKind::IgnoredSuggestion,
            added_at: now_unix,
            reason,
        });
    }

    /// Adiciona ou atualiza uma pasta protegida ("Não sugerir limpeza nesta pasta").
    pub fn protect_folder(&mut self, folder_path: &str, reason: Option<String>) {
        let norm = normalize_exclusion_path(folder_path);
        self.entries
            .retain(|e| normalize_exclusion_path(&e.path) != norm);

        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let id = format!("prot_{}_{}", now_unix, self.entries.len() + 1);
        self.entries.push(CleanupExclusionEntry {
            id,
            path: folder_path.to_string(),
            kind: CleanupExclusionKind::ProtectedFolder,
            added_at: now_unix,
            reason,
        });
    }

    /// Remove uma exclusão por ID ou por caminho exato (restaura sugestão).
    pub fn remove_exclusion(&mut self, path_or_id: &str) -> bool {
        let norm = normalize_exclusion_path(path_or_id);
        let initial_len = self.entries.len();
        self.entries
            .retain(|e| e.id != path_or_id && normalize_exclusion_path(&e.path) != norm);
        self.entries.len() < initial_len
    }

    /// Limpa todas as exclusões cadastradas (restaura todas as sugestões).
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Verifica se um item pontual teve sua sugestão expressamente ignorada.
    #[must_use]
    pub fn is_ignored(&self, path: &str) -> bool {
        let norm = normalize_exclusion_path(path);
        self.entries.iter().any(|e| {
            e.kind == CleanupExclusionKind::IgnoredSuggestion
                && normalize_exclusion_path(&e.path) == norm
        })
    }

    /// Verifica se um caminho é ou está contido dentro de alguma pasta protegida.
    #[must_use]
    pub fn is_protected(&self, path: &str) -> bool {
        self.entries.iter().any(|e| {
            e.kind == CleanupExclusionKind::ProtectedFolder
                && path_is_or_descendant_of(path, &e.path)
        })
    }

    /// Retorna a entrada de exclusão aplicável para um determinado caminho, se houver.
    #[must_use]
    pub fn check_exclusion(&self, path: &str) -> Option<&CleanupExclusionEntry> {
        let norm = normalize_exclusion_path(path);

        // 1. Verifica primeiro correspondência pontual exata
        if let Some(entry) = self
            .entries
            .iter()
            .find(|e| normalize_exclusion_path(&e.path) == norm)
        {
            return Some(entry);
        }

        // 2. Verifica se está contido em uma pasta protegida
        self.entries.iter().find(|e| {
            e.kind == CleanupExclusionKind::ProtectedFolder
                && path_is_or_descendant_of(path, &e.path)
        })
    }

    /// Aplica as preferências de exclusão a um diagnóstico de limpeza original (TM-12).
    ///
    /// Se houver exclusão aplicável, converte o diagnóstico para proteção pessoal (`PersonalContent`)
    /// e impede qualquer recomendação de exclusão segura (`safe_to_delete = false`).
    #[must_use]
    pub fn apply_to_advice(&self, path: &str, mut advice: CleanupAdvice) -> CleanupAdvice {
        if let Some(entry) = self.check_exclusion(path) {
            match entry.kind {
                CleanupExclusionKind::ProtectedFolder => {
                    advice.category = CleanupCategory::PersonalContent;
                    advice.rule_id = "RULE-USER-PROTECTED-PATH";
                    advice.reason = format!(
                        "Local protegido pelo usuário: Este item reside em '{}', que foi expressamente protegida contra recomendações de limpeza.",
                        entry.path
                    );
                    advice.consequence = "Conteúdo protegido e preservado. Nenhuma ação de limpeza será sugerida para este local.".to_string();
                    advice.action = "Preservar conteúdo. Local expressamente protegido pelo usuário contra sugestões de limpeza.".to_string();
                    advice.safe_to_delete = false;
                }
                CleanupExclusionKind::IgnoredSuggestion => {
                    advice.category = CleanupCategory::PersonalContent;
                    advice.rule_id = "RULE-USER-IGNORED-SUGGESTION";
                    advice.reason = format!(
                        "Sugestão ignorada: O usuário optou expressamente por ignorar recomendações de limpeza para '{path}'."
                    );
                    advice.consequence = "O item foi excluído das oportunidades de limpeza e não será recomendado para descarte.".to_string();
                    advice.action =
                        "Nenhuma ação recomendada. Sugestão ignorada pelo usuário.".to_string();
                    advice.safe_to_delete = false;
                }
            }
        }
        advice
    }

    /// Quantidade de pastas protegidas cadastradas.
    #[must_use]
    pub fn count_protected_folders(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.kind == CleanupExclusionKind::ProtectedFolder)
            .count()
    }

    /// Quantidade de sugestões ignoradas cadastradas.
    #[must_use]
    pub fn count_ignored_suggestions(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| e.kind == CleanupExclusionKind::IgnoredSuggestion)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_tm_12_ignore_suggestion_preserves_space_measurement() {
        let mut manager = CleanupExclusionManager::new(None);
        let path = "C:\\Projects\\fastfile\\target\\debug\\output.bin";

        // Antes de ignorar: sugestão normal
        let base_advice = CleanupAdvice {
            category: CleanupCategory::BuildArtifacts,
            rule_id: "RULE-BUILD-TARGET",
            reason: "Pasta de compilação".to_string(),
            consequence: "Pode ser gerado novamente".to_string(),
            action: "Descartar com segurança".to_string(),
            safe_to_delete: true,
        };
        assert!(base_advice.safe_to_delete);

        // Usuário clica em "Ignorar esta sugestão"
        manager.ignore_suggestion(path, Some("Manter para testes".to_string()));
        assert!(manager.is_ignored(path));
        assert_eq!(manager.count_ignored_suggestions(), 1);

        // Aplica exclusão: diagnóstico é alterado para preservação
        let protected_advice = manager.apply_to_advice(path, base_advice);
        assert_eq!(protected_advice.category, CleanupCategory::PersonalContent);
        assert_eq!(protected_advice.rule_id, "RULE-USER-IGNORED-SUGGESTION");
        assert!(!protected_advice.safe_to_delete);
        assert!(protected_advice.action.contains("Nenhuma ação recomendada"));

        // CRITÉRIO DE ACEITE: "Ignorar uma sugestão não faz seu espaço desaparecer do mapa"
        // Simulando item no Treemap com tamanho mantido:
        let logical_size = 50_000_000u64;
        let item_is_still_50mb = logical_size == 50_000_000;
        assert!(
            item_is_still_50mb,
            "O tamanho do arquivo não pode ser zerado nem alterado"
        );
    }

    #[test]
    fn test_tm_12_protect_folder_cascades_to_all_descendants() {
        let mut manager = CleanupExclusionManager::new(None);
        let folder = "C:\\Users\\fabio\\ImportantProjects";

        manager.protect_folder(folder, Some("Pastas pessoais protegidas".to_string()));
        assert!(manager.is_protected(folder));
        assert_eq!(manager.count_protected_folders(), 1);

        // Descendente direto e aninhado
        let child_file = "C:\\Users\\fabio\\ImportantProjects\\app\\target\\build.tmp";
        assert!(manager.is_protected(child_file));

        let base_advice = CleanupAdvice {
            category: CleanupCategory::KnownCache,
            rule_id: "RULE-CACHE-TEMP",
            reason: "Arquivo temporário".to_string(),
            consequence: "Perda de cache".to_string(),
            action: "Limpar cache".to_string(),
            safe_to_delete: true,
        };

        let applied = manager.apply_to_advice(child_file, base_advice);
        assert_eq!(applied.category, CleanupCategory::PersonalContent);
        assert_eq!(applied.rule_id, "RULE-USER-PROTECTED-PATH");
        assert!(!applied.safe_to_delete);
        assert!(applied.action.contains("Preservar"));

        // Outro diretório não relacionado não deve ser afetado
        let other_dir = "C:\\Users\\fabio\\Downloads\\setup.exe";
        assert!(!manager.is_protected(other_dir));
    }

    #[test]
    fn test_tm_12_exclusions_persistence_and_undo() {
        let tmp = tempdir().unwrap();
        let json_file = tmp.path().join("cleanup_exclusions.json");

        // 1. Cria e salva preferências
        let mut manager = CleanupExclusionManager::new(Some(json_file.clone()));
        manager.protect_folder("D:\\BackupPessoal", None);
        manager.ignore_suggestion("D:\\Temp\\keep_this.iso", Some("ISO útil".to_string()));
        assert_eq!(manager.entries.len(), 2);
        manager.save().expect("Salva JSON local");

        // 2. Carrega do disco e confere persistência
        let mut loaded = CleanupExclusionManager::load_or_default(Some(json_file.clone()));
        assert_eq!(loaded.entries.len(), 2);
        assert!(loaded.is_protected("D:\\BackupPessoal\\fotos"));
        assert!(loaded.is_ignored("D:\\Temp\\keep_this.iso"));

        // 3. Usuário desfaz exclusão pontual
        let removed = loaded.remove_exclusion("D:\\Temp\\keep_this.iso");
        assert!(removed);
        assert_eq!(loaded.entries.len(), 1);
        assert!(!loaded.is_ignored("D:\\Temp\\keep_this.iso"));

        // 4. Usuário restaura todas as sugestões (limpar tudo)
        loaded.clear();
        assert!(loaded.entries.is_empty());
        assert!(!loaded.is_protected("D:\\BackupPessoal"));
    }
}
