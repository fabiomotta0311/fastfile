//! Motor de regras explicáveis para reconhecimento de categorias e oportunidades de limpeza (TM-07).
//!
//! Analisa arquivos e pastas combinando contexto de caminho, marcadores de projeto, atributos e nomes,
//! gerando diagnósticos com regra de origem, motivo, consequência e recomendação responsável.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Categorias de classificação para análise e oportunidades de limpeza (TM-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CleanupCategory {
    /// Artefatos e dependências gerados por compiladores e ferramentas de build (ex: target com Cargo.toml, node_modules com package.json).
    BuildArtifacts,
    /// Caches descartáveis e arquivos temporários de sistema e aplicações conhecidas (ex: %TEMP%, AppData\Local\Temp).
    KnownCache,
    /// Pacotes instaladores e arquivos baixados da web (ex: .iso, setups antigos em Downloads).
    InstallerOrDownload,
    /// Conteúdo pessoal do usuário (ex: Documentos, Fotos, Músicas, Vídeos pessoais). NUNCA chamado de lixo.
    PersonalContent,
    /// Dados, bancos de dados e configurações gerenciadas por aplicações (ex: .git, AppData\Roaming, .mdf, .sqlite).
    ApplicationData,
    /// Arquivos protegidos ou componentes críticos do sistema operacional (ex: Windows, System Volume Information, pagefile.sys).
    SystemProtected,
    /// Itens não identificados com certeza ou nomes ambíguos sem marcadores (ex: pasta 'target' ou 'cache' isolada).
    ManualReview,
}

impl CleanupCategory {
    /// Rótulo curto em português para a categoria.
    #[must_use]
    pub fn label_pt(&self) -> &'static str {
        match self {
            Self::BuildArtifacts => "Artefatos de Build",
            Self::KnownCache => "Cache Conhecido",
            Self::InstallerOrDownload => "Instalador / Download",
            Self::PersonalContent => "Conteúdo Pessoal",
            Self::ApplicationData => "Dados de Aplicação",
            Self::SystemProtected => "Sistema / Protegido",
            Self::ManualReview => "Revisão Manual",
        }
    }

    /// Rótulo curto em inglês para a categoria.
    #[must_use]
    pub fn label_en(&self) -> &'static str {
        match self {
            Self::BuildArtifacts => "Build Artifacts",
            Self::KnownCache => "Known Cache",
            Self::InstallerOrDownload => "Installer / Download",
            Self::PersonalContent => "Personal Content",
            Self::ApplicationData => "Application Data",
            Self::SystemProtected => "System / Protected",
            Self::ManualReview => "Manual Review",
        }
    }

    /// Ícone representativo para a categoria.
    #[must_use]
    pub fn icon(&self) -> &'static str {
        match self {
            Self::BuildArtifacts => "🔨",
            Self::KnownCache => "🧹",
            Self::InstallerOrDownload => "💿",
            Self::PersonalContent => "👤",
            Self::ApplicationData => "📦",
            Self::SystemProtected => "🛡️",
            Self::ManualReview => "🔍",
        }
    }
}

/// Diagnóstico explicável gerado para um item do disco (TM-07).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanupAdvice {
    /// Categoria identificada.
    pub category: CleanupCategory,
    /// Identificador estável da regra que originou o diagnóstico.
    pub rule_id: &'static str,
    /// Motivo detalhado da classificação com base em nome, caminho e marcadores.
    pub reason: String,
    /// Consequência provável caso o usuário decida remover ou limpar o item.
    pub consequence: String,
    /// Ação responsável recomendada.
    pub action: String,
    /// Indica se o descarte direto do diretório/arquivo é considerado seguro e reconstituível.
    pub safe_to_delete: bool,
}

/// Identifica marcadores de projeto no diretório pai ou ancestrais para contextualizar pastas de build.
fn has_project_marker_in_parent(path_str: &str, markers: &[&str]) -> bool {
    let p = Path::new(path_str);
    // Um caminho sem pai real (ex.: `C:\Data\target` fora do Windows, onde `\` não separa) teria
    // como "pai" a string vazia, e o marcador seria procurado na pasta atual do processo.
    if let Some(parent) = p.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        for marker in markers {
            if parent.join(marker).exists() {
                return true;
            }
        }
    }
    false
}

