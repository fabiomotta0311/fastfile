//! Lista de revisão, simulação de recuperação de espaço e metas com alternativas responsáveis (TM-09, TM-13).
//!
//! Permite selecionar e gerenciar itens candidatos a recuperação sem realizar qualquer
//! ação destrutiva (modo exclusivamente consultivo), deduplicando automaticamente tamanhos
//! entre pastas e descendentes, alertando sobre itens desatualizados ou sensíveis,
//! projetando o progresso em relação a metas de recuperação definidas pelo usuário
//! e orientando alternativas responsáveis (como liberar espaço local em nuvem sem apagar da nuvem,
//! mover para storage secundário, compactar/arquivar ou limpar via aplicação).

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::cleanup::{analyze_cleanup_opportunity, CleanupAdvice, CleanupCategory};

/// Meta de recuperação de espaço em disco definida pelo usuário (TM-13).
///
/// Permite planejar a recuperação estipulando um tamanho alvo (ex: 5 GB, 500 MB).
/// A meta calcula projeções de progresso e faltante sobre a simulação consultiva,
/// mas NUNCA provoca seleção ou exclusão automática de arquivos (critério de aceite).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RecoveryGoal {
    /// Tamanho alvo desejado para recuperação em bytes (ou `None` se desativada).
    pub target_bytes: Option<u64>,
}

impl RecoveryGoal {
    /// Cria uma nova meta de recuperação.
    #[must_use]
    pub fn new(target_bytes: Option<u64>) -> Self {
        Self { target_bytes }
    }

    /// Cria uma meta ativa a partir de bytes.
    #[must_use]
    pub fn from_bytes(bytes: u64) -> Self {
        Self {
            target_bytes: Some(bytes),
        }
    }

    /// Indica se a meta está definida e ativa com valor maior que zero.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.target_bytes.is_some_and(|b| b > 0)
    }

    /// Retorna o valor alvo em bytes da meta (se houver).
    #[must_use]
    pub fn target_bytes(&self) -> Option<u64> {
        self.target_bytes
    }

    /// Retorna a fração atingida em relação à meta (0.0 a 1.0+).
    #[must_use]
    pub fn progress_ratio(&self, estimated_reclaimable_bytes: u64) -> f64 {
        match self.target_bytes {
            Some(target) if target > 0 => estimated_reclaimable_bytes as f64 / target as f64,
            _ => 0.0,
        }
    }

    /// Retorna o percentual atingido (0.0% a 100%+).
    #[must_use]
    pub fn progress_percentage(&self, estimated_reclaimable_bytes: u64) -> f64 {
        self.progress_ratio(estimated_reclaimable_bytes) * 100.0
    }

    /// Retorna quantos bytes ainda faltam para atingir a meta.
    #[must_use]
    pub fn remaining_bytes(&self, estimated_reclaimable_bytes: u64) -> u64 {
        self.target_bytes
            .unwrap_or(0)
            .saturating_sub(estimated_reclaimable_bytes)
    }

    /// Verifica se a meta de recuperação foi alcançada.
    #[must_use]
    pub fn is_reached(&self, estimated_reclaimable_bytes: u64) -> bool {
        match self.target_bytes {
            Some(target) if target > 0 => estimated_reclaimable_bytes >= target,
            _ => false,
        }
    }

    /// Analisa uma string amigável informada pelo usuário (ex: "5GB", "500 MB", "2.5 GiB", "1048576").
    #[must_use]
    pub fn parse_input(input: &str) -> Option<u64> {
        let s = input.trim();
        if s.is_empty() {
            return None;
        }

        if let Ok(val) = s.parse::<u64>() {
            return Some(val);
        }

        let mut num_part = String::new();
        let mut unit_part = String::new();
        for c in s.chars() {
            if c.is_ascii_digit() || c == '.' || c == ',' {
                num_part.push(if c == ',' { '.' } else { c });
            } else if !c.is_whitespace() {
                unit_part.push(c);
            }
        }

        let val: f64 = num_part.parse().ok()?;
        if val <= 0.0 {
            return None;
        }

        let unit = unit_part.to_lowercase();
        let multiplier: f64 = match unit.as_str() {
            "b" | "bytes" | "" => 1.0,
            "k" | "kb" | "kib" => 1024.0,
            "m" | "mb" | "mib" => 1024.0 * 1024.0,
            "g" | "gb" | "gib" => 1024.0 * 1024.0 * 1024.0,
            "t" | "tb" | "tib" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
            _ => return None,
        };

        let total = val * multiplier;
        if total.is_nan() || total.is_infinite() || total > (u64::MAX as f64) {
            None
        } else {
            Some(total as u64)
        }
    }
}

