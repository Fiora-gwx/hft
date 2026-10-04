// TYP-005.2: no From<Px> for f64
use hft_types::*;

fn main() {
    let _x: f64 = Px(1).into();
}
