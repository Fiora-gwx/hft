#![no_main]
#[path = "../../tests/support/mod.rs"]
mod support;
#[path = "../exercise.rs"]
mod exercise;
fn bin_401_fuzz_binance_spot(data: &[u8]) { exercise::run::<venue_binance::Spot>(data, support::S_BTC, venue_binance::FeedCtx::SPOT, venue_binance::GwCtx::default()); }
libfuzzer_sys::fuzz_target!(|data: &[u8]| { bin_401_fuzz_binance_spot(data); });