/// Alternativas orientadas à recuperação de espaço e descarte responsável (TM-13).
///
/// Distingue ações além da exclusão simples, como mover para armazenamento secundário,
/// compactar/arquivar, desinstalar via sistema, limpar pela própria aplicação ou
/// liberar cópia local de nuvem (tornar somente online) sem apagar da nuvem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecoveryAlternative {
    /// Arquivo presente em pasta sincronizada de nuvem (OneDrive, Dropbox, Google Drive, iCloud).
    /// Critério essencial de TM-13:
    /// 'Liberar cópia local' (Free up space) libera os bytes em disco mantendo o arquivo na nuvem.
    /// 'Excluir arquivo' apaga o arquivo também na nuvem em todos os dispositivos sincronizados.
    FreeCloudSpace {
        provider: String,
        cloud_warning: String,
    },
    /// Mover itens pessoais ou arquivos grandes para armazenamento secundário/externo (SSD externo, HDD, NAS).
    MoveToSecondaryStorage { target_hint: String },
    /// Compactar ou arquivar projetos antigos, relatórios ou históricos em formato comprimido.
    ArchiveOrCompress { suggestion: String },
    /// Limpar caches, dependências ou temporários através da própria ferramenta/aplicação proprietária.
    CleanViaApplication {
        app_name: String,
        command_or_instruction: String,
    },
    /// Desinstalar programa pelo painel de controle ou configurações do sistema operacional.
    UninstallViaSystem { app_name: String },
    /// Exclusão consciente do arquivo após revisão manual.
    DirectDeleteReview,
    /// Revisão manual necessária (item desconhecido ou ambíguo).
    ManualReviewRequired,
}

impl RecoveryAlternative {
    /// Rótulo curto com ícone representativo para exibição em tabelas e listas.
    #[must_use]
    pub fn short_label(&self) -> &'static str {
        match self {
            Self::FreeCloudSpace { .. } => "☁ Nuvem: Liberar Espaço Local (Mantém na Nuvem)",
            Self::MoveToSecondaryStorage { .. } => "📦 Mover para Armazenamento Secundário",
            Self::ArchiveOrCompress { .. } => "🗜 Compactar ou Arquivar",
            Self::CleanViaApplication { .. } => "⚙ Limpar via Aplicação Proprietária",
            Self::UninstallViaSystem { .. } => "🛡 Desinstalar pelo Sistema",
            Self::DirectDeleteReview => "🗑 Exclusão Manual Consciente",
            Self::ManualReviewRequired => "🔍 Revisão Manual Necessária",
        }
    }

    /// Ícone representativo.
    #[must_use]
    pub fn icon(&self) -> &'static str {
        match self {
            Self::FreeCloudSpace { .. } => "☁",
            Self::MoveToSecondaryStorage { .. } => "📦",
            Self::ArchiveOrCompress { .. } => "🗜",
            Self::CleanViaApplication { .. } => "⚙",
            Self::UninstallViaSystem { .. } => "🛡",
            Self::DirectDeleteReview => "🗑",
            Self::ManualReviewRequired => "🔍",
        }
    }

    /// Descrição detalhada da alternativa e orientações contextuais.
    #[must_use]
    pub fn description(&self) -> String {
        match self {
            Self::FreeCloudSpace {
                provider,
                cloud_warning,
            } => {
                format!("{provider}: {cloud_warning}")
            }
            Self::MoveToSecondaryStorage { target_hint } => target_hint.clone(),
            Self::ArchiveOrCompress { suggestion } => suggestion.clone(),
            Self::CleanViaApplication {
                app_name,
                command_or_instruction,
            } => {
                format!("{app_name}: {command_or_instruction}")
            }
            Self::UninstallViaSystem { app_name } => {
                format!("Desinstalar '{app_name}' através de Configurações > Aplicativos Instalados do Windows.")
            }
            Self::DirectDeleteReview => {
                "Item selecionado pelo usuário para exclusão após conferência manual.".to_string()
            }
            Self::ManualReviewRequired => {
                "Não há regra automatizada para este item. Avalie o conteúdo antes de decidir a ação.".to_string()
            }
        }
    }

    /// Indica se o item pertence a serviço de nuvem sincronizado.
    #[must_use]
    pub fn is_cloud(&self) -> bool {
        matches!(self, Self::FreeCloudSpace { .. })
    }

    /// Indica se o item é gerenciado por uma aplicação externa.
    #[must_use]
    pub fn is_app_managed(&self) -> bool {
        matches!(
            self,
            Self::CleanViaApplication { .. } | Self::UninstallViaSystem { .. }
        )
    }
}

