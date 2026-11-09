#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| sofel_read_fuzz::check_font(data));
