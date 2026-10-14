use hft_net::rest::{Error, decode};

#[test]
fn net_261_warm_chunked_fragmentation() {
    let wire = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n4\r\ndefg\r\n0\r\n\r\n";
    for n in 0..wire.len() {
        assert!(decode(&wire[..n], 7).unwrap().is_none(), "prefix {n}");
    }
    assert_eq!(decode(wire, 7).unwrap().unwrap().body, b"abcdefg");
    assert_eq!(decode(wire, 6).unwrap_err(), Error::TooLarge);
}

#[test]
fn net_262_warm_rejects_ambiguous_framing() {
    for wire in [
        &b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nTransfer-Encoding: chunked\r\n\r\n"[..],
        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nContent-Length: 0\r\n\r\n",
        b"HTTP/1.1 200 OK\r\n\r\n",
    ] {
        assert_eq!(decode(wire, 128).unwrap_err(), Error::Framing);
    }
    assert_eq!(
        decode(
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\n",
            usize::MAX
        )
        .unwrap_err(),
        Error::TooLarge
    );
    assert!(
        decode(b"HTTP/1.1 204 No Content\r\n\r\n", 0)
            .unwrap()
            .unwrap()
            .body
            .is_empty()
    );
}
