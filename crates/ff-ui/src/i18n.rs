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

    // --- TM-05: Painel de Detalhes e Ações de Inspeção do Treemap ---

    pub fn treemap_details_header(&self) -> &'static str {
        match self {
            Locale::Pt => "🔍 Detalhes e Inspeção",
            Locale::En => "🔍 Details & Inspection",
            Locale::Es => "🔍 Detalles e Inspección",
            Locale::Fr => "🔍 Détails et Inspection",
            Locale::It => "🔍 Dettagli e Ispezione",
            Locale::Ru => "🔍 Детали и Инспекция",
        }
    }

    pub fn treemap_btn_open_location(&self) -> &'static str {
        match self {
            Locale::Pt => "📂 Abrir Localização",
            Locale::En => "📂 Open Location",
            Locale::Es => "📂 Abrir Ubicación",
            Locale::Fr => "📂 Ouvrir l'Emplacement",
            Locale::It => "📂 Apri Posizione",
            Locale::Ru => "📂 Открыть расположение",
        }
    }

    pub fn treemap_btn_copy_path(&self) -> &'static str {
        match self {
            Locale::Pt => "📋 Copiar Caminho",
            Locale::En => "📋 Copy Path",
            Locale::Es => "📋 Copiar Ruta",
            Locale::Fr => "📋 Copier le Chemin",
            Locale::It => "📋 Copia Percorso",
            Locale::Ru => "📋 Копировать путь",
        }
    }

    pub fn treemap_btn_preview(&self) -> &'static str {
        match self {
            Locale::Pt => "👁️ Pré-visualizar",
            Locale::En => "👁️ Preview",
            Locale::Es => "👁️ Vista Previa",
            Locale::Fr => "👁️ Aperçu",
            Locale::It => "👁️ Anteprima",
            Locale::Ru => "👁️ Предпросмотр",
        }
    }

    pub fn treemap_btn_search_here(&self) -> &'static str {
        match self {
            Locale::Pt => "🔍 Pesquisar Aqui",
            Locale::En => "🔍 Search Here",
            Locale::Es => "🔍 Buscar Aquí",
            Locale::Fr => "🔍 Rechercher Ici",
            Locale::It => "🔍 Cerca Qui",
            Locale::Ru => "🔍 Искать здесь",
        }
    }

    pub fn treemap_btn_compare(&self) -> &'static str {
        match self {
            Locale::Pt => "⚖️ Comparar Diff",
            Locale::En => "⚖️ Compare Diff",
            Locale::Es => "⚖️ Comparar Diff",
            Locale::Fr => "⚖️ Comparer Diff",
            Locale::It => "⚖️ Confronta Diff",
            Locale::Ru => "⚖️ Сравнить Diff",
        }
    }

    pub fn treemap_path_copied(&self) -> &'static str {
        match self {
            Locale::Pt => "📋 Caminho copiado para a área de transferência!",
            Locale::En => "📋 Path copied to clipboard!",
            Locale::Es => "📋 ¡Ruta copiada al portapapeles!",
            Locale::Fr => "📋 Chemin copié dans le presse-papiers !",
            Locale::It => "📋 Percorso copiato negli appunti!",
            Locale::Ru => "📋 Путь скопирован в буфер обмена!",
        }
    }

    pub fn treemap_item_missing_warning(&self) -> &'static str {
        match self {
            Locale::Pt => "⚠️ Item não encontrado no disco (pode ter sido excluído ou movido após a análise)",
            Locale::En => "⚠️ Item not found on disk (may have been deleted or moved after analysis)",
            Locale::Es => "⚠️ Elemento no encontrado en el disco (puede haber sido eliminado o movido tras el análisis)",
            Locale::Fr => "⚠️ Élément introuvable sur le disque (peut avoir été supprimé ou déplacé après l'analyse)",
            Locale::It => "⚠️ Elemento non trovato sul disco (potrebbe essere stato eliminato o spostato dopo l'analisi)",
            Locale::Ru => "⚠️ Элемент не найден на диске (возможно, был удален или перемещен после анализа)",
        }
    }

    pub fn treemap_details_status_exists(&self) -> &'static str {
        match self {
            Locale::Pt => "Presente no disco",
            Locale::En => "Present on disk",
            Locale::Es => "Presente en disco",
            Locale::Fr => "Présent sur le disque",
            Locale::It => "Presente sul disco",
            Locale::Ru => "Присутствует на диске",
        }
    }

    pub fn treemap_open_location_failed(&self) -> &'static str {
        match self {
            Locale::Pt => "Falha ao abrir localização do item",
            Locale::En => "Failed to open item location",
            Locale::Es => "Fallo al abrir la ubicación del elemento",
            Locale::Fr => "Échec de l'ouverture de l'emplacement",
            Locale::It => "Apertura della posizione non riuscita",
            Locale::Ru => "Не удалось открыть расположение элемента",
        }
    }

    pub fn treemap_largest_files_title(&self) -> &'static str {
        match self {
            Locale::Pt => "Maiores arquivos internos:",
            Locale::En => "Largest internal files:",
            Locale::Es => "Archivos más grandes:",
            Locale::Fr => "Fichiers les plus volumineux :",
            Locale::It => "File interni più grandi:",
            Locale::Ru => "Крупнейшие внутренние файлы:",
        }
    }

    pub fn treemap_internal_counts(&self, files: u64, dirs: u64) -> String {
        match self {
            Locale::Pt => format!("{files} arquivos, {dirs} pastas internas"),
            Locale::En => format!("{files} files, {dirs} subfolders"),
            Locale::Es => format!("{files} archivos, {dirs} subcarpetas"),
            Locale::Fr => format!("{files} fichiers, {dirs} sous-dossiers"),
            Locale::It => format!("{files} file, {dirs} sottocartelle"),
            Locale::Ru => format!("{files} файлов, {dirs} подпапок"),
        }
    }

    // --- Filtros Orientados à Recuperação de Espaço (TM-06) ---

    pub fn treemap_space_filter_title(&self) -> &'static str {
        match self {
            Locale::Pt => "🎯 Recuperação de Espaço:",
            Locale::En => "🎯 Space Recovery:",
            Locale::Es => "🎯 Recuperación de Espacio:",
            Locale::Fr => "🎯 Récupération d'Espace :",
            Locale::It => "🎯 Recupero Spazio:",
            Locale::Ru => "🎯 Освобождение места:",
        }
    }

    pub fn treemap_filter_all(&self) -> &'static str {
        match self {
            Locale::Pt => "Todos",
            Locale::En => "All",
            Locale::Es => "Todos",
            Locale::Fr => "Tous",
            Locale::It => "Tutti",
            Locale::Ru => "Все",
        }
    }

    pub fn treemap_filter_largest_files(&self) -> &'static str {
        match self {
            Locale::Pt => "🐘 Maiores Arquivos",
            Locale::En => "🐘 Largest Files",
            Locale::Es => "🐘 Archivos Más Grandes",
            Locale::Fr => "🐘 Plus Grands Fichiers",
            Locale::It => "🐘 File Più Grandi",
            Locale::Ru => "🐘 Крупнейшие файлы",
        }
    }

    pub fn treemap_filter_largest_files_tooltip(&self) -> &'static str {
        match self {
            Locale::Pt => "Filtra arquivos com tamanho maior ou igual a 100 MB.",
            Locale::En => "Filters files with size greater than or equal to 100 MB.",
            Locale::Es => "Filtra archivos de tamaño mayor o igual a 100 MB.",
            Locale::Fr => "Filtre les fichiers de taille supérieure ou égale à 100 Mo.",
            Locale::It => "Filtra i file con dimensione maggiore o uguale a 100 MB.",
            Locale::Ru => "Фильтрует файлы размером от 100 МБ.",
        }
    }

    pub fn treemap_filter_old_downloads(&self) -> &'static str {
        match self {
            Locale::Pt => "📥 Downloads Antigos",
            Locale::En => "📥 Old Downloads",
            Locale::Es => "📥 Descargas Antiguas",
            Locale::Fr => "📥 Téléchargements Anciens",
            Locale::It => "📥 Download Datati",
            Locale::Ru => "📥 Старые загрузки",
        }
    }

    pub fn treemap_filter_old_downloads_tooltip(&self) -> &'static str {
        match self {
            Locale::Pt => "Filtra arquivos na pasta de downloads ou cujo caminho contenha 'download' e com data de modificação superior a 30 dias.\n\nNota: Idade baseada exclusivamente na data da última modificação (mtime), não constituindo prova de desuso.",
            Locale::En => "Filters files in downloads folder or path containing 'download' modified over 30 days ago.\n\nNote: Age is based strictly on last modification timestamp (mtime), and does not prove lack of use.",
            Locale::Es => "Filtra archivos en la carpeta de descargas o con 'download' en la ruta modificados hace más de 30 días.\n\nNota: Edad basada exclusivamente en la fecha de modificación (mtime), no constituye prueba de desuso.",
            Locale::Fr => "Filtre les fichiers du dossier téléchargements ou contenant 'download' modifiés il y a plus de 30 jours.\n\nNote : Âge basé uniquement sur la date de modification (mtime), ne constitue pas une preuve de désuétude.",
            Locale::It => "Filtra i file nella cartella download o con percorso contenente 'download' modificati da oltre 30 giorni.\n\nNota: Età basata esclusivamente sulla data di modifica (mtime), non costituisce prova di inutilizzo.",
            Locale::Ru => "Фильтрует файлы в папке загрузок или с 'download' в пути, измененные более 30 дней назад.\n\nПримечание: Возраст основан строго на дате изменения (mtime), не является доказательством неиспользования.",
        }
    }

    pub fn treemap_filter_installers_images(&self) -> &'static str {
        match self {
            Locale::Pt => "💿 Instaladores e Imagens",
            Locale::En => "💿 Installers & Images",
            Locale::Es => "💿 Instaladores e Imágenes",
            Locale::Fr => "💿 Installeurs et Images",
            Locale::It => "💿 Installer e Immagini",
            Locale::Ru => "💿 Установщики и образы",
        }
    }

    pub fn treemap_filter_installers_images_tooltip(&self) -> &'static str {
        match self {
            Locale::Pt => "Filtra arquivos instaladores e imagens de disco (.iso, .img, .vmdk, .vhdx, .vdi, .msi, .exe, .pkg, .dmg).",
            Locale::En => "Filters installer packages and disk images (.iso, .img, .vmdk, .vhdx, .vdi, .msi, .exe, .pkg, .dmg).",
            Locale::Es => "Filtra paquetes instaladores e imágenes de disco (.iso, .img, .vmdk, .vhdx, .vdi, .msi, .exe, .pkg, .dmg).",
            Locale::Fr => "Filtre les paquets d'installation et images disque (.iso, .img, .vmdk, .vhdx, .vdi, .msi, .exe, .pkg, .dmg).",
            Locale::It => "Filtra pacchetti installer e immagini disco (.iso, .img, .vmdk, .vhdx, .vdi, .msi, .exe, .pkg, .dmg).",
            Locale::Ru => "Фильтрует установочные пакеты и образы дисков (.iso, .img, .vmdk, .vhdx, .vdi, .msi, .exe, .pkg, .dmg).",
        }
    }

    pub fn treemap_filter_large_videos(&self) -> &'static str {
        match self {
            Locale::Pt => "🎬 Vídeos Grandes",
            Locale::En => "🎬 Large Videos",
            Locale::Es => "🎬 Videos Grandes",
            Locale::Fr => "🎬 Grandes Vidéos",
            Locale::It => "🎬 Video Grandi",
            Locale::Ru => "🎬 Большие видео",
        }
    }

    pub fn treemap_filter_large_videos_tooltip(&self) -> &'static str {
        match self {
            Locale::Pt => "Filtra vídeos e gravações multimídia (.mp4, .mkv, .mov, .avi, .webm, .wmv, .flv, .m4v) com tamanho maior ou igual a 20 MB.",
            Locale::En => "Filters multimedia videos and recordings (.mp4, .mkv, .mov, .avi, .webm, .wmv, .flv, .m4v) with size greater than or equal to 20 MB.",
            Locale::Es => "Filtra videos y grabaciones multimedia (.mp4, .mkv, .mov, .avi, .webm, .wmv, .flv, .m4v) con tamaño mayor o igual a 20 MB.",
            Locale::Fr => "Filtre les vidéos et enregistrements multimédias (.mp4, .mkv, .mov, .avi, .webm, .wmv, .flv, .m4v) d'au moins 20 Mo.",
            Locale::It => "Filtra video e registrazioni multimediali (.mp4, .mkv, .mov, .avi, .webm, .wmv, .flv, .m4v) con dimensione maggiore o uguale a 20 MB.",
            Locale::Ru => "Фильтрует мультимедийные видео и записи (.mp4, .mkv, .mov, .avi, .webm, .wmv, .flv, .m4v) размером от 20 МБ.",
        }
    }

    pub fn treemap_filter_archives(&self) -> &'static str {
        match self {
            Locale::Pt => "📦 Compactados",
            Locale::En => "📦 Archives",
            Locale::Es => "📦 Comprimidos",
            Locale::Fr => "📦 Archives",
            Locale::It => "📦 Archivi",
            Locale::Ru => "📦 Архивы",
        }
    }

    pub fn treemap_filter_archives_tooltip(&self) -> &'static str {
        match self {
            Locale::Pt => "Filtra arquivos compactados (.zip, .tar, .gz, .7z, .rar, .tgz, .bz2, .xz, .cab).",
            Locale::En => "Filters compressed archive files (.zip, .tar, .gz, .7z, .rar, .tgz, .bz2, .xz, .cab).",
            Locale::Es => "Filtra archivos comprimidos (.zip, .tar, .gz, .7z, .rar, .tgz, .bz2, .xz, .cab).",
            Locale::Fr => "Filtre les fichiers d'archives compressées (.zip, .tar, .gz, .7z, .rar, .tgz, .bz2, .xz, .cab).",
            Locale::It => "Filtra file compressi di archivio (.zip, .tar, .gz, .7z, .rar, .tgz, .bz2, .xz, .cab).",
            Locale::Ru => "Фильтрует сжатые архивные файлы (.zip, .tar, .gz, .7z, .rar, .tgz, .bz2, .xz, .cab).",
        }
    }

    pub fn treemap_filter_not_modified_recently(&self) -> &'static str {
        match self {
            Locale::Pt => "⏳ Sem Modificação (>180d)",
            Locale::En => "⏳ Unmodified (>180d)",
            Locale::Es => "⏳ Sin Modificar (>180d)",
            Locale::Fr => "⏳ Non Modifié (>180j)",
            Locale::It => "⏳ Non Modificato (>180g)",
            Locale::Ru => "⏳ Без изменений (>180д)",
        }
    }

    pub fn treemap_filter_not_modified_recently_tooltip(&self) -> &'static str {
        match self {
            Locale::Pt => "Filtra itens cuja última modificação ocorreu há mais de 180 dias (~6 meses).\n\nNota: Idade baseada exclusivamente na data de modificação (mtime) do arquivo, não constituindo prova de desuso.",
            Locale::En => "Filters items whose last modification was over 180 days ago (~6 months).\n\nNote: Age is based strictly on file modification timestamp (mtime), and does not prove lack of use.",
            Locale::Es => "Filtra elementos cuya última modificación fue hace más de 180 días (~6 meses).\n\nNota: Edad basada exclusivamente en la fecha de modificación (mtime), no constituye prueba de desuso.",
            Locale::Fr => "Filtre les éléments dont la dernière modification remonte à plus de 180 jours (~6 mois).\n\nNote : Âge basé uniquement sur la date de modification (mtime), ne constitue pas une preuve de désuétude.",
            Locale::It => "Filtra elementi la cui ultima modifica risale a più di 180 giorni fa (~6 mesi).\n\nNota: Età basata esclusivamente sulla data di modifica (mtime), non costituisce prova di inutilizzo.",
            Locale::Ru => "Фильтрует элементы, последнее изменение которых произошло более 180 дней назад (~6 месяцев).\n\nПримечание: Возраст основан строго на дате изменения (mtime), не является доказательством неиспользования.",
        }
    }

    pub fn treemap_clear_filter(&self) -> &'static str {
        match self {
            Locale::Pt => "✖ Limpar Filtro",
            Locale::En => "✖ Clear Filter",
            Locale::Es => "✖ Limpiar Filtro",
            Locale::Fr => "✖ Effacer le Filtre",
            Locale::It => "✖ Cancella Filtro",
            Locale::Ru => "✖ Очистить фильтр",
        }
    }

    pub fn treemap_filter_criteria_title(&self) -> &'static str {
        match self {
            Locale::Pt => "Filtros combináveis:",
            Locale::En => "Combinable filters:",
            Locale::Es => "Filtros combinables:",
            Locale::Fr => "Filtres combinables :",
            Locale::It => "Filtri combinabili:",
            Locale::Ru => "Комбинируемые фильтры:",
        }
    }

    pub fn treemap_stats_filtered_vs_total(
        &self,
        filtered_count: usize,
        filtered_size: &str,
        total_count: usize,
        total_size: &str,
    ) -> String {
        match self {
            Locale::Pt => format!("Exibindo {filtered_count} itens ({filtered_size}) • Total da pasta: {total_count} itens ({total_size})"),
            Locale::En => format!("Showing {filtered_count} items ({filtered_size}) • Folder total: {total_count} items ({total_size})"),
            Locale::Es => format!("Mostrando {filtered_count} elementos ({filtered_size}) • Total carpeta: {total_count} elementos ({total_size})"),
            Locale::Fr => format!("Affichage de {filtered_count} éléments ({filtered_size}) • Total dossier : {total_count} éléments ({total_size})"),
            Locale::It => format!("Visualizzati {filtered_count} elementi ({filtered_size}) • Totale cartella: {total_count} elementi ({total_size})"),
            Locale::Ru => format!("Показано {filtered_count} элементов ({filtered_size}) • Всего в папке: {total_count} элементов ({total_size})"),
        }
    }

    pub fn treemap_no_items_match_filter(&self) -> &'static str {
        match self {
            Locale::Pt => "Nenhum item corresponde aos critérios do filtro nesta pasta.",
            Locale::En => "No items match the filter criteria in this folder.",
            Locale::Es => "Ningún elemento coincide con los criterios del filtro en esta carpeta.",
            Locale::Fr => "Aucun élément ne correspond aux critères du filtre dans ce dossier.",
            Locale::It => "Nessun elemento corrisponde ai criteri del filtro in questa cartella.",
            Locale::Ru => "Нет элементов, соответствующих критериям фильтра в этой папке.",
        }
    }

    // --- Diagnóstico de Limpeza e Categorias (TM-07) ---

    pub fn treemap_cleanup_title(&self) -> &'static str {
        match self {
            Locale::Pt => "Oportunidade de Limpeza:",
            Locale::En => "Cleanup Opportunity:",
            Locale::Es => "Oportunidad de Limpieza:",
            Locale::Fr => "Opportunité de Nettoyage :",
            Locale::It => "Opportunità di Pulizia:",
            Locale::Ru => "Возможность очистки:",
        }
    }

    pub fn treemap_cleanup_rule(&self, rule_id: &str) -> String {
        match self {
            Locale::Pt => format!("Regra: {rule_id}"),
            Locale::En => format!("Rule: {rule_id}"),
            Locale::Es => format!("Regla: {rule_id}"),
            Locale::Fr => format!("Règle : {rule_id}"),
            Locale::It => format!("Regola: {rule_id}"),
            Locale::Ru => format!("Правило: {rule_id}"),
        }
    }

    pub fn treemap_cleanup_reason_title(&self) -> &'static str {
        match self {
            Locale::Pt => "Motivo",
            Locale::En => "Reason",
            Locale::Es => "Motivo",
            Locale::Fr => "Motif",
            Locale::It => "Motivo",
            Locale::Ru => "Причина",
        }
    }

    pub fn treemap_cleanup_consequence_title(&self) -> &'static str {
        match self {
            Locale::Pt => "Consequência provável",
            Locale::En => "Likely consequence",
            Locale::Es => "Consecuencia probable",
            Locale::Fr => "Conséquence probable",
            Locale::It => "Conseguenza probabile",
            Locale::Ru => "Вероятные последствия",
        }
    }

    pub fn treemap_cleanup_action_title(&self) -> &'static str {
        match self {
            Locale::Pt => "Ação recomendada",
            Locale::En => "Recommended action",
            Locale::Es => "Acción recomendada",
            Locale::Fr => "Action recommandée",
            Locale::It => "Azione consigliata",
            Locale::Ru => "Рекомендуемое действие",
        }
    }

    pub fn treemap_cleanup_safe_badge(&self) -> &'static str {
        match self {
            Locale::Pt => "✓ Descarte Seguro (Reconstituível)",
            Locale::En => "✓ Safe to Delete (Reconstructible)",
            Locale::Es => "✓ Descarte Seguro (Reconstruible)",
            Locale::Fr => "✓ Suppression Sûre (Reconstructible)",
            Locale::It => "✓ Eliminazione Sicura (Ricostruibile)",
            Locale::Ru => "✓ Безопасно для удаления (Восстановимо)",
        }
    }

    pub fn treemap_cleanup_manual_badge(&self) -> &'static str {
        match self {
            Locale::Pt => "🔍 Revisão Manual Necessária",
            Locale::En => "🔍 Manual Review Required",
            Locale::Es => "🔍 Revisión Manual Requerida",
            Locale::Fr => "🔍 Révision Manuelle Requise",
            Locale::It => "🔍 Revisione Manuale Necessaria",
            Locale::Ru => "🔍 Требуется ручная проверка",
        }
    }

    pub fn treemap_cleanup_personal_badge(&self) -> &'static str {
        match self {
            Locale::Pt => "👤 Conteúdo Pessoal",
            Locale::En => "👤 Personal Content",
            Locale::Es => "👤 Contenido Personal",
            Locale::Fr => "👤 Contenu Personnel",
            Locale::It => "👤 Contenuto Personale",
            Locale::Ru => "👤 Личные данные",
        }
    }

    pub fn treemap_cleanup_app_badge(&self) -> &'static str {
        match self {
            Locale::Pt => "📦 Gerenciado por Aplicação",
            Locale::En => "📦 Application-Managed",
            Locale::Es => "📦 Gestionado por Aplicación",
            Locale::Fr => "📦 Géré par l'Application",
            Locale::It => "📦 Gestito dall'Applicazione",
            Locale::Ru => "📦 Управляется приложением",
        }
    }

    pub fn treemap_cleanup_system_badge(&self) -> &'static str {
        match self {
            Locale::Pt => "🛡️ Protegido pelo Sistema",
            Locale::En => "🛡️ System-Protected",
            Locale::Es => "🛡️ Protegido por el Sistema",
            Locale::Fr => "🛡️ Protégé par le Système",
            Locale::It => "🛡️ Protetto dal Sistema",
            Locale::Ru => "🛡️ Защищено системой",
        }
    }

    // TM-08: Localizações para integração de duplicados à análise visual
    pub fn treemap_btn_dupes(&self) -> &'static str {
        match self {
            Locale::Pt => "🔎 Duplicados",
            Locale::En => "🔎 Duplicates",
            Locale::Es => "🔎 Duplicados",
            Locale::Fr => "🔎 Doublons",
            Locale::It => "🔎 Duplicati",
            Locale::Ru => "🔎 Дубликаты",
        }
    }

    pub fn treemap_dupes_title(&self) -> &'static str {
        match self {
            Locale::Pt => "Análise Visual de Arquivos Duplicados",
            Locale::En => "Visual Duplicate Files Analysis",
            Locale::Es => "Análisis Visual de Archivos Duplicados",
            Locale::Fr => "Analyse Visuelle des Fichiers en Double",
            Locale::It => "Analisi Visiva dei File Duplicati",
            Locale::Ru => "Визуальный анализ дубликатов файлов",
        }
    }

    pub fn treemap_dupes_start_scan(&self) -> &'static str {
        match self {
            Locale::Pt => "🔎 Iniciar Análise de Duplicados",
            Locale::En => "🔎 Start Duplicate Scan",
            Locale::Es => "🔎 Iniciar Análisis de Duplicados",
            Locale::Fr => "🔎 Démarrer l'Analyse des Doublons",
            Locale::It => "🔎 Avvia Analisi Duplicati",
            Locale::Ru => "🔎 Начать поиск дубликатов",
        }
    }

    pub fn treemap_dupes_scanning(&self) -> &'static str {
        match self {
            Locale::Pt => "Analisando duplicados por conteúdo (hashing sob demanda)...",
            Locale::En => "Scanning duplicates by content (on-demand hashing)...",
            Locale::Es => "Analizando duplicados por contenido (hashing bajo demanda)...",
            Locale::Fr => "Analyse des doublons par contenu (hachage à la demande)...",
            Locale::It => "Analisi dei duplicati per contenuto (hashing su richiesta)...",
            Locale::Ru => "Поиск дубликатов по содержимому (хеширование по требованию)...",
        }
    }

    pub fn treemap_dupes_cancel(&self) -> &'static str {
        match self {
            Locale::Pt => "⏹ Cancelar Análise",
            Locale::En => "⏹ Cancel Scan",
            Locale::Es => "⏹ Cancelar Análisis",
            Locale::Fr => "⏹ Annuler l'Analyse",
            Locale::It => "⏹ Annulla Analisi",
            Locale::Ru => "⏹ Отмена поиска",
        }
    }

    pub fn treemap_dupes_no_dupes(&self) -> &'static str {
        match self {
            Locale::Pt => "Nenhum arquivo duplicado encontrado nesta pasta.",
            Locale::En => "No duplicate files found in this folder.",
            Locale::Es => "No se encontraron archivos duplicados en esta carpeta.",
            Locale::Fr => "Aucun fichier en double trouvé dans ce dossier.",
            Locale::It => "Nessun file duplicato trovato in questa cartella.",
            Locale::Ru => "В этой папке не найдено дубликатов файлов.",
        }
    }

    pub fn treemap_dupes_reclaimable_physical(&self, size_str: &str) -> String {
        match self {
            Locale::Pt => format!("Recuperação física real: {size_str}"),
            Locale::En => format!("Real physical reclaim: {size_str}"),
            Locale::Es => format!("Recuperación física real: {size_str}"),
            Locale::Fr => format!("Récupération physique réelle : {size_str}"),
            Locale::It => format!("Recupero fisico effettivo: {size_str}"),
            Locale::Ru => format!("Реальное физ. освобождение: {size_str}"),
        }
    }

    pub fn treemap_dupes_nominal_wasted(&self, size_str: &str) -> String {
        match self {
            Locale::Pt => format!("Desperdício nominal: {size_str}"),
            Locale::En => format!("Nominal wasted: {size_str}"),
            Locale::Es => format!("Desperdicio nominal: {size_str}"),
            Locale::Fr => format!("Gaspillage nominal : {size_str}"),
            Locale::It => format!("Spreco nominale: {size_str}"),
            Locale::Ru => format!("Номинальные потери: {size_str}"),
        }
    }

    pub fn treemap_dupes_hardlink_info(&self) -> &'static str {
        match self {
            Locale::Pt => "🔗 Hard Link (mesmos blocos físicos no disco; remoção deste link não libera espaço físico)",
            Locale::En => "🔗 Hard Link (shares physical blocks; removing this link does not free physical space)",
            Locale::Es => "🔗 Hard Link (comparte bloques físicos; eliminar este enlace no libera espacio físico)",
            Locale::Fr => "🔗 Lien physique (partage les mêmes blocs ; supprimer ce lien ne libère pas d'espace physique)",
            Locale::It => "🔗 Hard Link (condivide blocchi fisici; rimuovere questo link non libera spazio fisico)",
            Locale::Ru => "🔗 Жесткая ссылка (общие блоки; удаление этой ссылки не освобождает физ. место)",
        }
    }

    pub fn treemap_dupes_independent_info(&self) -> &'static str {
        match self {
            Locale::Pt => "📄 Cópia Independente (blocos físicos dedicados no disco)",
            Locale::En => "📄 Independent Copy (dedicated physical blocks on disk)",
            Locale::Es => "📄 Copia Independiente (bloques físicos dedicados en disco)",
            Locale::Fr => "📄 Copie Indépendante (blocs physiques dédiés sur disque)",
            Locale::It => "📄 Copia Indipendente (blocchi fisici dedicati su disco)",
            Locale::Ru => "📄 Независимая копия (выделенные физические блоки на диске)",
        }
    }

    pub fn treemap_dupes_preserve_checkbox(&self) -> &'static str {
        match self {
            Locale::Pt => "Preservar esta cópia",
            Locale::En => "Preserve this copy",
            Locale::Es => "Preservar esta copia",
            Locale::Fr => "Préserver cette copie",
            Locale::It => "Conserva questa copia",
            Locale::Ru => "Сохранить эту копию",
        }
    }

    pub fn treemap_dupes_personal_banner(&self) -> &'static str {
        match self {
            Locale::Pt => "👤 Conteúdo Pessoal identificado: A aplicação nunca seleciona arquivos pessoais automaticamente para remoção. A decisão de preservação é estritamente manual do usuário.",
            Locale::En => "👤 Personal Content identified: The application never automatically selects personal files for removal. Preservation choice is strictly manual.",
            Locale::Es => "👤 Contenido Personal identificado: La aplicación nunca selecciona archivos personales automáticamente para eliminación. La decisión es estrictamente manual.",
            Locale::Fr => "👤 Contenu Personnel identifié : L'application ne sélectionne jamais automatiquement de fichiers personnels pour suppression. Le choix est strictement manuel.",
            Locale::It => "👤 Contenuto Personale identificato: L'applicazione non seleziona mai file personali per l'eliminazione automatica. La scelta è strettamente manuale.",
            Locale::Ru => "👤 Обнаружены личные данные: Приложение никогда не выбирает личные файлы для удаления автоматически. Решение о сохранении принимает только пользователь.",
        }
    }

    pub fn treemap_dupes_stale_alert(&self) -> &'static str {
        match self {
            Locale::Pt => "⚠️ Alterações externas detectadas: Arquivo(s) modificado(s) ou ausente(s) após a análise. Clique em Revalidar.",
            Locale::En => "⚠️ External changes detected: File(s) modified or missing after analysis. Click Revalidate.",
            Locale::Es => "⚠️ Cambios externos detectados: Archivo(s) modificado(s) o ausente(s) tras el análisis. Haga clic en Revalidar.",
            Locale::Fr => "⚠️ Modifications externes détectées : Fichier(s) modifié(s) ou manquant(s) après l'analyse. Cliquez sur Revalider.",
            Locale::It => "⚠️ Modifiche esterne rilevate: File modificati o mancanti dopo l'analisi. Clicca su Riconvalida.",
            Locale::Ru => "⚠️ Обнаружены внешние изменения: Файлы изменены или отсутствуют после анализа. Нажмите 'Проверить'.",
        }
    }

    pub fn treemap_dupes_btn_revalidate(&self) -> &'static str {
        match self {
            Locale::Pt => "🔄 Revalidar Grupo",
            Locale::En => "🔄 Revalidate Group",
            Locale::Es => "🔄 Revalidar Grupo",
            Locale::Fr => "🔄 Revalider le Groupe",
            Locale::It => "🔄 Riconvalida Gruppo",
            Locale::Ru => "🔄 Проверить группу",
        }
    }

    pub fn treemap_dupes_compare_diff_tooltip(&self) -> &'static str {
        match self {
            Locale::Pt => "Comparar cópias lado a lado no visualizador de Diff",
            Locale::En => "Compare copies side-by-side in Diff viewer",
            Locale::Es => "Comparar copias lado a lado en el visor de Diff",
            Locale::Fr => "Comparer les copies côte à côte dans le visualiseur de Diff",
            Locale::It => "Confronta le copie affiancate nel visualizzatore Diff",
            Locale::Ru => "Сравнить копии бок о бок в просмотрщике Diff",
        }
    }

    // TM-09: Localizações para lista de revisão e simulação de recuperação
    pub fn treemap_btn_review_list(&self) -> &'static str {
        match self {
            Locale::Pt => "📋 Lista de Revisão",
            Locale::En => "📋 Review List",
            Locale::Es => "📋 Lista de Revisión",
            Locale::Fr => "📋 Liste de Révision",
            Locale::It => "📋 Lista di Revisione",
            Locale::Ru => "📋 Список пересмотра",
        }
    }

    pub fn treemap_review_title(&self) -> &'static str {
        match self {
            Locale::Pt => "Simulação de Recuperação e Lista de Revisão",
            Locale::En => "Space Recovery Simulation & Review List",
            Locale::Es => "Simulación de Recuperación y Lista de Revisión",
            Locale::Fr => "Simulation de Récupération et Liste de Révision",
            Locale::It => "Simulazione di Recupero e Lista di Revisione",
            Locale::Ru => "Симуляция освобождения и список пересмотра",
        }
    }

    pub fn treemap_review_advisory_banner(&self) -> &'static str {
        match self {
            Locale::Pt => "ℹ️ Modo Exclusivamente Consultivo: O FastFile nunca executa exclusão ou alteração de arquivos no disco. Esta simulação projeta o espaço liberável com base na sua seleção.",
            Locale::En => "ℹ️ Strictly Advisory Mode: FastFile never deletes or alters disk files. This simulation projects recoverable space based on your selection.",
            Locale::Es => "ℹ️ Modo Exclusivamente Consultivo: FastFile nunca elimina ni modifica archivos en el disco. Esta simulación proyecta el espacio liberable según su selección.",
            Locale::Fr => "ℹ️ Mode Exclusivement Consultatif : FastFile ne supprime ni ne modifie jamais les fichiers sur le disque. Cette simulation projette l'espace libérable selon votre sélection.",
            Locale::It => "ℹ️ Modalità Esclusivamente Consultiva: FastFile non elimina né modifica mai i file su disco. Questa simulazione proietta lo spazio liberabile in base alla selezione.",
            Locale::Ru => "ℹ️ Исключительно консультативный режим: FastFile никогда не удаляет и не изменяет файлы на диске. Данная симуляция рассчитывает освобождаемое место на основе вашего выбора.",
        }
    }

    pub fn treemap_review_selected_nominal(&self, size_str: &str) -> String {
        match self {
            Locale::Pt => format!("Tamanho Selecionado (Nominal): {size_str}"),
            Locale::En => format!("Selected Size (Nominal): {size_str}"),
            Locale::Es => format!("Tamaño Seleccionado (Nominal): {size_str}"),
            Locale::Fr => format!("Taille Sélectionnée (Nominale) : {size_str}"),
            Locale::It => format!("Dimensione Selezionata (Nominale): {size_str}"),
            Locale::Ru => format!("Выбранный размер (номинал): {size_str}"),
        }
    }

    pub fn treemap_review_estimated_reclaimable(&self, size_str: &str) -> String {
        match self {
            Locale::Pt => format!("Recuperação Física Estimada: {size_str}"),
            Locale::En => format!("Estimated Physical Reclaim: {size_str}"),
            Locale::Es => format!("Recuperación Física Estimada: {size_str}"),
            Locale::Fr => format!("Récupération Physique Estimée : {size_str}"),
            Locale::It => format!("Recupero Fisico Stimato: {size_str}"),
            Locale::Ru => format!("Оценочное физическое освобождение: {size_str}"),
        }
    }

    pub fn treemap_review_already_reclaimed(&self, size_str: &str) -> String {
        match self {
            Locale::Pt => format!("Espaço Já Recuperado: {size_str} (Nenhuma alteração em disco)"),
            Locale::En => format!("Already Reclaimed Space: {size_str} (No disk changes made)"),
            Locale::Es => format!("Espacio Ya Recuperado: {size_str} (Ningún cambio en disco)"),
            Locale::Fr => {
                format!("Espace Déjà Récupéré : {size_str} (Aucune modification sur disque)")
            }
            Locale::It => format!("Spazio Già Recuperato: {size_str} (Nessuna modifica su disco)"),
            Locale::Ru => format!("Уже освобождено: {size_str} (Без изменений на диске)"),
        }
    }

    pub fn treemap_review_covered_badge(&self, ancestor: &str) -> String {
        match self {
            Locale::Pt => format!("(Coberto pela pasta pai: {ancestor})"),
            Locale::En => format!("(Covered by parent directory: {ancestor})"),
            Locale::Es => format!("(Cubierto por la carpeta padre: {ancestor})"),
            Locale::Fr => format!("(Couvert par le dossier parent : {ancestor})"),
            Locale::It => format!("(Coperto dalla cartella padre: {ancestor})"),
            Locale::Ru => format!("(Покрыто родительской папкой: {ancestor})"),
        }
    }

    pub fn treemap_review_coverage_info(&self, count: usize) -> String {
        match self {
            Locale::Pt => format!("{count} item(ns) estão contidos em pastas ancestrais já selecionadas e tiveram sua contagem deduplicada (sem soma dupla)."),
            Locale::En => format!("{count} item(s) are inside already selected ancestor folders and were deduplicated (no double counting)."),
            Locale::Es => format!("{count} elemento(s) están dentro de carpetas ancestrales ya seleccionadas y se deduplicaron (sin conteo doble)."),
            Locale::Fr => format!("{count} élément(s) sont dans des dossiers parents déjà sélectionnés et ont été dédupliqués (aucun double compte)."),
            Locale::It => format!("{count} elemento/i sono all'interno di cartelle genitrici già selezionate e sono stati deduplicati (nessun doppio conteggio)."),
            Locale::Ru => format!("{count} элемент(ов) находятся внутри уже выбранных папок и дедуплицированы (без двойного счета)."),
        }
    }

    pub fn treemap_review_stale_badge(&self) -> &'static str {
        match self {
            Locale::Pt => "⚠ Modificado no disco",
            Locale::En => "⚠ Modified on disk",
            Locale::Es => "⚠ Modificado en disco",
            Locale::Fr => "⚠ Modifié sur disque",
            Locale::It => "⚠ Modificato su disco",
            Locale::Ru => "⚠ Изменено на диске",
        }
    }

    pub fn treemap_review_missing_badge(&self) -> &'static str {
        match self {
            Locale::Pt => "❌ Inexistente no disco",
            Locale::En => "❌ Missing on disk",
            Locale::Es => "❌ Inexistente en disco",
            Locale::Fr => "❌ Manquant sur disque",
            Locale::It => "❌ Inesistente su disco",
            Locale::Ru => "❌ Отсутствует на диске",
        }
    }

    pub fn treemap_review_add_candidate(&self) -> &'static str {
        match self {
            Locale::Pt => "➕ Adicionar à Revisão",
            Locale::En => "➕ Add to Review",
            Locale::Es => "➕ Agregar a Revisión",
            Locale::Fr => "➕ Ajouter à la Révision",
            Locale::It => "➕ Aggiungi a Revisione",
            Locale::Ru => "➕ В список пересмотра",
        }
    }

    pub fn treemap_review_remove_candidate(&self) -> &'static str {
        match self {
            Locale::Pt => "➖ Remover da Revisão",
            Locale::En => "➖ Remove from Review",
            Locale::Es => "➖ Quitar de Revisión",
            Locale::Fr => "➖ Retirer de la Révision",
            Locale::It => "➖ Rimuovi da Revisione",
            Locale::Ru => "➖ Удалить из списка",
        }
    }

    pub fn treemap_review_clear_all(&self) -> &'static str {
        match self {
            Locale::Pt => "🗑 Limpar Lista",
            Locale::En => "🗑 Clear List",
            Locale::Es => "🗑 Limpiar Lista",
            Locale::Fr => "🗑 Vider la Liste",
            Locale::It => "🗑 Svuota Lista",
            Locale::Ru => "🗑 Очистить список",
        }
    }

    pub fn treemap_review_export_csv(&self) -> &'static str {
        match self {
            Locale::Pt => "💾 Exportar Simulação (CSV)",
            Locale::En => "💾 Export Simulation (CSV)",
            Locale::Es => "💾 Exportar Simulación (CSV)",
            Locale::Fr => "💾 Exporter Simulation (CSV)",
            Locale::It => "💾 Esporta Simulazione (CSV)",
            Locale::Ru => "💾 Экспорт симуляции (CSV)",
        }
    }

    pub fn treemap_review_empty_list(&self) -> &'static str {
        match self {
            Locale::Pt => "Nenhum item na lista de revisão. Marque caixas de seleção na tabela ou clique em 'Adicionar à Revisão' nos detalhes.",
            Locale::En => "No items in the review list. Check items in the table or click 'Add to Review' in the details panel.",
            Locale::Es => "Ningún elemento en la lista de revisión. Marque casillas en la tabla o haga clic en 'Agregar a Revisión' en detalles.",
            Locale::Fr => "Aucun élément dans la liste de révision. Cochez des éléments dans le tableau ou cliquez sur 'Ajouter à la Révision' dans les détails.",
            Locale::It => "Nessun elemento nella lista di revisione. Seleziona elementi nella tabella o clicca su 'Aggiungi a Revisione' nei dettagli.",
            Locale::Ru => "Нет элементов в списке пересмотра. Отметьте элементы в таблице или нажмите 'В список пересмотра' на панели деталей.",
        }
    }

    // --- TM-10: Modos de Cor e Legenda com Indicadores Textuais ---

    pub fn treemap_color_mode_label(&self) -> &'static str {
        match self {
            Locale::Pt => "🎨 Cores:",
            Locale::En => "🎨 Colors:",
            Locale::Es => "🎨 Colores:",
            Locale::Fr => "🎨 Couleurs :",
            Locale::It => "🎨 Colori:",
            Locale::Ru => "🎨 Цвета:",
        }
    }

    pub fn treemap_color_mode_filetype(&self) -> &'static str {
        match self {
            Locale::Pt => "📁 Tipo de Arquivo",
            Locale::En => "📁 File Type",
            Locale::Es => "📁 Tipo de Archivo",
            Locale::Fr => "📁 Type de Fichier",
            Locale::It => "📁 Tipo di File",
            Locale::Ru => "📁 Тип файла",
        }
    }

    pub fn treemap_color_mode_age(&self) -> &'static str {
        match self {
            Locale::Pt => "⏳ Idade de Modificação",
            Locale::En => "⏳ Modification Age",
            Locale::Es => "⏳ Edad de Modificación",
            Locale::Fr => "⏳ Âge de Modification",
            Locale::It => "⏳ Età di Modifica",
            Locale::Ru => "⏳ Возраст изменения",
        }
    }

    pub fn treemap_color_mode_cleanup(&self) -> &'static str {
        match self {
            Locale::Pt => "⚡ Oportunidade de Limpeza",
            Locale::En => "⚡ Cleanup Category",
            Locale::Es => "⚡ Oportunidad de Limpieza",
            Locale::Fr => "⚡ Opportunité de Nettoyage",
            Locale::It => "⚡ Opportunità di Pulizia",
            Locale::Ru => "⚡ Категория очистки",
        }
    }

    pub fn treemap_legend_label(&self) -> &'static str {
        match self {
            Locale::Pt => "Legenda:",
            Locale::En => "Legend:",
            Locale::Es => "Leyenda:",
            Locale::Fr => "Légende :",
            Locale::It => "Legenda:",
            Locale::Ru => "Легенда:",
        }
    }

    pub fn treemap_legend_tooltip(&self) -> &'static str {
        match self {
            Locale::Pt => "Legenda cromática e indicadores textuais para acessibilidade e significado explícito (TM-10)",
            Locale::En => "Color legend and textual indicators for accessibility and explicit meaning (TM-10)",
            Locale::Es => "Leyenda cromática e indicadores textuales para accesibilidad y significado explícito (TM-10)",
            Locale::Fr => "Légende chromatique et indicateurs textuels pour l'accessibilité et signification explicite (TM-10)",
            Locale::It => "Legenda cromatica e indicatori testuali per accessibilità e significato esplicito (TM-10)",
            Locale::Ru => "Цветовая легенда и текстовые индикаторы для доступности и однозначного понимания (TM-10)",
        }
    }

    pub fn treemap_col_color_badge(&self) -> &'static str {
        match self {
            Locale::Pt => "Classe",
            Locale::En => "Class",
            Locale::Es => "Clase",
            Locale::Fr => "Classe",
            Locale::It => "Classe",
            Locale::Ru => "Класс",
        }
    }
}
