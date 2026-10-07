// NET-009 step 1: CAP = 65 536 does not fit the u16 length.
fn main() {
    let f = hft_net::MaskedFrame::<65_536>::new();
    drop(f);
}
