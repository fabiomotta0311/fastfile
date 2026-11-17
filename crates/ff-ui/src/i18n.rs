//! Módulo de Internacionalização (i18n) e Localização (Locale) para o FastFile UI.
//!
//! Suporta detecção automática do idioma do Sistema Operacional (Windows e Linux)
//! e alternância manual na interface para 6 idiomas:
//! - Português (Pt)
//! - English (En)
//! - Español (Es)
//! - Français (Fr)
//! - Italiano (It)
//! - Русский (Ru) — com alfabeto cirílico completo em UTF-8

use serde::{Deserialize, Serialize};

/// Idiomas suportados pela interface gráfica.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Locale {
    /// Português (Brasil / Portugal)
    #[default]
    Pt,
    /// Inglês (Universal / EUA)
    En,
    /// Espanhol (Espanha / América Latina)
    Es,
    /// Francês (França / Canadá / etc.)
    Fr,
    /// Italiano (Itália / Suíça)
    It,
    /// Russo (Federação Russa / alfabeto cirílico)
    Ru,
}

impl Locale {
    /// Detecta o idioma padrão da interface a partir do sistema operacional.
    ///
    /// No Windows: Consulta `GetUserDefaultUILanguage()` via Win32.
    /// Códigos primários:
    /// - 0x16: Português
    /// - 0x0A: Espanhol
    /// - 0x0C: Francês
    /// - 0x10: Italiano
    /// - 0x19: Russo
    /// - Padrão / outros: Inglês
    ///
    /// No Linux / Unix: Analisa as variáveis de ambiente `LC_ALL`, `LC_MESSAGES` ou `LANG`.
    pub fn detect_from_system() -> Self {
        #[cfg(windows)]
        {
            extern "system" {
                fn GetUserDefaultUILanguage() -> u16;
            }
            let lang_id = unsafe { GetUserDefaultUILanguage() };
            let primary_lang = lang_id & 0x03FF;
            match primary_lang {
                0x16 => Locale::Pt,
                0x0A => Locale::Es,
                0x0C => Locale::Fr,
                0x10 => Locale::It,
                0x19 => Locale::Ru,
                _ => Locale::En,
            }
        }

        #[cfg(not(windows))]
        {
            for var in &["LC_ALL", "LC_MESSAGES", "LANG"] {
                if let Ok(val) = std::env::var(var) {
                    let lower = val.to_lowercase();
                    if lower.starts_with("pt") {
                        return Locale::Pt;
                    } else if lower.starts_with("es") {
                        return Locale::Es;
                    } else if lower.starts_with("fr") {
                        return Locale::Fr;
                    } else if lower.starts_with("it") {
                        return Locale::It;
                    } else if lower.starts_with("ru") {
                        return Locale::Ru;
                    }
                }
            }
            Locale::En
        }
    }

