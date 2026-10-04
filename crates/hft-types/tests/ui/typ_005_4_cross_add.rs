// TYP-005.4: no cross-type Add
use hft_types::*;

fn main() {
    let _ = Px(1) + Qty(1);
}
