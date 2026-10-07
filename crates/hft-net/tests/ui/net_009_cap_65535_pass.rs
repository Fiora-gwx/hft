// NET-009 step 2: CAP = 65 535 compiles.
fn main() {
    let f = Box::new(hft_net::MaskedFrame::<65_535>::new());
    drop(f);
}
