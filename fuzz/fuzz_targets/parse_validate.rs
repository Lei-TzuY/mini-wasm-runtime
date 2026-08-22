#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(module) = wasm_parser::parse_module(data) {
        let _ = wasm_validator::validate(&module);
    }
});
