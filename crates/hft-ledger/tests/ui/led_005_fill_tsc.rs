use hft_ledger::{FillDelta, Ledger};
use hft_types::Tsc;

fn call(l: &mut Ledger, f: &FillDelta) {
    let _ = l.apply_fill(f, Tsc(5));
}

fn main() {}
