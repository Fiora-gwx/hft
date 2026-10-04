// TYP-005.1: no to_f64
use hft_types::*;

fn main() {
    let _ = Px(1).to_f64();
}
