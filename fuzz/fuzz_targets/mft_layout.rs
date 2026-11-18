#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut rec = data.to_vec();
    rec.resize(1024, 0);
    if let Some(layout) = ff_index::ntfs::parse_mft_layout(&mut rec.clone(), 1024, 4096) {
        // O plano de leitura também precisa ser seguro para qualquer layout aceito.
        let _ = ff_index::ntfs::plan_mft_reads(&layout, 1024, 4096, 1024);
    }
    // Seguindo o $ATTRIBUTE_LIST: os bytes restantes fazem as vezes de volume.
    let volume = data.get(1024..).unwrap_or(&[]);
    let _ = ff_index::ntfs::resolve_mft_layout(&mut rec, 1024, 4096, |off, buf| {
        let o = (off as usize).min(volume.len());
        let n = buf.len().min(volume.len() - o);
        buf[..n].copy_from_slice(&volume[o..o + n]);
        Ok(n)
    });
});