/// Infere a alternativa mais responsável com base em caminho, nome, tipo e categoria diagnosticada (TM-13).
#[must_use]
pub fn infer_recovery_alternative(
    path: &str,
    name: &str,
    is_dir: bool,
    category: CleanupCategory,
) -> RecoveryAlternative {
    let lower_path = path.to_lowercase();
    let lower_name = name.to_lowercase();

    // 1. Detecção de Pastas Sincronizadas de Nuvem (Critério TM-13: Distinção entre liberar cópia local e excluir)
    let cloud_provider = if lower_path.contains("onedrive") {
        Some("Microsoft OneDrive")
    } else if lower_path.contains("dropbox") {
        Some("Dropbox")
    } else if lower_path.contains("google drive") || lower_path.contains("googledrive") {
        Some("Google Drive")
    } else if lower_path.contains("iclouddrive") || lower_path.contains("icloud") {
        Some("iCloud Drive")
    } else {
        None
    };

    if let Some(provider) = cloud_provider {
        return RecoveryAlternative::FreeCloudSpace {
            provider: provider.to_string(),
            cloud_warning: "Atenção: Excluir este arquivo sincronizado irá removê-lo também da sua nuvem e de outros dispositivos sincronizados. Para apenas liberar espaço no disco local mantendo o arquivo seguro na nuvem, utilize a opção 'Liberar espaço' (somente online) do cliente de sincronização.".to_string(),
        };
    }

    // 2. Orientações contextuais para Artefatos de Build e Aplicações Conhecidas
    match category {
        CleanupCategory::BuildArtifacts => {
            if lower_name == "target" || lower_path.contains("\\target") {
                RecoveryAlternative::CleanViaApplication {
                    app_name: "Rust / Cargo".to_string(),
                    command_or_instruction: "Execute 'cargo clean' na raiz do projeto para remover a pasta target e recompilar sem resíduos quando necessário.".to_string(),
                }
            } else if lower_name == "node_modules" || lower_path.contains("\\node_modules") {
                RecoveryAlternative::CleanViaApplication {
                    app_name: "Node.js / npm".to_string(),
                    command_or_instruction: "Execute 'npm prune' ou remova via ferramentas do gerenciador de pacotes (ex: 'npm ci' recompõe as dependências).".to_string(),
                }
            } else if lower_name == ".gradle" || lower_path.contains("\\.gradle") {
                RecoveryAlternative::CleanViaApplication {
                    app_name: "Gradle".to_string(),
                    command_or_instruction: "Limpe através de './gradlew clean' ou encerre daemons com './gradlew --stop' antes de limpar o cache.".to_string(),
                }
            } else if lower_name == "__pycache__" || lower_name == ".venv" || lower_name == "venv" {
                RecoveryAlternative::CleanViaApplication {
                    app_name: "Python".to_string(),
                    command_or_instruction: "Utilize 'pip cache purge' ou recrie o ambiente virtual (venv) quando necessário.".to_string(),
                }
            } else {
                RecoveryAlternative::CleanViaApplication {
                    app_name: "Ferramenta de Build".to_string(),
                    command_or_instruction: "Utilize o comando de limpeza oficial do compilador/gerenciador de dependências.".to_string(),
                }
            }
        }
        CleanupCategory::KnownCache => {
            if lower_path.contains("google\\chrome")
                || lower_path.contains("microsoft\\edge")
                || lower_path.contains("mozilla\\firefox")
            {
                RecoveryAlternative::CleanViaApplication {
                    app_name: "Navegador Web".to_string(),
                    command_or_instruction: "Limpe o histórico e arquivos temporários pelas configurações do navegador para preservar preferências e logins ativos.".to_string(),
                }
            } else if lower_path.contains("\\temp") || lower_name == "temp" {
                RecoveryAlternative::CleanViaApplication {
                    app_name: "Windows Temp".to_string(),
                    command_or_instruction: "Pode ser limpo com segurança ou através do utilitário 'Sensor de Armazenamento' (Storage Sense) do Windows.".to_string(),
                }
            } else {
                RecoveryAlternative::CleanViaApplication {
                    app_name: "Aplicação Proprietária".to_string(),
                    command_or_instruction: "Verifique se a aplicação oferece opção dedicada de limpeza de cache em suas preferências.".to_string(),
                }
            }
        }
        CleanupCategory::ApplicationData => {
            if lower_name == ".git" || lower_path.contains("\\.git") {
                RecoveryAlternative::CleanViaApplication {
                    app_name: "Git".to_string(),
                    command_or_instruction: "Execute 'git gc --prune=now' para compactar a base de objetos sem perder commits locais.".to_string(),
                }
            } else if lower_path.contains("docker") {
                RecoveryAlternative::CleanViaApplication {
                    app_name: "Docker Desktop".to_string(),
                    command_or_instruction: "Execute 'docker system prune' para remover imagens e contêineres órfãos sem corromper o daemon do WSL/Docker.".to_string(),
                }
            } else if !is_dir
                && matches!(
                    Path::new(name)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or(""),
                    "mdf" | "ldf" | "sqlite" | "sqlite3" | "db"
                )
            {
                RecoveryAlternative::ArchiveOrCompress {
                    suggestion: "Realize backup e compacte bases históricas em arquivo .zip/.7z antes de descartar.".to_string(),
                }
            } else {
                RecoveryAlternative::CleanViaApplication {
                    app_name: "Software Proprietário".to_string(),
                    command_or_instruction: "Gerencie dados e preferências através do próprio software para evitar perda de configurações ativas.".to_string(),
                }
            }
        }
        CleanupCategory::SystemProtected => RecoveryAlternative::CleanViaApplication {
            app_name: "Sistema Operacional Windows".to_string(),
            command_or_instruction: "Não altere manualmente. Utilize utilitários oficiais como a 'Limpeza de Disco' (cleanmgr) ou 'Sensor de Armazenamento'.".to_string(),
        },
        CleanupCategory::InstallerOrDownload => {
            let ext = Path::new(name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if matches!(ext.as_str(), "msi" | "exe") && lower_path.contains("program files") {
                RecoveryAlternative::UninstallViaSystem {
                    app_name: name.to_string(),
                }
            } else if matches!(ext.as_str(), "iso" | "img" | "vhd" | "vhdx") {
                RecoveryAlternative::MoveToSecondaryStorage {
                    target_hint: "Mova imagens de disco grandes para armazenamento externo secundário ou NAS.".to_string(),
                }
            } else {
                RecoveryAlternative::DirectDeleteReview
            }
        }
        CleanupCategory::PersonalContent => RecoveryAlternative::MoveToSecondaryStorage {
            target_hint: "Mova fotos, vídeos ou documentos pessoais para uma unidade secundária, SSD externo ou backup na nuvem. Nunca descarte sem verificar cópias.".to_string(),
        },
        CleanupCategory::ManualReview => {
            if lower_name == "target" || lower_path.contains("\\target") {
                RecoveryAlternative::CleanViaApplication {
                    app_name: "Rust / Cargo".to_string(),
                    command_or_instruction: "Se pertencer a um projeto Rust, execute 'cargo clean' para limpar a pasta target com segurança.".to_string(),
                }
            } else if lower_name == "node_modules" || lower_path.contains("\\node_modules") {
                RecoveryAlternative::CleanViaApplication {
                    app_name: "Node.js / npm".to_string(),
                    command_or_instruction: "Se pertencer a um projeto Node.js, execute 'npm prune' ou gerencie pelo gerenciador de pacotes.".to_string(),
                }
            } else if is_dir {
                RecoveryAlternative::ArchiveOrCompress {
                    suggestion: "Se a pasta contiver projetos antigos ou arquivos históricos, compacte em .zip antes de liberar espaço.".to_string(),
                }
            } else {
                RecoveryAlternative::ManualReviewRequired
            }
        }
    }
}

/// Representa um arquivo ou diretório marcado para simulação de recuperação (TM-09, TM-13).
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
    /// Alternativa recomendada de recuperação além de exclusão (TM-13).
    pub alternative: RecoveryAlternative,
}

