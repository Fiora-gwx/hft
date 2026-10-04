// TYP-005.5: no cross-type Sub
use hft_types::*;

fn main() {
    let _ = Qty(1) - Px(1);
}
