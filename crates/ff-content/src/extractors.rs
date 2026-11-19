//! `extractors`
//!
//! Módulo de extração de texto para documentos enriquecidos (DOCX e PDF)
//! para permitir busca em conteúdo e pré-visualização instantânea (Issue #16).

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::reader::Reader;

/// Limite do `word/document.xml` descompactado (defesa contra *zip bombs*).
pub const MAX_DOCX_XML_BYTES: u64 = 32 * 1024 * 1024;

/// Limite do tamanho de um PDF carregado em memória para extração de texto.
pub const MAX_PDF_BYTES: u64 = 256 * 1024 * 1024;

/// Extrai o texto legível de um arquivo Word (.docx).
///
/// Arquivos DOCX são arquivos compactados em formato PKZip contendo
/// estruturas XML, sendo `word/document.xml` o container principal de texto.
pub fn extract_docx_text<P: AsRef<Path>>(path: P) -> io::Result<String> {
    let file = File::open(path.as_ref())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("erro ao abrir zip docx: {e}"),
        )
    })?;

    // Procura por word/document.xml
    let mut doc_file = match archive.by_name("word/document.xml") {
        Ok(f) => f,
        Err(_) => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "arquivo word/document.xml não encontrado no pacote DOCX",
            ));
        }
    };

    // O tamanho declarado no zip não é confiável: limita a leitura real.
    let mut xml_bytes = Vec::new();
    doc_file
        .by_ref()
        .take(MAX_DOCX_XML_BYTES + 1)
        .read_to_end(&mut xml_bytes)?;
    if xml_bytes.len() as u64 > MAX_DOCX_XML_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "word/document.xml excede o limite de {MAX_DOCX_XML_BYTES} bytes descompactado"
            ),
        ));
    }
    let xml_content =
        String::from_utf8(xml_bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    extract_text_from_docx_xml(&xml_content)
}

/// Extrai nós de texto `<w:t>` e quebras `<w:p>` do XML interno do DOCX.
pub fn extract_text_from_docx_xml(xml: &str) -> io::Result<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut output = String::new();
    let mut in_text_tag = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"w:t" => {
                    in_text_tag = true;
                }
                b"w:p" if !output.is_empty() && !output.ends_with('\n') => {
                    output.push('\n');
                }
                _ => {}
            },
            Ok(Event::Empty(ref e)) => match e.name().as_ref() {
                b"w:br" | b"w:cr" => {
                    output.push('\n');
                }
                b"w:tab" => {
                    output.push('\t');
                }
                _ => {}
            },
            Ok(Event::End(ref e)) => match e.name().as_ref() {
                b"w:t" => {
                    in_text_tag = false;
                }
                b"w:p" if !output.ends_with('\n') => {
                    output.push('\n');
                }
                _ => {}
            },
            Ok(Event::Text(ref e)) => {
                if in_text_tag {
                    if let Ok(text) = e.unescape() {
                        output.push_str(&text);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("erro ao processar xml do docx: {e}"),
                ));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(output.trim().to_string())
}

/// Extrai o texto legível de um arquivo PDF.
pub fn extract_pdf_text<P: AsRef<Path>>(path: P) -> io::Result<String> {
    let p = path.as_ref();
    if std::fs::metadata(p)?.len() > MAX_PDF_BYTES {
        return Ok(format!(
            "[PDF acima de {} MiB: extração de texto ignorada.]",
            MAX_PDF_BYTES / (1024 * 1024)
        ));
    }
    let bytes = std::fs::read(p)?;
    if bytes.is_empty() {
        return Ok(String::new());
    }

    // `pdf-extract` entra em pânico com PDFs malformados (ex.: referência de objeto ausente): um
    // arquivo hostil não pode derrubar a busca/prévia, então o pânico vira erro de extração.
    let extracted = std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(&bytes))
        .unwrap_or_else(|_| {
            Err(pdf_extract::OutputError::IoError(io::Error::other(
                "estrutura PDF malformada (falha interna do extrator)",
            )))
        });

    match extracted {
        Ok(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                Ok("[Documento PDF sem camada de texto extraível / página escaneada]".to_string())
            } else {
                Ok(trimmed.to_string())
            }
        }
        Err(e) => {
            // Se falhou no extrator de texto (ex.: PDF protegido ou com codificação não-padrão),
            // tentamos extrair streams de texto básicos com o lopdf se disponível, ou emitir mensagem explicativa
            Ok(format!(
                "[Não foi possível extrair o texto deste PDF: {e}.\nO arquivo pode estar protegido por senha ou conter páginas exclusivamente em formato de imagem/scanner.]"
            ))
        }
    }
}

/// Detecta se a extensão corresponde a um documento suportado (docx ou pdf)
/// e extrai o texto do arquivo se aplicável.
pub fn extract_text_from_file<P: AsRef<Path>>(path: P) -> io::Result<Option<String>> {
    let p = path.as_ref();
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext.as_deref() {
        Some("docx") => extract_docx_text(p).map(Some),
        Some("pdf") => extract_pdf_text(p).map(Some),
        _ => Ok(None),
    }
}

/// Verifica se uma dada extensão possui extrator de texto dedicado.
#[must_use]
pub fn has_document_extractor(ext: &str) -> bool {
    let ext_lc = ext.to_lowercase();
    ext_lc == "docx" || ext_lc == "pdf"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_docx_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p>
            <w:r><w:t>FastFile Document Search</w:t></w:r>
        </w:p>
        <w:p>
            <w:r><w:t>Esta linha contém termo_chave para teste.</w:t></w:r>
        </w:p>
    </w:body>
</w:document>"#;

        let extracted = extract_text_from_docx_xml(xml).unwrap();
        assert!(extracted.contains("FastFile Document Search"));
        assert!(extracted.contains("termo_chave"));
        let lines: Vec<&str> = extracted.lines().collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_has_document_extractor() {
        assert!(has_document_extractor("docx"));
        assert!(has_document_extractor("DOCX"));
        assert!(has_document_extractor("pdf"));
        assert!(has_document_extractor("PDF"));
        assert!(!has_document_extractor("rs"));
        assert!(!has_document_extractor("exe"));
    }

    #[test]
    fn test_extract_docx_zip_file() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;

        let tmp_dir = std::env::temp_dir().join(format!("ff_docx_test_{}", std::process::id()));
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let docx_path = tmp_dir.join("sample.docx");

        let file = std::fs::File::create(&docx_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);

        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("word/document.xml", options).unwrap();
        let xml_body = r#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:body>
        <w:p><w:r><w:t>Primeiro parágrafo de teste DOCX</w:t></w:r></w:p>
        <w:p><w:r><w:t>Segundo parágrafo com palavra_secreta_42</w:t></w:r></w:p>
    </w:body>
</w:document>"#;
        zip.write_all(xml_body.as_bytes()).unwrap();
        zip.finish().unwrap();

        let extracted = extract_docx_text(&docx_path).unwrap();
        assert!(extracted.contains("Primeiro parágrafo"));
        assert!(extracted.contains("palavra_secreta_42"));

        let auto_extracted = extract_text_from_file(&docx_path).unwrap();
        assert!(auto_extracted.is_some());
        assert_eq!(auto_extracted.unwrap(), extracted);

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