impl ReviewCandidateItem {
    /// Cria um candidato de revisão a partir de dados básicos e realiza a classificação explicável (TM-07, TM-13).
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

        let alternative = infer_recovery_alternative(&path, &name, is_dir, advice.category);

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
            alternative,
        }
    }

    /// Sobrescreve a alternativa com um valor explícito.
    #[must_use]
    pub fn with_alternative(mut self, alternative: RecoveryAlternative) -> Self {
        self.alternative = alternative;
        self
    }
}

/// Resumo agregado da simulação de recuperação de espaço (TM-09, TM-13).
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
    /// Quantidade de itens com recuperação física desconhecida (TM-13).
    pub unknown_reclaimable_count: usize,
    /// Quantidade de itens em pastas sincronizadas de nuvem (TM-13).
    pub cloud_copies_count: usize,
    /// Quantidade de itens gerenciados por aplicações conhecidas (TM-13).
    pub app_managed_count: usize,
}

/// Gerenciador consultivo da lista de revisão e simulação de recuperação (TM-09, TM-13).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecoverySimulation {
    /// Itens atualmente marcados na lista de revisão.
    pub candidates: Vec<ReviewCandidateItem>,
    /// Espaço já recuperado fisicamente (mantido para clareza e separação de medidas).
    pub space_already_reclaimed: u64,
    /// Meta de recuperação definida pelo usuário (TM-13).
    pub goal: RecoveryGoal,
}