/// Motor principal de regras explicáveis de categorização e diagnóstico de limpeza (TM-07).
#[must_use]
pub fn analyze_cleanup_opportunity(
    path: &str,
    name: &str,
    is_dir: bool,
    sibling_names: Option<&[&str]>,
) -> CleanupAdvice {
    let lower_path = path.to_lowercase();
    let lower_name = name.to_lowercase();

    // 1. Sistema Operacional e Conteúdo Protegido
    if lower_path.contains("system volume information")
        || lower_path.contains("$recycle.bin")
        || lower_path.starts_with("c:\\windows")
        || lower_name == "hiberfil.sys"
        || lower_name == "pagefile.sys"
        || lower_name == "swapfile.sys"
    {
        return CleanupAdvice {
            category: CleanupCategory::SystemProtected,
            rule_id: "RULE-SYSTEM-PROTECTED-OS",
            reason: format!(
                "'{name}' faz parte do sistema operacional ou é um arquivo protegido crítico do Windows."
            ),
            consequence: "A exclusão forçada pode causar instabilidade crítica, corromper o sistema ou falhas no boot.".to_string(),
            action: "Não excluir manualmente. Utilize utilitários oficiais como a Limpeza de Disco do Windows (cleanmgr) ou ajuste a memória virtual/hibernação nas opções do sistema.".to_string(),
            safe_to_delete: false,
        };
    }

    // 2. Dados Gerenciados por Aplicações e Controle de Versão
    if lower_name == ".git" || lower_path.contains("\\.git\\") {
        return CleanupAdvice {
            category: CleanupCategory::ApplicationData,
            rule_id: "RULE-APP-GIT-REPO",
            reason: "Diretório do repositório Git contendo o histórico de versões e configurações locais.".to_string(),
            consequence: "Remover esta pasta destruirá todo o histórico de commits locais e o rastreamento do repositório.".to_string(),
            action: "Gerencie o repositório através de comandos Git (ex: 'git gc', 'git prune') ou arquive o projeto antes de qualquer descarte.".to_string(),
            safe_to_delete: false,
        };
    }

    if lower_path.contains("\\appdata\\roaming\\") {
        return CleanupAdvice {
            category: CleanupCategory::ApplicationData,
            rule_id: "RULE-APP-ROAMING-CONFIG",
            reason: format!("'{name}' reside em AppData\\Roaming e guarda configurações ativas ou dados de perfil de um aplicativo."),
            consequence: "Excluir manualmente pode redefinir preferências do aplicativo, perder credenciais salvas ou corromper dados da aplicação.".to_string(),
            action: "Gerencie as configurações diretamente pelo aplicativo proprietário ou através do desinstalador oficial do software.".to_string(),
            safe_to_delete: false,
        };
    }

    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if !is_dir && matches!(ext.as_str(), "mdf" | "ldf" | "sqlite" | "sqlite3" | "db") {
        return CleanupAdvice {
            category: CleanupCategory::ApplicationData,
            rule_id: "RULE-APP-DATABASE",
            reason: format!("Arquivo de banco de dados '{name}' gerenciado por uma aplicação ou serviço local."),
            consequence: "Exclusão direta causa perda imediata de dados estruturados e falhas no serviço proprietário.".to_string(),
            action: "Realize backup e gerencie o banco de dados através do serviço correspondente; não realize exclusão direta sem exportar os dados.".to_string(),
            safe_to_delete: false,
        };
    }

    // 3. Conteúdo Pessoal do Usuário (NUNCA chamado de lixo!)
    let personal_locations = [
        "\\documents",
        "\\documentos",
        "\\pictures",
        "\\imagens",
        "\\videos",
        "\\vídeos",
        "\\music",
        "\\músicas",
        "\\desktop",
        "\\área de trabalho",
        "\\onedrive\\documents",
        "\\onedrive\\pictures",
    ];

    let is_in_personal_location = personal_locations
        .iter()
        .any(|loc| lower_path.contains(loc));
    let personal_extensions = [
        "docx", "xlsx", "pptx", "pdf", "odt", "ods", "odp", "psd", "ai", "raw", "cr2", "nef",
        "dng", "jpg", "jpeg", "png", "mp3", "flac", "wav",
    ];

    if is_in_personal_location || (!is_dir && personal_extensions.contains(&ext.as_str())) {
        return CleanupAdvice {
            category: CleanupCategory::PersonalContent,
            rule_id: "RULE-PERSONAL-USER-CONTENT",
            reason: format!("'{name}' corresponde a documentos, fotos ou mídia pessoal do usuário."),
            consequence: "Exclusão resulta na perda permanente de arquivos e criações pessoais sem possibilidade de reconstrução automática.".to_string(),
            action: "Este item não é lixo. Mantenha em disco ou transfira para um dispositivo de backup/armazenamento externo caso necessite liberar espaço.".to_string(),
            safe_to_delete: false,
        };
    }

    // 4. Artefatos de Build e Dependências (com verificação rigorosa de marcadores de projeto)
    if is_dir && lower_name == "target" {
        let has_cargo = sibling_names
            .is_some_and(|sibs| sibs.iter().any(|s| s.eq_ignore_ascii_case("Cargo.toml")))
            || has_project_marker_in_parent(path, &["Cargo.toml"]);

        if has_cargo {
            return CleanupAdvice {
                category: CleanupCategory::BuildArtifacts,
                rule_id: "RULE-BUILD-RUST-TARGET",
                reason: "Diretório 'target' contendo artefatos de compilação dentro de repositório Rust (Cargo.toml confirmado no projeto).".to_string(),
                consequence: "Liberará espaço em disco imediatamente. O próximo comando 'cargo build' recompilará os artefatos a partir do código-fonte.".to_string(),
                action: "Pode ser limpo com segurança executando 'cargo clean' no terminal do projeto ou excluindo esta pasta.".to_string(),
                safe_to_delete: true,
            };
        }

        // Sem marcador do projeto: CRITÉRIO DE ACEITE: Uma pasta chamada cache ou target, isoladamente, NÃO recebe garantia de descarte seguro!
        return CleanupAdvice {
            category: CleanupCategory::ManualReview,
            rule_id: "RULE-AMBIGUOUS-TARGET-DIR",
            reason: "Pasta chamada 'target' encontrada sem marcadores de compilação Rust (Cargo.toml) no contexto imediato.".to_string(),
            consequence: "Não há garantia de que se trate de artefatos de build descartáveis; pode conter arquivos de dados ou código.".to_string(),
            action: "Realize revisão manual do conteúdo antes de qualquer ação. Não garantimos descarte seguro para pastas chamadas target de forma isolada.".to_string(),
            safe_to_delete: false,
        };
    }

    if is_dir && lower_name == "node_modules" {
        let has_package_json = sibling_names
            .is_some_and(|sibs| sibs.iter().any(|s| s.eq_ignore_ascii_case("package.json")))
            || has_project_marker_in_parent(path, &["package.json"]);

        if has_package_json {
            return CleanupAdvice {
                category: CleanupCategory::BuildArtifacts,
                rule_id: "RULE-BUILD-NODE-MODULES",
                reason: "Diretório 'node_modules' de dependências de projeto JavaScript/Node.js (package.json confirmado).".to_string(),
                consequence: "Liberará espaço expressivo em disco. As dependências podem ser reinstaladas posteriormente com 'npm install' ou 'pnpm install'.".to_string(),
                action: "Pode ser excluído para economizar espaço se o projeto estiver inativo. Reinstale quando for retomar o desenvolvimento.".to_string(),
                safe_to_delete: true,
            };
        }

        return CleanupAdvice {
            category: CleanupCategory::ManualReview,
            rule_id: "RULE-AMBIGUOUS-NODE-MODULES",
            reason: "Pasta chamada 'node_modules' sem package.json identificado no diretório pai.".to_string(),
            consequence: "Pode conter módulos isolados ou ser parte de outra estrutura não versionada.".to_string(),
            action: "Revise manualmente antes de excluir para confirmar a possibilidade de restauração via gerenciador de pacotes.".to_string(),
            safe_to_delete: false,
        };
    }

    if is_dir && (lower_name == ".venv" || lower_name == "venv") {
        let has_pyproject = sibling_names.is_some_and(|sibs| {
            sibs.iter().any(|s| {
                s.eq_ignore_ascii_case("pyproject.toml")
                    || s.eq_ignore_ascii_case("requirements.txt")
                    || s.eq_ignore_ascii_case("setup.py")
            })
        }) || has_project_marker_in_parent(
            path,
            &["pyproject.toml", "requirements.txt", "setup.py"],
        );

        if has_pyproject {
            return CleanupAdvice {
                category: CleanupCategory::BuildArtifacts,
                rule_id: "RULE-BUILD-PYTHON-VENV",
                reason: "Ambiente virtual Python (.venv) associado a projeto com manifesto identificado.".to_string(),
                consequence: "Liberará espaço; os pacotes instalados precisarão ser recriados com 'python -m venv' e 'pip install -r requirements.txt'.".to_string(),
                action: "Pode ser removido caso não esteja em uso ativo no momento.".to_string(),
                safe_to_delete: true,
            };
        }
    }

    // 5. Caches Conhecidos
    let is_temp_location = lower_path.contains("\\appdata\\local\\temp")
        || lower_path.contains("\\windows\\temp")
        || lower_path.ends_with("\\temp")
        || lower_path.ends_with("/temp");

    if is_temp_location {
        return CleanupAdvice {
            category: CleanupCategory::KnownCache,
            rule_id: "RULE-CACHE-KNOWN-TEMP",
            reason: format!("'{name}' localiza-se em pasta temporária conhecida do sistema (%TEMP%)."),
            consequence: "Arquivos temporários liberam espaço imediato; processos ativos que ainda usem o arquivo podem acusar bloqueio temporário.".to_string(),
            action: "Pode ser limpo com segurança. Arquivos em uso pelo sistema operacional ou aplicativos abertos serão ignorados.".to_string(),
            safe_to_delete: true,
        };
    }

    let is_browser_cache = lower_path.contains("\\google\\chrome\\user data\\default\\cache")
        || lower_path.contains("\\microsoft\\edge\\user data\\default\\cache")
        || lower_path.contains("\\mozilla\\firefox\\profiles\\") && lower_path.contains("\\cache2");

    if is_browser_cache {
        return CleanupAdvice {
            category: CleanupCategory::KnownCache,
            rule_id: "RULE-CACHE-BROWSER",
            reason: "Cache de navegação na internet gerado por navegador web.".to_string(),
            consequence: "Liberará espaço em disco; páginas web visitadas recentemente levarão alguns milissegundos a mais para carregar no primeiro acesso.".to_string(),
            action: "Pode ser limpo com segurança pelas configurações do navegador ou excluindo o diretório de cache com o navegador fechado.".to_string(),
            safe_to_delete: true,
        };
    }

    // CRITÉRIO DE ACEITE: Uma pasta chamada cache, isoladamente, NÃO recebe garantia de descarte seguro!
    if is_dir && lower_name == "cache" {
        return CleanupAdvice {
            category: CleanupCategory::ManualReview,
            rule_id: "RULE-AMBIGUOUS-CACHE-DIR",
            reason: "Pasta chamada 'cache' sem proveniência ou assinatura de aplicação temporária confirmada.".to_string(),
            consequence: "Algumas aplicações armazenam dados persistentes essenciais ou estados locais em pastas chamadas cache.".to_string(),
            action: "Realize revisão manual. Uma pasta chamada cache, isoladamente, não recebe garantia de descarte seguro.".to_string(),
            safe_to_delete: false,
        };
    }

    // 6. Instaladores e Downloads
    let is_installer_extension = matches!(
        ext.as_str(),
        "iso" | "img" | "vmdk" | "vhdx" | "vdi" | "msi" | "pkg" | "dmg"
    );
    let is_in_downloads =
        lower_path.contains("\\downloads\\") || lower_path.contains("/downloads/");
    let has_setup_name = lower_name.contains("setup")
        || lower_name.contains("installer")
        || lower_name.contains("update");

    if is_installer_extension
        || (is_in_downloads && (has_setup_name || ext == "exe" || ext == "msi"))
    {
        return CleanupAdvice {
            category: CleanupCategory::InstallerOrDownload,
            rule_id: "RULE-INSTALLER-OR-DOWNLOAD",
            reason: format!("'{name}' identificado como pacote instalador, imagem de disco ou arquivo baixado."),
            consequence: "Se o software já estiver instalado na máquina ou não for mais necessário, o instalador original pode ser descartado sem afetar os programas em execução.".to_string(),
            action: "Revise se a instalação já foi concluída. Se concluída, o arquivo instalador pode ser excluído para liberar espaço.".to_string(),
            safe_to_delete: true,
        };
    }

    // 7. Padrão: Revisão Manual para qualquer item não categorizado com segurança
    CleanupAdvice {
        category: CleanupCategory::ManualReview,
        rule_id: "RULE-MANUAL-REVIEW-DEFAULT",
        reason: format!("'{name}' não correspondeu a nenhum padrão seguro automatizado conhecido."),
        consequence: "A exclusão de arquivos não identificados pode causar perda de informações ou falha em ferramentas terceiras.".to_string(),
        action: "Inspecione o conteúdo e localização do arquivo antes de tomar qualquer decisão de exclusão.".to_string(),
        safe_to_delete: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_isolated_cache_and_target_dirs_do_not_get_safe_delete_guarantee() {
        // Critério de aceite: Uma pasta chamada cache ou target, isoladamente, não recebe garantia de descarte seguro.
        let advice_target = analyze_cleanup_opportunity("C:\\Data\\target", "target", true, None);
        assert_eq!(advice_target.category, CleanupCategory::ManualReview);
        assert_eq!(advice_target.rule_id, "RULE-AMBIGUOUS-TARGET-DIR");
        assert!(!advice_target.safe_to_delete);

        let advice_cache = analyze_cleanup_opportunity("C:\\MyFolder\\cache", "cache", true, None);
        assert_eq!(advice_cache.category, CleanupCategory::ManualReview);
        assert_eq!(advice_cache.rule_id, "RULE-AMBIGUOUS-CACHE-DIR");
        assert!(!advice_cache.safe_to_delete);
    }

    /// Um caminho sem pasta pai não pode herdar o `Cargo.toml` da pasta atual do processo
    /// (os testes rodam dentro do crate, que tem `Cargo.toml`).
    #[test]
    fn test_parentless_target_does_not_use_cwd_marker() {
        assert!(Path::new("Cargo.toml").exists(), "pré-condição do teste");
        let advice = analyze_cleanup_opportunity("target", "target", true, None);
        assert_eq!(advice.category, CleanupCategory::ManualReview);
        assert!(!advice.safe_to_delete);
    }

    #[test]
    fn test_rust_target_with_cargo_marker_is_safe_build_artifact() {
        let siblings = ["Cargo.toml", "src", "Cargo.lock"];
        let advice = analyze_cleanup_opportunity(
            "C:\\Projects\\fastfile\\target",
            "target",
            true,
            Some(&siblings),
        );
        assert_eq!(advice.category, CleanupCategory::BuildArtifacts);
        assert_eq!(advice.rule_id, "RULE-BUILD-RUST-TARGET");
        assert!(advice.safe_to_delete);
        assert!(advice.reason.contains("Cargo.toml"));
    }

    #[test]
    fn test_personal_content_is_never_called_trash() {
        // Critério de aceite: Conteúdo pessoal não é chamado de lixo.
        let advice = analyze_cleanup_opportunity(
            "C:\\Users\\fabio\\Pictures\\vacation.jpg",
            "vacation.jpg",
            false,
            None,
        );
        assert_eq!(advice.category, CleanupCategory::PersonalContent);
        assert_eq!(advice.rule_id, "RULE-PERSONAL-USER-CONTENT");
        assert!(!advice.safe_to_delete);
        assert!(!advice.reason.to_lowercase().contains("lixo"));
        assert!(
            !advice.action.to_lowercase().contains("lixo") || advice.action.contains("não é lixo")
        );
    }

    #[test]
    fn test_application_data_and_git_receive_proper_guidance() {
        // Critério de aceite: Dados de aplicações e sistema recebem orientação adequada, não recomendação genérica de exclusão.
        let advice_git =
            analyze_cleanup_opportunity("C:\\Projects\\repo\\.git", ".git", true, None);
        assert_eq!(advice_git.category, CleanupCategory::ApplicationData);
        assert_eq!(advice_git.rule_id, "RULE-APP-GIT-REPO");
        assert!(!advice_git.safe_to_delete);
        assert!(advice_git.action.contains("git"));

        let advice_db =
            analyze_cleanup_opportunity("C:\\Services\\app.sqlite", "app.sqlite", false, None);
        assert_eq!(advice_db.category, CleanupCategory::ApplicationData);
        assert_eq!(advice_db.rule_id, "RULE-APP-DATABASE");
        assert!(!advice_db.safe_to_delete);
    }

    #[test]
    fn test_system_and_protected_files_have_safety_warnings() {
        let advice = analyze_cleanup_opportunity(
            "C:\\Windows\\System32\\ntdll.dll",
            "ntdll.dll",
            false,
            None,
        );
        assert_eq!(advice.category, CleanupCategory::SystemProtected);
        assert_eq!(advice.rule_id, "RULE-SYSTEM-PROTECTED-OS");
        assert!(!advice.safe_to_delete);
    }

    #[test]
    fn test_installers_and_downloads_classified_properly() {
        let advice = analyze_cleanup_opportunity(
            "C:\\Users\\fabio\\Downloads\\node_setup.exe",
            "node_setup.exe",
            false,
            None,
        );
        assert_eq!(advice.category, CleanupCategory::InstallerOrDownload);
        assert_eq!(advice.rule_id, "RULE-INSTALLER-OR-DOWNLOAD");
        assert!(advice.safe_to_delete);
    }
}
