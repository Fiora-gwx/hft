use std::path::Path;

use hft_rt::{PageSize, RingSide, ShmSpsc};

// RT-028.1: 4 B element.
fn main() {
    let _ = ShmSpsc::create::<[u8; 4]>(Path::new("/nonexistent-hft-dir/ring"), 2, PageSize::Normal, RingSide::Producer, 1);
}
