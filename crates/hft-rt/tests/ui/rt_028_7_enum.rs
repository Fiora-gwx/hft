use std::path::Path;

use hft_rt::{PageSize, RingSide, ShmSpsc};

// RT-028.7: an enum carrying data is not FromBytes.
#[derive(Copy, Clone)]
#[repr(u8)]
enum E {
    A(u64),
    B,
}
fn main() {
    let _ = ShmSpsc::create::<E>(Path::new("/nonexistent-hft-dir/ring"), 2, PageSize::Normal, RingSide::Producer, 1);
}
