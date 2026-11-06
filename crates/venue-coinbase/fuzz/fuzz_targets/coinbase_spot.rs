#![no_main]
#[path = "../../tests/support/mod.rs"]
mod support;
#[path = "../exercise.rs"]
mod exercise;
fn cbx_018_fuzz_coinbase_spot(data: &[u8]) {
    exercise::run(data);
}
libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    cbx_018_fuzz_coinbase_spot(data);
});
