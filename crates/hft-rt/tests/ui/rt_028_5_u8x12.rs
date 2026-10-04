use std::path::Path;

use hft_rt::{PageSize, RingSide, ShmSpsc};

// RT-028.5: not a multiple of 8.
fn main() {
    let _ = ShmSpsc::create::<[u8; 12]>(Path::new("/nonexistent-hft-dir/ring"), 2, PageSize::Normal, RingSide::Producer, 1);
}
