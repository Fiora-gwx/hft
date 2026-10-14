#![no_main]
#[path = "../../tests/support/mod.rs"]
mod support;
#[path = "../exercise.rs"]
mod exercise;
fn bgt_400_fuzz_fut(data: &[u8]) { exercise::run::<venue_bitget::BitgetUsdtFutures>(data, support::F_BTC, venue_bitget::FeedCtx::new(), venue_bitget::GwCtx::new()); }
libfuzzer_sys::fuzz_target!(|data: &[u8]| { bgt_400_fuzz_fut(data); });
