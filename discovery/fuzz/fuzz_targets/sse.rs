#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| peritus_fuzz::sse(data));
