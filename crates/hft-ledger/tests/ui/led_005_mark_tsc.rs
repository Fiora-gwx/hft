use hft_ledger::{FixedVec, Ledger};
use hft_types::{InstId, Px, Tsc};

fn call(l: &mut Ledger) {
    let mut touched = FixedVec::new();
    l.on_mark(InstId(21), Px(1), Tsc(5), &mut touched);
}

fn main() {}
