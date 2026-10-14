#![no_main]
#[path = "../../tests/support/mod.rs"]
mod support;
#[path = "../exercise.rs"]
mod exercise;
fn bgt_401_fuzz_spot(data: &[u8]) { exercise::run::<venue_bitget::BitgetSpot>(data, support::S_BTC, venue_bitget::FeedCtx::new(), venue_bitget::GwCtx::new()); }
libfuzzer_sys::fuzz_target!(|data: &[u8]| { bgt_401_fuzz_spot(data); });
