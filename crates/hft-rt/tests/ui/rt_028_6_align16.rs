use std::path::Path;

use hft_rt::{PageSize, RingSide, ShmSpsc};

// RT-028.6: 16 B alignment.
#[derive(Copy, Clone, zerocopy::FromBytes, zerocopy::IntoBytes, zerocopy::KnownLayout, zerocopy::Immutable)]
#[repr(C, align(16))]
struct A16([u64; 2]);
fn main() {
    let _ = ShmSpsc::create::<A16>(Path::new("/nonexistent-hft-dir/ring"), 2, PageSize::Normal, RingSide::Producer, 1);
}