impl RecoverySimulation {
    /// Cria uma nova simulação vazia.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Define uma meta de recuperação. Não altera nem seleciona itens automaticamente (TM-13).
    pub fn set_goal(&mut self, goal: RecoveryGoal) {
        self.goal = goal;
    }

    /// Remove a meta de recuperação ativa.
    pub fn clear_goal(&mut self) {
        self.goal = RecoveryGoal::default();
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
        let mut unknown_reclaimable_count = 0usize;
        let mut cloud_copies_count = 0usize;
        let mut app_managed_count = 0usize;

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
            if item.reclaimable_size.is_none() && item.allocated_size.is_none() {
                unknown_reclaimable_count += 1;
            }
            if item.alternative.is_cloud() {
                cloud_copies_count += 1;
            }
            if item.alternative.is_app_managed() {
                app_managed_count += 1;
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
            unknown_reclaimable_count,
            cloud_copies_count,
            app_managed_count,
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
        let mut csv = String::with_capacity(self.candidates.len() * 160 + 160);
        csv.push_str("Caminho,Nome,Tipo,Tamanho_Logico,Recuperacao_Estimada,Status_Cobertura,Categoria,Alternativa_Sugerida,Detalhes_Alternativa,Alerta,Status_Disco\n");

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
                .map(|v| v.to_string())
                .unwrap_or_else(|| "Desconhecido".to_string());

            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{}\n",
                escape_csv_field(&item.path),
                escape_csv_field(&item.name),
                tipo,
                item.logical_size,
                rec_val,
                escape_csv_field(&cobertura),
                escape_csv_field(item.category.label_pt()),
                escape_csv_field(item.alternative.short_label()),
                escape_csv_field(&item.alternative.description()),
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

    #[test]
    fn test_tm13_recovery_goal_progress_and_completion() {
        // Critério TM-13: Meta de recuperação definida pelo usuário e progresso estimado
        let goal = RecoveryGoal::from_bytes(10_000_000); // 10 MB
        assert!(goal.is_active());
        assert_eq!(goal.target_bytes(), Some(10_000_000));

        // 0% atingido
        assert_eq!(goal.progress_ratio(0), 0.0);
        assert_eq!(goal.remaining_bytes(0), 10_000_000);
        assert!(!goal.is_reached(0));

        // 50% atingido (5 MB)
        assert!((goal.progress_ratio(5_000_000) - 0.5).abs() < 0.001);
        assert!((goal.progress_percentage(5_000_000) - 50.0).abs() < 0.001);
        assert_eq!(goal.remaining_bytes(5_000_000), 5_000_000);
        assert!(!goal.is_reached(5_000_000));

        // 100% atingido (10 MB)
        assert!((goal.progress_ratio(10_000_000) - 1.0).abs() < 0.001);
        assert_eq!(goal.remaining_bytes(10_000_000), 0);
        assert!(goal.is_reached(10_000_000));

        // Ultrapassou a meta (12 MB)
        assert!(goal.progress_ratio(12_000_000) > 1.0);
        assert_eq!(goal.remaining_bytes(12_000_000), 0);
        assert!(goal.is_reached(12_000_000));
    }

    #[test]
    fn test_tm13_goal_never_causes_automatic_selection_or_deletion() {
        // Critério de aceite TM-13: A meta não provoca seleção ou exclusão automática.
        let mut sim = RecoverySimulation::new();

        // Lista inicialmente vazia
        assert!(sim.candidates.is_empty());
        assert_eq!(sim.space_already_reclaimed, 0);

        // Define uma meta de 50 GB
        sim.set_goal(RecoveryGoal::from_bytes(50 * 1024 * 1024 * 1024));

        // A simulação continua com 0 itens! Nada foi selecionado automaticamente.
        assert!(sim.candidates.is_empty());
        let summary = sim.recalculate();
        assert_eq!(summary.total_items, 0);
        assert_eq!(summary.total_estimated_reclaimable_size, 0);
        assert_eq!(summary.space_already_reclaimed, 0);

        // Adiciona um item manualmente
        let item = ReviewCandidateItem::new(
            1,
            "C:\\Temp\\manual_item.log".to_string(),
            "manual_item.log".to_string(),
            false,
            1000,
            Some(4096),
            Some(4096),
            100,
            false,
        );
        sim.add_candidate(item);
        assert_eq!(sim.candidates.len(), 1);

        // Altera a meta para outro valor (ex: 100 GB)
        sim.set_goal(RecoveryGoal::from_bytes(100 * 1024 * 1024 * 1024));
        // A lista permanece exatamente com o único item selecionado pelo usuário
        assert_eq!(sim.candidates.len(), 1);
        assert_eq!(sim.candidates[0].path, "C:\\Temp\\manual_item.log");
        assert_eq!(sim.space_already_reclaimed, 0);
    }

    #[test]
    fn test_tm13_distinction_between_free_local_cloud_and_delete_cloud_file() {
        // Critério de aceite TM-13: Distinção entre liberar cópia local de nuvem e excluir o arquivo sincronizado.
        let onedrive_item = ReviewCandidateItem::new(
            10,
            "C:\\Users\\fabio\\OneDrive\\Projetos\\big_presentation.pptx".to_string(),
            "big_presentation.pptx".to_string(),
            false,
            500_000_000,
            Some(500_000_000),
            Some(500_000_000),
            100,
            false,
        );

        assert!(onedrive_item.alternative.is_cloud());
        assert_eq!(
            onedrive_item.alternative.short_label(),
            "☁ Nuvem: Liberar Espaço Local (Mantém na Nuvem)"
        );

        let desc = onedrive_item.alternative.description();
        // A orientação deve alertar claramente que excluir apaga da nuvem também e orientar a liberar espaço local
        assert!(desc.contains(
            "Atenção: Excluir este arquivo sincronizado irá removê-lo também da sua nuvem"
        ));
        assert!(desc.contains("Liberar espaço"));
    }

    #[test]
    fn test_tm13_application_managed_data_receives_contextual_guidance() {
        // Critério de aceite TM-13: Dados gerenciados por aplicações recebem orientação contextual.
        // 1. Rust / Cargo target
        let cargo_target = ReviewCandidateItem::new(
            20,
            "C:\\Projects\\fastfile\\target".to_string(),
            "target".to_string(),
            true,
            2_000_000_000,
            Some(2_000_000_000),
            Some(2_000_000_000),
            100,
            false,
        );
        assert!(cargo_target.alternative.is_app_managed());
        assert!(cargo_target
            .alternative
            .description()
            .contains("cargo clean"));

        // 2. Repositório Git (.git)
        let git_dir = ReviewCandidateItem::new(
            21,
            "C:\\Projects\\fastfile\\.git".to_string(),
            ".git".to_string(),
            true,
            50_000_000,
            Some(50_000_000),
            Some(50_000_000),
            100,
            false,
        );
        assert!(git_dir.alternative.is_app_managed());
        assert!(git_dir.alternative.description().contains("git gc"));

        // 3. Node modules
        let node_modules = ReviewCandidateItem::new(
            22,
            "C:\\Projects\\webapp\\node_modules".to_string(),
            "node_modules".to_string(),
            true,
            300_000_000,
            Some(300_000_000),
            Some(300_000_000),
            100,
            false,
        );
        assert!(node_modules.alternative.is_app_managed());
        assert!(node_modules.alternative.description().contains("npm"));
    }

    #[test]
    fn test_tm13_unknown_reclaimable_appears_as_unknown() {
        // Critério de aceite TM-13: Recuperação desconhecida aparece como desconhecida.
        let mut sim = RecoverySimulation::new();

        let unknown_item = ReviewCandidateItem::new(
            30,
            "C:\\Data\\mystery.dat".to_string(),
            "mystery.dat".to_string(),
            false,
            1_000_000,
            None, // allocated_size desconhecido
            None, // reclaimable_size desconhecido
            100,
            false,
        );

        sim.add_candidate(unknown_item);
        let summary = sim.recalculate();
        assert_eq!(summary.unknown_reclaimable_count, 1);

        // No CSV gerado, a recuperação estimada deve conter "Desconhecido" explicitamente,
        // e nunca 0 ou valor arbitrário
        let csv = sim.export_csv();
        assert!(csv.contains("Desconhecido"));
    }

    #[test]
    fn test_tm13_goal_input_parsing() {
        // Teste de parsing flexível de metas de recuperação
        assert_eq!(
            RecoveryGoal::parse_input("5GB"),
            Some(5 * 1024 * 1024 * 1024)
        );
        assert_eq!(RecoveryGoal::parse_input("500 MB"), Some(500 * 1024 * 1024));
        assert_eq!(
            RecoveryGoal::parse_input("2.5 GiB"),
            Some((2.5 * 1024.0 * 1024.0 * 1024.0) as u64)
        );
        assert_eq!(RecoveryGoal::parse_input("1024 KB"), Some(1024 * 1024));
        assert_eq!(RecoveryGoal::parse_input("1048576"), Some(1048576));
        assert_eq!(RecoveryGoal::parse_input("invalid"), None);
        assert_eq!(RecoveryGoal::parse_input(""), None);
    }
}
