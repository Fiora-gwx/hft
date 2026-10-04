// TYP-005.9: control row compiles and holds
use hft_types::*;

fn main() {
    assert_eq!(Px(1) + Px(2), Px(3));
}
