// TYP-005.8: wire enums derive TryFromBytes, never FromBytes
use hft_types::*;

fn f<T: zerocopy::FromBytes>() {}

fn main() {
    f::<Side>();
}
