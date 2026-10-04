use std::path::Path;

use hft_rt::{PageSize, RingSide, ShmSpsc};

// RT-028.4: 4 KiB + 8 B element.
fn main() {
    let _ = ShmSpsc::create::<[u64; 513]>(Path::new("/nonexistent-hft-dir/ring"), 2, PageSize::Normal, RingSide::Producer, 1);
}