    /// Rótulo curto de 2 letras para o botão na UI (ex: "PT", "EN", "ES", "FR", "IT", "RU").
    pub fn code(&self) -> &'static str {
        match self {
            Locale::Pt => "PT",
            Locale::En => "EN",
            Locale::Es => "ES",
            Locale::Fr => "FR",
            Locale::It => "IT",
            Locale::Ru => "RU",
        }
    }

    /// Nome legível do idioma no próprio idioma nativo.
    pub fn display_name(&self) -> &'static str {
        match self {
            Locale::Pt => "Português",
            Locale::En => "English",
            Locale::Es => "Español",
            Locale::Fr => "Français",
            Locale::It => "Italiano",
            Locale::Ru => "Русский",
        }
    }

    /// Alterna ciclicamente entre os 6 idiomas disponíveis: PT -> EN -> ES -> FR -> IT -> RU -> PT.
    pub fn toggle(&self) -> Self {
        match self {
            Locale::Pt => Locale::En,
            Locale::En => Locale::Es,
            Locale::Es => Locale::Fr,
            Locale::Fr => Locale::It,
            Locale::It => Locale::Ru,
            Locale::Ru => Locale::Pt,
        }
    }

    // --- Títulos e Cabeçalhos ---

    pub fn window_title(&self) -> &'static str {
        match self {
            Locale::Pt => "FastFile — Busca, Prévia, Diff e Uso de Disco",
            Locale::En => "FastFile — Search, Preview, Diff & Disk Treemap",
            Locale::Es => "FastFile — Búsqueda, Vista Previa, Diff y Uso de Disco",
            Locale::Fr => "FastFile — Recherche, Aperçu, Diff et Utilisation du Disque",
            Locale::It => "FastFile — Ricerca, Anteprima, Diff e Utilizzo Disco",
            Locale::Ru => "FastFile — Поиск, Просмотр, Diff и Анализ Диска",
        }
    }

    // --- Abas Principais ---

    pub fn tab_results(&self) -> &'static str {
        match self {
            Locale::Pt => "📂 Resultados",
            Locale::En => "📂 Results",
            Locale::Es => "📂 Resultados",
            Locale::Fr => "📂 Résultats",
            Locale::It => "📂 Risultati",
            Locale::Ru => "📂 Результаты",
        }
    }

    pub fn tab_treemap(&self) -> &'static str {
        match self {
            Locale::Pt => "📊 Treemap de Uso",
            Locale::En => "📊 Disk Treemap",
            Locale::Es => "📊 Treemap de Uso",
            Locale::Fr => "📊 Treemap Disque",
            Locale::It => "📊 Treemap Disco",
            Locale::Ru => "📊 Дерево Диска",
        }
    }

    pub fn tab_diff(&self) -> &'static str {
        match self {
            Locale::Pt => "⚖️ Diff Lado a Lado",
            Locale::En => "⚖️ Side-by-Side Diff",
            Locale::Es => "⚖️ Diff Lado a Lado",
            Locale::Fr => "⚖️ Diff Côte à Côte",
            Locale::It => "⚖️ Diff Fianco a Fianco",
            Locale::Ru => "⚖️ Сравнение Diff",
        }
    }

    // --- Filtros de Categoria ---

    pub fn filter_all(&self) -> &'static str {
        match self {
            Locale::Pt => "Todos",
            Locale::En => "All",
            Locale::Es => "Todos",
            Locale::Fr => "Tous",
            Locale::It => "Tutti",
            Locale::Ru => "Все",
        }
    }

    pub fn filter_code(&self) -> &'static str {
        match self {
            Locale::Pt => "Código",
            Locale::En => "Code",
            Locale::Es => "Código",
            Locale::Fr => "Code",
            Locale::It => "Codice",
            Locale::Ru => "Код",
        }
    }

    pub fn filter_docs(&self) -> &'static str {
        match self {
            Locale::Pt => "Documentos",
            Locale::En => "Documents",
            Locale::Es => "Documentos",
            Locale::Fr => "Documents",
            Locale::It => "Documenti",
            Locale::Ru => "Документы",
        }
    }

    pub fn filter_media(&self) -> &'static str {
        match self {
            Locale::Pt => "Mídia",
            Locale::En => "Media",
            Locale::Es => "Multimedia",
            Locale::Fr => "Médias",
            Locale::It => "Media",
            Locale::Ru => "Медиа",
        }
    }

    pub fn filter_archives(&self) -> &'static str {
        match self {
            Locale::Pt => "Compactados",
            Locale::En => "Archives",
            Locale::Es => "Comprimidos",
            Locale::Fr => "Archives",
            Locale::It => "Archivi",
            Locale::Ru => "Архивы",
        }
    }

    pub fn filter_large(&self) -> &'static str {
        match self {
            Locale::Pt | Locale::En | Locale::Es | Locale::It => "> 100 MB",
            Locale::Fr => "> 100 Mo",
            Locale::Ru => "> 100 МБ",
        }
    }

    // --- Botões e Ações Principais ---

    pub fn compare_button(&self, count: usize) -> String {
        match self {
            Locale::Pt | Locale::Es => format!("Comparar ({count})"),
            Locale::En => format!("Compare ({count})"),
            Locale::Fr => format!("Comparer ({count})"),
            Locale::It => format!("Confronta ({count})"),
            Locale::Ru => format!("Сравнить ({count})"),
        }
    }

    pub fn show_preview(&self) -> &'static str {
        match self {
            Locale::Pt => "Mostrar Prévia",
            Locale::En => "Show Preview",
            Locale::Es => "Mostrar Vista Previa",
            Locale::Fr => "Afficher l'Aperçu",
            Locale::It => "Mostra Anteprima",
            Locale::Ru => "Показать Превью",
        }
    }

    pub fn hide_preview(&self) -> &'static str {
        match self {
            Locale::Pt => "Ocultar Prévia",
            Locale::En => "Hide Preview",
            Locale::Es => "Ocultar Vista Previa",
            Locale::Fr => "Masquer l'Aperçu",
            Locale::It => "Nascondi Anteprima",
            Locale::Ru => "Скрыть Превью",
        }
    }

    pub fn download_csv(&self) -> &'static str {
        match self {
            Locale::Pt => "Baixar CSV",
            Locale::En => "Download CSV",
            Locale::Es => "Descargar CSV",
            Locale::Fr => "Télécharger CSV",
            Locale::It => "Scarica CSV",
            Locale::Ru => "Скачать CSV",
        }
    }

    pub fn back_to_search(&self) -> &'static str {
        match self {
            Locale::Pt => "Voltar à Busca",
            Locale::En => "Back to Search",
            Locale::Es => "Volver a la Búsqueda",
            Locale::Fr => "Retour à la Recherche",
            Locale::It => "Torna alla Ricerca",
            Locale::Ru => "Назад к Поиску",
        }
    }

    pub fn search_placeholder(&self) -> &'static str {
        match self {
            Locale::Pt => "Buscar por nome, curingas (*.rs) ou regex (/pattern/)...",
            Locale::En => "Search by name, wildcards (*.rs) or regex (/pattern/)...",
            Locale::Es => "Buscar por nombre, comodines (*.rs) o regex (/patrón/)...",
            Locale::Fr => "Rechercher par nom, jokers (*.rs) ou regex (/motif/)...",
            Locale::It => "Cerca per nome, caratteri jolly (*.rs) o regex (/pattern/)...",
            Locale::Ru => "Поиск по имени, шаблону (*.rs) или regex (/шаблон/)...",
        }
    }

    // --- Colunas da Tabela de Resultados ---

    pub fn col_name(&self) -> &'static str {
        match self {
            Locale::Pt => "Nome",
            Locale::En => "Name",
            Locale::Es => "Nombre",
            Locale::Fr => "Nom",
            Locale::It => "Nome",
            Locale::Ru => "Имя",
        }
    }

    pub fn col_size(&self) -> &'static str {
        match self {
            Locale::Pt => "Tamanho",
            Locale::En => "Size",
            Locale::Es => "Tamaño",
            Locale::Fr => "Taille",
            Locale::It => "Dimensione",
            Locale::Ru => "Размер",
        }
    }

    pub fn col_path(&self) -> &'static str {
        match self {
            Locale::Pt => "Caminho",
            Locale::En => "Path",
            Locale::Es => "Ruta",
            Locale::Fr => "Chemin",
            Locale::It => "Percorso",
            Locale::Ru => "Путь",
        }
    }

    pub fn col_date(&self) -> &'static str {
        match self {
            Locale::Pt => "Modificado",
            Locale::En => "Modified",
            Locale::Es => "Modificado",
            Locale::Fr => "Modifié",
            Locale::It => "Modificato",
            Locale::Ru => "Изменен",
        }
    }

    pub fn no_files_found(&self) -> &'static str {
        match self {
            Locale::Pt => "Nenhum arquivo encontrado",
            Locale::En => "No files found",
            Locale::Es => "No se encontraron archivos",
            Locale::Fr => "Aucun fichier trouvé",
            Locale::It => "Nessun file trovato",
            Locale::Ru => "Файлы не найдены",
        }
    }

    // --- Painel de Pré-visualização ---

    pub fn preview_header(&self) -> &'static str {
        match self {
            Locale::Pt => "👁 Pré-visualização",
            Locale::En => "👁 Preview",
            Locale::Es => "👁 Vista Previa",
            Locale::Fr => "👁 Aperçu",
            Locale::It => "👁 Anteprima",
            Locale::Ru => "👁 Предпросмотр",
        }
    }

    pub fn preview_filter_hint(&self) -> &'static str {
        match self {
            Locale::Pt => "Filtrar no arquivo...",
            Locale::En => "Filter in file...",
            Locale::Es => "Filtrar en el archivo...",
            Locale::Fr => "Filtrer dans le fichier...",
            Locale::It => "Filtra nel file...",
            Locale::Ru => "Фильтр в файле...",
        }
    }

    pub fn preview_matches(&self, count: usize) -> String {
        match self {
            Locale::Pt => format!("{count} ocorrência(s)"),
            Locale::En => format!("{count} match(es)"),
            Locale::Es => format!("{count} coincidencia(s)"),
            Locale::Fr => format!("{count} occurrence(s)"),
            Locale::It => format!("{count} corrispondenza/e"),
            Locale::Ru => format!("{count} совпадение(й)"),
        }
    }

    pub fn preview_binary_title(&self, size_str: &str) -> String {
        match self {
            Locale::Pt => format!("Arquivo Binário — Tamanho: {size_str} (Hex Dump)"),
            Locale::En => format!("Binary File — Size: {size_str} (Hex Dump)"),
            Locale::Es => format!("Archivo Binario — Tamaño: {size_str} (Hex Dump)"),
            Locale::Fr => format!("Fichier Binaire — Taille: {size_str} (Hex Dump)"),
            Locale::It => format!("File Binario — Dimensione: {size_str} (Hex Dump)"),
            Locale::Ru => format!("Бинарный файл — Размер: {size_str} (Hex Dump)"),
        }
    }

    pub fn preview_pdf_title(&self, size_str: &str) -> String {
        match self {
            Locale::Pt => format!("Texto extraído do documento PDF (Tamanho: {size_str})"),
            Locale::En => format!("Extracted text from PDF document (Size: {size_str})"),
            Locale::Es => format!("Texto extraído del documento PDF (Tamaño: {size_str})"),
            Locale::Fr => format!("Texte extrait du document PDF (Taille: {size_str})"),
            Locale::It => format!("Testo estratto dal documento PDF (Dimensione: {size_str})"),
            Locale::Ru => format!("Текст из документа PDF (Размер: {size_str})"),
        }
    }

    pub fn preview_docx_title(&self, size_str: &str) -> String {
        match self {
            Locale::Pt => format!("Texto extraído do documento DOCX (Tamanho: {size_str})"),
            Locale::En => format!("Extracted text from DOCX document (Size: {size_str})"),
            Locale::Es => format!("Texto extraído del documento DOCX (Tamaño: {size_str})"),
            Locale::Fr => format!("Texte extrait du document DOCX (Taille: {size_str})"),
            Locale::It => format!("Testo estratto dal documento DOCX (Dimensione: {size_str})"),
            Locale::Ru => format!("Текст из документа DOCX (Размер: {size_str})"),
        }
    }

    pub fn preview_loading(&self) -> &'static str {
        match self {
            Locale::Pt => "Carregando pré-visualização...",
            Locale::En => "Loading preview...",
            Locale::Es => "Cargando vista previa...",
            Locale::Fr => "Chargement de l'aperçu...",
            Locale::It => "Caricamento anteprima...",
            Locale::Ru => "Загрузка предпросмотра...",
        }
    }

    pub fn preview_failed(&self, err: &str) -> String {
        match self {
            Locale::Pt => format!("Falha ao carregar pré-visualização: {err}"),
            Locale::En => format!("Failed to load preview: {err}"),
            Locale::Es => format!("Error al cargar vista previa: {err}"),
            Locale::Fr => format!("Échec du chargement de l'aperçu: {err}"),
            Locale::It => format!("Impossibile caricare l'anteprima: {err}"),
            Locale::Ru => format!("Ошибка загрузки предпросмотра: {err}"),
        }
    }

    pub fn preview_truncated(&self) -> &'static str {
        match self {
            Locale::Pt => "[Conteúdo truncado para exibição rápida. Arquivo completo no disco]",
            Locale::En => "[Content truncated for fast display. Full file preserved on disk]",
            Locale::Es => {
                "[Contenido truncado para visualización rápida. Archivo completo en disco]"
            }
            Locale::Fr => "[Contenu tronqué pour affichage rapide. Fichier complet sur disque]",
            Locale::It => "[Contenuto troncato per visualizzazione rapida. File completo su disco]",
            Locale::Ru => {
                "[Содержимое сокращено для быстрого показа. Полный файл сохранен на диске]"
            }
        }
    }

    // --- Treemap de Disco ---

    pub fn treemap_choose_folder(&self) -> &'static str {
        match self {
            Locale::Pt => "📁 Escolher Pasta...",
            Locale::En => "📁 Choose Folder...",
            Locale::Es => "📁 Elegir Carpeta...",
            Locale::Fr => "📁 Choisir Dossier...",
            Locale::It => "📁 Scegli Cartella...",
            Locale::Ru => "📁 Выбрать Папку...",
        }
    }

    pub fn treemap_analyze(&self) -> &'static str {
        match self {
            Locale::Pt => "🔍 Analisar",
            Locale::En => "🔍 Analyze",
            Locale::Es => "🔍 Analizar",
            Locale::Fr => "🔍 Analyser",
            Locale::It => "🔍 Analizza",
            Locale::Ru => "🔍 Анализ",
        }
    }

    pub fn treemap_analyze_tooltip(&self) -> &'static str {
        match self {
            Locale::Pt => "Calcula e renderiza o Treemap da pasta informada",
            Locale::En => "Calculates and renders Treemap for the specified directory",
            Locale::Es => "Calcula y muestra el Treemap de la carpeta especificada",
            Locale::Fr => "Calcule et affiche le Treemap du dossier spécifié",
            Locale::It => "Calcola e visualizza il Treemap della cartella specificata",
            Locale::Ru => "Рассчитывает и отображает Treemap для указанной папки",
        }
    }

    pub fn treemap_up_folder(&self) -> &'static str {
        match self {
            Locale::Pt => "⬆ Subir Pasta",
            Locale::En => "⬆ Up Folder",
            Locale::Es => "⬆ Subir Carpeta",
            Locale::Fr => "⬆ Dossier Parent",
            Locale::It => "⬆ Livello Superiore",
            Locale::Ru => "⬆ Вверх",
        }
    }

    pub fn treemap_up_folder_tooltip(&self) -> &'static str {
        match self {
            Locale::Pt => "Volta um nível acima no diretório",
            Locale::En => "Go up one directory level",
            Locale::Es => "Sube un nivel en el directorio",
            Locale::Fr => "Remonte d'un niveau dans le répertoire",
            Locale::It => "Sale di un livello nella directory",
            Locale::Ru => "Перейти на уровень выше",
        }
    }

    pub fn treemap_refresh(&self) -> &'static str {
        match self {
            Locale::Pt => "🔄 Atualizar análise",
            Locale::En => "🔄 Refresh analysis",
            Locale::Es => "🔄 Actualizar análisis",
            Locale::Fr => "🔄 Actualiser l'analyse",
            Locale::It => "🔄 Aggiorna analisi",
            Locale::Ru => "🔄 Обновить анализ",
        }
    }

    pub fn treemap_refresh_tooltip(&self) -> &'static str {
        match self {
            Locale::Pt => "Lê os dados atualizados do disco para a pasta e recalcula o treemap",
            Locale::En => "Reads updated data from disk for the folder and recalculates treemap",
            Locale::Es => {
                "Lee los datos actualizados del disco para la carpeta y recalcula el treemap"
            }
            Locale::Fr => {
                "Lit les données à jour du disque pour le dossier et recalcule le treemap"
            }
            Locale::It => {
                "Legge i dati aggiornati dal disco per la cartella e ricalcola il treemap"
            }
            Locale::Ru => "Считывает обновленные данные с диска и пересчитывает treemap",
        }
    }

    pub fn treemap_last_analysis(&self, time_str: &str, source_str: &str) -> String {
        match self {
            Locale::Pt => format!("🕒 Última análise: {time_str} • Origem: {source_str}"),
            Locale::En => format!("🕒 Last analysis: {time_str} • Source: {source_str}"),
            Locale::Es => format!("🕒 Último análisis: {time_str} • Origen: {source_str}"),
            Locale::Fr => format!("🕒 Dernière analyse: {time_str} • Source: {source_str}"),
            Locale::It => format!("🕒 Ultima analisi: {time_str} • Origine: {source_str}"),
            Locale::Ru => format!("🕒 Последний анализ: {time_str} • Источник: {source_str}"),
        }
    }

    pub fn treemap_source_disk(&self) -> &'static str {
        match self {
            Locale::Pt => "Varredura de Disco",
            Locale::En => "Disk Scan",
            Locale::Es => "Escaneo de Disco",
            Locale::Fr => "Analyse du Disque",
            Locale::It => "Scansione Disco",
            Locale::Ru => "Сканирование диска",
        }
    }

    pub fn treemap_source_mft(&self) -> &'static str {
        match self {
            Locale::Pt => "NTFS MFT (Volume)",
            Locale::En => "NTFS MFT (Volume)",
            Locale::Es => "NTFS MFT (Volumen)",
            Locale::Fr => "NTFS MFT (Volume)",
            Locale::It => "NTFS MFT (Volume)",
            Locale::Ru => "NTFS MFT (Том)",
        }
    }

    pub fn treemap_cancel(&self) -> &'static str {
        match self {
            Locale::Pt => "❌ Cancelar varredura",
            Locale::En => "❌ Cancel scan",
            Locale::Es => "❌ Cancelar escaneo",
            Locale::Fr => "❌ Annuler l'analyse",
            Locale::It => "❌ Annulla scansione",
            Locale::Ru => "❌ Отменить сканирование",
        }
    }

    pub fn treemap_status_complete(&self) -> &'static str {
        match self {
            Locale::Pt => "✓ Completa",
            Locale::En => "✓ Complete",
            Locale::Es => "✓ Completa",
            Locale::Fr => "✓ Complète",
            Locale::It => "✓ Completa",
            Locale::Ru => "✓ Завершено",
        }
    }

    pub fn treemap_status_partial(&self, count: usize) -> String {
        match self {
            Locale::Pt => format!("Parcial ({count} pastas sem permissão)"),
            Locale::En => format!("Partial ({count} restricted folders)"),
            Locale::Es => format!("Parcial ({count} carpetas sin permiso)"),
            Locale::Fr => format!("Partielle ({count} dossiers restreints)"),
            Locale::It => format!("Parziale ({count} cartelle protette)"),
            Locale::Ru => format!("Частично ({count} папок без доступа)"),
        }
    }

    pub fn treemap_status_cancelled(&self, count: usize) -> String {
        match self {
            Locale::Pt => format!("Cancelada ({count} itens)"),
            Locale::En => format!("Cancelled ({count} items)"),
            Locale::Es => format!("Cancelada ({count} elementos)"),
            Locale::Fr => format!("Annulée ({count} éléments)"),
            Locale::It => format!("Annullata ({count} elementi)"),
            Locale::Ru => format!("Отменено ({count} элементов)"),
        }
    }

    pub fn treemap_status_failed(&self, err: &str) -> String {
        match self {
            Locale::Pt => format!("Falha: {err}"),
            Locale::En => format!("Failed: {err}"),
            Locale::Es => format!("Fallo: {err}"),
            Locale::Fr => format!("Échec: {err}"),
            Locale::It => format!("Fallito: {err}"),
            Locale::Ru => format!("Ошибка: {err}"),
        }
    }

    pub fn treemap_access_errors_title(&self) -> &'static str {
        match self {
            Locale::Pt => "Pastas com acesso negado ou erro:",
            Locale::En => "Folders with access denied or error:",
            Locale::Es => "Carpetas con acceso denegado o error:",
            Locale::Fr => "Dossiers avec accès refusé ou erreur:",
            Locale::It => "Cartelle con accesso negato o errore:",
            Locale::Ru => "Папки с ошибкой доступа:",
        }
    }

    pub fn treemap_scanning_stats(&self, items: usize, bytes_str: &str) -> String {
        match self {
            Locale::Pt => format!("{items} itens identificados • {bytes_str} processados"),
            Locale::En => format!("{items} items identified • {bytes_str} processed"),
            Locale::Es => format!("{items} elementos identificados • {bytes_str} procesados"),
            Locale::Fr => format!("{items} éléments identifiés • {bytes_str} traités"),
            Locale::It => format!("{items} elementi identificati • {bytes_str} elaborati"),
            Locale::Ru => format!("{items} элементов найдено • {bytes_str} обработано"),
        }
    }

    pub fn treemap_scanning_dir_label(&self, dir: &str) -> String {
        match self {
            Locale::Pt => format!("📂 Diretório em análise: {dir}"),
            Locale::En => format!("📂 Scanning directory: {dir}"),
            Locale::Es => format!("📂 Carpeta en análisis: {dir}"),
            Locale::Fr => format!("📂 Dossier en cours d'analyse: {dir}"),
            Locale::It => format!("📂 Cartella in scansione: {dir}"),
            Locale::Ru => format!("📂 Анализ папки: {dir}"),
        }
    }

    // --- Métricas de Tamanho e Projeção de Recuperação (TM-03) ---

    pub fn treemap_mode_logical(&self) -> &'static str {
        match self {
            Locale::Pt => "📊 Lógico",
            Locale::En => "📊 Logical",
            Locale::Es => "📊 Lógico",
            Locale::Fr => "📊 Logique",
            Locale::It => "📊 Logico",
            Locale::Ru => "📊 Логический",
        }
    }

    pub fn treemap_mode_allocated(&self) -> &'static str {
        match self {
            Locale::Pt => "💾 Em disco",
            Locale::En => "💾 On disk",
            Locale::Es => "💾 En disco",
            Locale::Fr => "💾 Sur disque",
            Locale::It => "💾 Su disco",
            Locale::Ru => "💾 На диске",
        }
    }

    pub fn treemap_mode_reclaimable(&self) -> &'static str {
        match self {
            Locale::Pt => "♻️ Recuperável",
            Locale::En => "♻️ Reclaimable",
            Locale::Es => "♻️ Recuperable",
            Locale::Fr => "♻️ Récupérable",
            Locale::It => "♻️ Recuperabile",
            Locale::Ru => "♻️ Освобождаемое",
        }
    }

    pub fn treemap_metric_logical_title(&self) -> &'static str {
        match self {
            Locale::Pt => "Tamanho Lógico",
            Locale::En => "Logical Size",
            Locale::Es => "Tamaño Lógico",
            Locale::Fr => "Taille Logique",
            Locale::It => "Dimensione Logica",
            Locale::Ru => "Логический размер",
        }
    }

    pub fn treemap_metric_allocated_title(&self) -> &'static str {
        match self {
            Locale::Pt => "Espaço no Disco",
            Locale::En => "Space on Disk",
            Locale::Es => "Espacio en Disco",
            Locale::Fr => "Espace sur Disque",
            Locale::It => "Spazio su Disco",
            Locale::Ru => "Место на диске",
        }
    }

    pub fn treemap_metric_reclaimable_title(&self) -> &'static str {
        match self {
            Locale::Pt => "Recuperação Estimada",
            Locale::En => "Estimated Reclamation",
            Locale::Es => "Recuperación Estimada",
            Locale::Fr => "Récupération Estimée",
            Locale::It => "Recupero Stimato",
            Locale::Ru => "Оценка освобождения",
        }
    }

    pub fn treemap_metric_unknown(&self) -> &'static str {
        match self {
            Locale::Pt => "— (Não disponível)",
            Locale::En => "— (Unavailable)",
            Locale::Es => "— (No disponible)",
            Locale::Fr => "— (Non disponible)",
            Locale::It => "— (Non disponibile)",
            Locale::Ru => "— (Недоступно)",
        }
    }

    pub fn treemap_badge_sparse(&self) -> &'static str {
        match self {
            Locale::Pt => "Esparso",
            Locale::En => "Sparse",
            Locale::Es => "Disperso",
            Locale::Fr => "Creux",
            Locale::It => "Sparso",
            Locale::Ru => "Разреженный",
        }
    }

    pub fn treemap_badge_compressed(&self) -> &'static str {
        match self {
            Locale::Pt => "Comprimido",
            Locale::En => "Compressed",
            Locale::Es => "Comprimido",
            Locale::Fr => "Compressé",
            Locale::It => "Compresso",
            Locale::Ru => "Сжатый",
        }
    }

    pub fn treemap_badge_offline(&self) -> &'static str {
        match self {
            Locale::Pt => "Nuvem / Desidratado",
            Locale::En => "Cloud / Offline",
            Locale::Es => "Nube / Deshidratado",
            Locale::Fr => "Cloud / Déshydraté",
            Locale::It => "Cloud / Disidratato",
            Locale::Ru => "Облачный / Автономный",
        }
    }

    pub fn treemap_badge_hardlink(&self) -> &'static str {
        match self {
            Locale::Pt => "Hard link compartilhado",
            Locale::En => "Shared hard link",
            Locale::Es => "Hard link compartido",
            Locale::Fr => "Lien matériel partagé",
            Locale::It => "Hard link condiviso",
            Locale::Ru => "Жесткая ссылка",
        }
    }

    pub fn treemap_hardlink_note(&self) -> &'static str {
        match self {
            Locale::Pt => "(Espaço retido por outros links no disco)",
            Locale::En => "(Space retained by other links on disk)",
            Locale::Es => "(Espacio retenido por otros enlaces en disco)",
            Locale::Fr => "(Espace retenu par d'autres liens sur disque)",
            Locale::It => "(Spazio trattenuto da altri link su disco)",
            Locale::Ru => "(Место удерживается другими ссылками)",
        }
    }

    pub fn treemap_offline_note(&self) -> &'static str {
        match self {
            Locale::Pt => "(Armazenado na nuvem: sem ocupação local)",
            Locale::En => "(Stored in cloud: no local disk usage)",
            Locale::Es => "(Almacenado en la nube: sin ocupación local)",
            Locale::Fr => "(Stocké dans le cloud : pas d'espace disque local)",
            Locale::It => "(Archiviato nel cloud: nessun uso disco locale)",
            Locale::Ru => "(В облаке: не занимает места на диске)",
        }
    }

    pub fn treemap_quick_links(&self) -> &'static str {
        match self {
            Locale::Pt => "Atalhos rápidos:",
            Locale::En => "Quick links:",
            Locale::Es => "Accesos rápidos:",
            Locale::Fr => "Raccourcis:",
            Locale::It => "Collegamenti rapidi:",
            Locale::Ru => "Быстрый переход:",
        }
    }

    pub fn treemap_shortcut_user(&self) -> &'static str {
        match self {
            Locale::Pt => "👤 Usuário",
            Locale::En => "👤 User",
            Locale::Es => "👤 Usuario",
            Locale::Fr => "👤 Utilisateur",
            Locale::It => "👤 Utente",
            Locale::Ru => "👤 Пользователь",
        }
    }

    pub fn treemap_shortcut_docs(&self) -> &'static str {
        match self {
            Locale::Pt => "📁 Documentos",
            Locale::En => "📁 Documents",
            Locale::Es => "📁 Documentos",
            Locale::Fr => "📁 Documents",
            Locale::It => "📁 Documenti",
            Locale::Ru => "📁 Документы",
        }
    }

    pub fn treemap_shortcut_downloads(&self) -> &'static str {
        match self {
            Locale::Pt => "⬇ Downloads",
            Locale::En => "⬇ Downloads",
            Locale::Es => "⬇ Descargas",
            Locale::Fr => "⬇ Téléchargements",
            Locale::It => "⬇ Download",
            Locale::Ru => "⬇ Загрузки",
        }
    }

    pub fn treemap_shortcut_desktop(&self) -> &'static str {
        match self {
            Locale::Pt => "🖥 Desktop",
            Locale::En => "🖥 Desktop",
            Locale::Es => "🖥 Escritorio",
            Locale::Fr => "🖥 Bureau",
            Locale::It => "🖥 Desktop",
            Locale::Ru => "🖥 Рабочий стол",
        }
    }

    pub fn treemap_total(&self, size_str: &str) -> String {
        format!("Total: {size_str}")
    }

    pub fn treemap_scanning_title(&self) -> &'static str {
        match self {
            Locale::Pt => "Varrendo pasta em segundo plano...",
            Locale::En => "Scanning directory in background...",
            Locale::Es => "Escaneando carpeta en segundo plano...",
            Locale::Fr => "Analyse du dossier en arrière-plan...",
            Locale::It => "Scansione cartella in background...",
            Locale::Ru => "Сканирование папки в фоновом режиме...",
        }
    }

    pub fn treemap_scanning_path(&self, path: &str) -> String {
        match self {
            Locale::Pt => format!("Calculando tamanhos e arquivos de: {path}"),
            Locale::En => format!("Calculating sizes and files for: {path}"),
            Locale::Es => format!("Calculando tamaños y archivos de: {path}"),
            Locale::Fr => format!("Calcul des tailles et fichiers de: {path}"),
            Locale::It => format!("Calcolo delle dimensioni e dei file per: {path}"),
            Locale::Ru => format!("Расчет размеров и файлов для: {path}"),
        }
    }

    pub fn treemap_scanning_hint(&self) -> &'static str {
        match self {
            Locale::Pt => "Você pode consultar a aba 'Resultados' ou 'Diff' normalmente enquanto o cálculo é concluído.",
            Locale::En => "You can continue using the 'Results' or 'Diff' tabs normally while calculation finishes.",
            Locale::Es => "Puede usar las pestañas 'Resultados' o 'Diff' normalmente mientras finaliza el cálculo.",
            Locale::Fr => "Vous pouvez utiliser les onglets 'Résultats' ou 'Diff' pendant que le calcul se termine.",
            Locale::It => "Puoi utilizzare le schede 'Risultati' o 'Diff' mentre il calcolo termina.",
            Locale::Ru => "Вы можете продолжать пользоваться вкладками 'Результаты' или 'Diff', пока расчет завершается.",
        }
    }

    pub fn treemap_empty(&self) -> &'static str {
        match self {
            Locale::Pt => "Nenhum arquivo ou pasta encontrado para exibição no Treemap.",
            Locale::En => "No files or folders found for Treemap display.",
            Locale::Es => "No se encontraron archivos ni carpetas para mostrar en el Treemap.",
            Locale::Fr => "Aucun fichier ou dossier trouvé pour l'affichage Treemap.",
            Locale::It => "Nessun file o cartella trovato per la visualizzazione Treemap.",
            Locale::Ru => "Файлы или папки для отображения в Treemap не найдены.",
        }
    }

    pub fn treemap_list_search_placeholder(&self) -> &'static str {
        match self {
            Locale::Pt => "🔍 Filtrar itens nesta pasta...",
            Locale::En => "🔍 Filter items in this folder...",
            Locale::Es => "🔍 Filtrar elementos en esta carpeta...",
            Locale::Fr => "🔍 Filtrer les éléments dans ce dossier...",
            Locale::It => "🔍 Filtra elementi in questa cartella...",
            Locale::Ru => "🔍 Фильтровать элементы в этой папке...",
        }
    }

    pub fn treemap_col_name(&self) -> &'static str {
        match self {
            Locale::Pt => "Nome",
            Locale::En => "Name",
            Locale::Es => "Nombre",
            Locale::Fr => "Nom",
            Locale::It => "Nome",
            Locale::Ru => "Имя",
        }
    }

    pub fn treemap_col_size(&self) -> &'static str {
        match self {
            Locale::Pt => "Tamanho",
            Locale::En => "Size",
            Locale::Es => "Tamaño",
            Locale::Fr => "Taille",
            Locale::It => "Dimensione",
            Locale::Ru => "Размер",
        }
    }

    pub fn treemap_col_percent(&self) -> &'static str {
        match self {
            Locale::Pt => "% da Pasta",
            Locale::En => "% of Folder",
            Locale::Es => "% de Carpeta",
            Locale::Fr => "% du Dossier",
            Locale::It => "% della Cartella",
            Locale::Ru => "% Папки",
        }
    }

    pub fn treemap_col_type(&self) -> &'static str {
        match self {
            Locale::Pt => "Tipo",
            Locale::En => "Type",
            Locale::Es => "Tipo",
            Locale::Fr => "Type",
            Locale::It => "Tipo",
            Locale::Ru => "Тип",
        }
    }

    pub fn treemap_col_modified(&self) -> &'static str {
        match self {
            Locale::Pt => "Modificado",
            Locale::En => "Modified",
            Locale::Es => "Modificado",
            Locale::Fr => "Modifié",
            Locale::It => "Modificato",
            Locale::Ru => "Изменен",
        }
    }

    pub fn treemap_folder_label(&self) -> &'static str {
        match self {
            Locale::Pt => "Pasta",
            Locale::En => "Folder",
            Locale::Es => "Carpeta",
            Locale::Fr => "Dossier",
            Locale::It => "Cartella",
            Locale::Ru => "Папка",
        }
    }

    pub fn treemap_file_label(&self) -> &'static str {
        match self {
            Locale::Pt => "Arquivo",
            Locale::En => "File",
            Locale::Es => "Archivo",
            Locale::Fr => "Fichier",
            Locale::It => "File",
            Locale::Ru => "Файл",
        }
    }

    pub fn treemap_filter_count(&self, matching: usize, total: usize) -> String {
        match self {
            Locale::Pt => format!("{matching} de {total} itens"),
            Locale::En => format!("{matching} of {total} items"),
            Locale::Es => format!("{matching} de {total} elementos"),
            Locale::Fr => format!("{matching} sur {total} éléments"),
            Locale::It => format!("{matching} su {total} elementi"),
            Locale::Ru => format!("{matching} из {total} элементов"),
        }
    }

    // --- Diff Lado a Lado ---

    pub fn diff_header(&self) -> &'static str {
        match self {
            Locale::Pt => "⚖️ Diff Lado a Lado",
            Locale::En => "⚖️ Side-by-Side Diff",
            Locale::Es => "⚖️ Diff Lado a Lado",
            Locale::Fr => "⚖️ Diff Côte à Côte",
            Locale::It => "⚖️ Diff Fianco a Fianco",
            Locale::Ru => "⚖️ Сравнение Diff",
        }
    }

    pub fn diff_additions(&self, n: usize) -> String {
        match self {
            Locale::Pt => format!("+{n} adições"),
            Locale::En => format!("+{n} additions"),
            Locale::Es => format!("+{n} adiciones"),
            Locale::Fr => format!("+{n} ajouts"),
            Locale::It => format!("+{n} aggiunte"),
            Locale::Ru => format!("+{n} добавлений"),
        }
    }

    pub fn diff_deletions(&self, n: usize) -> String {
        match self {
            Locale::Pt => format!("-{n} remoções"),
            Locale::En => format!("-{n} deletions"),
            Locale::Es => format!("-{n} eliminaciones"),
            Locale::Fr => format!("-{n} suppressions"),
            Locale::It => format!("-{n} rimozioni"),
            Locale::Ru => format!("-{n} удалений"),
        }
    }

    pub fn diff_identical(&self) -> &'static str {
        match self {
            Locale::Pt => "Arquivos são idênticos em conteúdo",
            Locale::En => "Files are identical in content",
            Locale::Es => "Los archivos son idénticos en contenido",
            Locale::Fr => "Les fichiers ont un contenu identique",
            Locale::It => "I file hanno contenuto identico",
            Locale::Ru => "Файлы полностью идентичны по содержимому",
        }
    }

    pub fn diff_binary(&self) -> &'static str {
        match self {
            Locale::Pt => "Arquivos binários diferentes (não comparáveis em texto puro)",
            Locale::En => "Binary files differ (cannot be compared as plain text)",
            Locale::Es => "Archivos binarios diferentes (no comparables en texto plano)",
            Locale::Fr => "Fichiers binaires différents (non comparables en texte)",
            Locale::It => "File binari differenti (non confrontabili in testo)",
            Locale::Ru => "Бинарные файлы различаются (текстовое сравнение недоступно)",
        }
    }

    pub fn diff_error(&self) -> &'static str {
        match self {
            Locale::Pt => "Erro ao abrir arquivos para diff",
            Locale::En => "Error opening files for diff",
            Locale::Es => "Error al abrir archivos para diff",
            Locale::Fr => "Erreur lors de l'ouverture des fichiers pour le diff",
            Locale::It => "Errore durante l'apertura dei file per il diff",
            Locale::Ru => "Ошибка при открытии файлов для diff",
        }
    }

    // --- Barra de Status Inferior ---

    pub fn status_mft_scan(&self) -> &'static str {
        match self {
            Locale::Pt => "✓ Varredura MFT Concluída",
            Locale::En => "✓ MFT Scan Complete",
            Locale::Es => "✓ Escaneo MFT Completado",
            Locale::Fr => "✓ Analyse MFT Terminée",
            Locale::It => "✓ Scansione MFT Completata",
            Locale::Ru => "✓ Сканирование MFT Завершено",
        }
    }

    pub fn status_local_scan(&self) -> &'static str {
        match self {
            Locale::Pt => "✓ Varredura Local Concluída",
            Locale::En => "✓ Local Scan Complete",
            Locale::Es => "✓ Escaneo Local Completado",
            Locale::Fr => "✓ Analyse Locale Terminée",
            Locale::It => "✓ Scansione Locale Completata",
            Locale::Ru => "✓ Локальное Сканирование Завершено",
        }
    }

    pub fn status_results_count(&self, filtered: usize, total: usize) -> String {
        match self {
            Locale::Pt => format!("{filtered} resultados de {total} arquivos indexados"),
            Locale::En => format!("{filtered} results out of {total} indexed files"),
            Locale::Es => format!("{filtered} resultados de {total} archivos indexados"),
            Locale::Fr => format!("{filtered} résultats sur {total} fichiers indexés"),
            Locale::It => format!("{filtered} risultati su {total} file indicizzati"),
            Locale::Ru => format!("{filtered} результатов из {total} проиндексированных файлов"),
        }
    }

    pub fn status_no_files_to_export(&self) -> &'static str {
        match self {
            Locale::Pt => "Nenhum arquivo nos resultados para exportar",
            Locale::En => "No files in results to export",
            Locale::Es => "No hay archivos en los resultados para exportar",
            Locale::Fr => "Aucun fichier dans les résultats à exporter",
            Locale::It => "Nessun file nei risultati da esportare",
            Locale::Ru => "Нет файлов в результатах для экспорта",
        }
    }

    pub fn status_csv_saved(&self, path: &str) -> String {
        match self {
            Locale::Pt => format!("✓ Resultados exportados para: {path}"),
            Locale::En => format!("✓ Results exported to: {path}"),
            Locale::Es => format!("✓ Resultados exportados a: {path}"),
            Locale::Fr => format!("✓ Résultats exportés vers: {path}"),
            Locale::It => format!("✓ Risultati esportati in: {path}"),
            Locale::Ru => format!("✓ Результаты экспортированы в: {path}"),
        }
    }

    pub fn status_csv_error(&self, err: &str) -> String {
        match self {
            Locale::Pt => format!("Erro ao exportar CSV: {err}"),
            Locale::En => format!("Error exporting CSV: {err}"),
            Locale::Es => format!("Error al exportar CSV: {err}"),
            Locale::Fr => format!("Erreur lors de l'export CSV: {err}"),
            Locale::It => format!("Errore durante l'esportazione CSV: {err}"),
            Locale::Ru => format!("Ошибка при экспорте CSV: {err}"),
        }
    }

    pub fn status_treemap_ready(&self, path: &str) -> String {
        match self {
            Locale::Pt => format!("✓ Treemap de {path} calculado com sucesso!"),
            Locale::En => format!("✓ Treemap for {path} calculated successfully!"),
            Locale::Es => format!("✓ ¡Treemap de {path} calculado con éxito!"),
            Locale::Fr => format!("✓ Treemap de {path} calculé avec succès!"),
            Locale::It => format!("✓ Treemap di {path} calcolato con successo!"),
            Locale::Ru => format!("✓ Treemap для {path} успешно рассчитан!"),
        }
    }

    pub fn status_treemap_error(&self, err: &str) -> String {
        match self {
            Locale::Pt => format!("Falha na varredura do Treemap: {err}"),
            Locale::En => format!("Treemap scan failed: {err}"),
            Locale::Es => format!("Fallo en el escaneo del Treemap: {err}"),
            Locale::Fr => format!("Échec de l'analyse Treemap: {err}"),
            Locale::It => format!("Scansione Treemap non riuscita: {err}"),
            Locale::Ru => format!("Сбой сканирования Treemap: {err}"),
        }
    }

    pub fn label_search_path(&self) -> &'static str {
        match self {
            Locale::Pt => "Pasta:",
            Locale::En => "Folder:",
            Locale::Es => "Carpeta:",
            Locale::Fr => "Dossier:",
            Locale::It => "Cartella:",
            Locale::Ru => "Папка:",
        }
    }

    pub fn btn_browse(&self) -> &'static str {
        match self {
            Locale::Pt => "Procurar...",
            Locale::En => "Browse...",
            Locale::Es => "Examinar...",
            Locale::Fr => "Parcourir...",
            Locale::It => "Sfoglia...",
            Locale::Ru => "Обзор...",
        }
    }
}
