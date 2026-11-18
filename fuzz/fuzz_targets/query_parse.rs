#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(pattern) = std::str::from_utf8(data) {
        for (is_regex, is_fuzzy) in [(false, false), (true, false), (false, true)] {
            let _ = ff_search::Pattern::parse_with_options(pattern, is_regex, is_fuzzy);
        }
    }
});
