//! Robustez dos extratores de DOCX/PDF contra arquivos corrompidos ou hostis.
//! Determinístico (xorshift) para rodar no CI normal; exploração longa fica em `fuzz/`.

use std::io::Write;
use std::path::PathBuf;

use ff_content::extractors::{extract_docx_text, extract_pdf_text, extract_text_from_docx_xml};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn mutate(rng: &mut Rng, base: &[u8]) -> Vec<u8> {
    let mut b = base.to_vec();
    match rng.below(4) {
        0 => {
            for _ in 0..1 + rng.below(8) {
                let i = rng.below(b.len());
                b[i] = rng.next() as u8;
            }
        }
        1 => b.truncate(rng.below(b.len())),
        2 => b.extend((0..rng.below(64)).map(|_| rng.next() as u8)),
        _ => {
            let i = rng.below(b.len());
            b[i] ^= 1 << rng.below(8);
        }
    }
    b
}

fn temp_file(tag: &str, bytes: &[u8]) -> PathBuf {
    let p = std::env::temp_dir().join(format!("ff-content-rob-{tag}-{}", std::process::id()));
    std::fs::write(&p, bytes).unwrap();
    p
}

const DOCX_XML: &str = r#"<?xml version="1.0"?><w:document><w:body><w:p><w:r><w:t>Ola mundo</w:t></w:r></w:p><w:p><w:r><w:t>Segunda &amp; linha</w:t><w:tab/><w:br/></w:r></w:p></w:body></w:document>"#;

fn make_docx(xml: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut out));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("word/document.xml", opts).unwrap();
        zip.write_all(xml).unwrap();
        zip.finish().unwrap();
    }
    out
}

fn make_pdf(text: &str) -> Vec<u8> {
    let stream = format!("BT /F1 18 Tf 20 100 Td ({text}) Tj ET");
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in offsets {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxml\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .replace("startxml", "startxref")
        .as_bytes(),
    );
    pdf
}

#[test]
fn docx_xml_roundtrip_and_mutations_never_panic() {
    let text = extract_text_from_docx_xml(DOCX_XML).unwrap();
    assert!(text.contains("Ola mundo") && text.contains("Segunda & linha"));

    let mut rng = Rng(7);
    for _ in 0..3000 {
        let b = mutate(&mut rng, DOCX_XML.as_bytes());
        let s = String::from_utf8_lossy(&b);
        let _ = extract_text_from_docx_xml(&s);
    }
    // Aninhamento profundo e entidades malformadas.
    let deep = "<w:p>".repeat(50_000);
    let _ = extract_text_from_docx_xml(&deep);
    let _ = extract_text_from_docx_xml("<w:t>&#xFFFFFFFF;&bogus;&#;</w:t>");
}

#[test]
fn docx_zip_mutations_never_panic() {
    let base = make_docx(DOCX_XML.as_bytes());
    let ok = temp_file("docx-ok", &base);
    assert!(extract_docx_text(&ok).unwrap().contains("Ola mundo"));

    let mut rng = Rng(99);
    let path = temp_file("docx-mut", &base);
    for _ in 0..1500 {
        std::fs::write(&path, mutate(&mut rng, &base)).unwrap();
        let _ = extract_docx_text(&path);
    }
    let _ = std::fs::remove_file(ok);
    let _ = std::fs::remove_file(path);
}

#[test]
fn docx_decompression_bomb_is_rejected() {
    // `document.xml` maior que o limite, altamente compressível (zip bomb).
    let big = vec![b' '; ff_content::extractors::MAX_DOCX_XML_BYTES as usize + 1];
    let p = temp_file("docx-bomb", &make_docx(&big));
    let err = extract_docx_text(&p).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData, "{err}");
    let _ = std::fs::remove_file(p);
}

#[test]
fn pdf_valid_and_mutations_never_panic() {
    let base = make_pdf("Hello FastFile");
    let ok = temp_file("pdf-ok", &base);
    let text = extract_pdf_text(&ok).unwrap();
    assert!(text.contains("Hello"), "texto extraído: {text}");

    let mut rng = Rng(2024);
    let path = temp_file("pdf-mut", &base);
    for _ in 0..600 {
        std::fs::write(&path, mutate(&mut rng, &base)).unwrap();
        let _ = extract_pdf_text(&path);
    }
    for _ in 0..300 {
        let n = rng.below(512);
        let mut junk: Vec<u8> = b"%PDF-1.7\n".to_vec();
        junk.extend((0..n).map(|_| rng.next() as u8));
        std::fs::write(&path, junk).unwrap();
        let _ = extract_pdf_text(&path);
    }
    let _ = std::fs::remove_file(ok);
    let _ = std::fs::remove_file(path);
}
