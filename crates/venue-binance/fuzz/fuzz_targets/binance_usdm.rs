#![no_main]
#[path = "../../tests/support/mod.rs"]
mod support;
#[path = "../exercise.rs"]
mod exercise;
fn bin_400_fuzz_binance_usdm(data: &[u8]) { exercise::run::<venue_binance::Usdm>(data, support::U_BTC, venue_binance::FeedCtx::USDM, venue_binance::GwCtx::default()); }
libfuzzer_sys::fuzz_target!(|data: &[u8]| { bin_400_fuzz_binance_usdm(data); });
