use std::path::Path;

use hft_rt::{PageSize, RingSide, ShmSpsc};

// RT-028.3: 4 KiB element compiles.
fn main() {
    let _ = ShmSpsc::create::<[u64; 512]>(Path::new("/nonexistent-hft-dir/ring"), 2, PageSize::Normal, RingSide::Producer, 1);
}
