use std::path::Path;

use hft_rt::{PageSize, RingSide, ShmSpsc};

// RT-028.2: 8 B element compiles.
fn main() {
    let _ = ShmSpsc::create::<[u64; 1]>(Path::new("/nonexistent-hft-dir/ring"), 2, PageSize::Normal, RingSide::Producer, 1);
}
