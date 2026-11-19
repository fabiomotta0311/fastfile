#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(index) = ff_index::FileIndex::load_snapshot(&mut &data[..]) {
        for id in 0..index.len().min(10_000) as u32 {
            let _ = index.name(id);
            let _ = index.name_lc(id);
            let _ = index.build_path(id);
        }
    }
});
