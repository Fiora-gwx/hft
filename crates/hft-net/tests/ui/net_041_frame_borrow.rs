// NET-041: a frame borrows the connection until it is dropped.
fn f(c: &mut hft_net::Conn) {
    let a = c.next_frame().unwrap();
    let b = c.next_frame();
    std::hint::black_box(a.payload);
    drop(b);
}

fn main() {}
