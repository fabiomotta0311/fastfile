#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let path = std::env::temp_dir().join(format!("ff-fuzz-docx-{}.docx", std::process::id()));
    if std::fs::write(&path, data).is_ok() {
        let _ = ff_content::extractors::extract_docx_text(&path);
    }
});
