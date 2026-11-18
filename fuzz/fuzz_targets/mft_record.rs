#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // O chamador real sempre entrega `record_size` bytes (normalmente 1024); o fuzz também varia o
    // tamanho declarado para exercitar o fixup.
    for record_size in [1024usize, 4096] {
        let mut rec = data.to_vec();
        rec.resize(record_size, 0);
        let _ = ff_index::ntfs::parse_mft_record(&mut rec, record_size);
    }
});
