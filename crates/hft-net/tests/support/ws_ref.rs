//! Byte-by-byte reference WebSocket framing (RFC 6455 §5.2, §5.3) for the tests: server frames,
//! client-frame masking (key bytes = `mask_key.to_be_bytes()`) and client-frame parsing.

/// An unmasked server frame.
pub fn server_frame(fin: bool, opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(payload.len() + 10);
    v.push(if fin { 0x80 } else { 0 } | opcode);
    push_len(&mut v, payload.len(), false);
    v.extend_from_slice(payload);
    v
}

/// A Text frame with FIN = 1.
pub fn text(payload: &[u8]) -> Vec<u8> {
    server_frame(true, 0x1, payload)
}

fn push_len(v: &mut Vec<u8>, n: usize, mask: bool) {
    let m = if mask { 0x80 } else { 0 };
    if n < 126 {
        v.push(m | n as u8);
    } else if n <= 0xFFFF {
        v.push(m | 126);
        v.extend_from_slice(&(n as u16).to_be_bytes());
    } else {
        v.push(m | 127);
        v.extend_from_slice(&(n as u64).to_be_bytes());
    }
}

/// Reference masking: byte i XOR key[i % 4].
pub fn mask(payload: &[u8], key: u32) -> Vec<u8> {
    let k = key.to_be_bytes();
    payload.iter().enumerate().map(|(i, b)| b ^ k[i % 4]).collect()
}

/// Reference client frame (FIN = 1, MASK = 1).
pub fn client_frame(opcode: u8, payload: &[u8], key: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity(payload.len() + 14);
    v.push(0x80 | opcode);
    push_len(&mut v, payload.len(), true);
    v.extend_from_slice(&key.to_be_bytes());
    v.extend_from_slice(&mask(payload, key));
    v
}

/// One parsed client frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientFrame {
    pub fin: bool,
    pub opcode: u8,
    pub masked: bool,
    pub key: u32,
    /// Unmasked payload.
    pub payload: Vec<u8>,
}

/// Parses complete client frames from the front of `b`; returns them and the bytes consumed.
pub fn parse_client(b: &[u8]) -> (Vec<ClientFrame>, usize) {
    let mut out = Vec::new();
    let mut off = 0;
    loop {
        let r = &b[off..];
        if r.len() < 2 {
            break;
        }
        let fin = r[0] & 0x80 != 0;
        let opcode = r[0] & 0x0F;
        let masked = r[1] & 0x80 != 0;
        let l7 = (r[1] & 0x7F) as usize;
        let (len, mut h) = match l7 {
            126 => {
                if r.len() < 4 {
                    break;
                }
                (u16::from_be_bytes([r[2], r[3]]) as usize, 4)
            }
            127 => {
                if r.len() < 10 {
                    break;
                }
                (u64::from_be_bytes(r[2..10].try_into().unwrap()) as usize, 10)
            }
            n => (n, 2),
        };
        let mut key = 0u32;
        if masked {
            if r.len() < h + 4 {
                break;
            }
            key = u32::from_be_bytes(r[h..h + 4].try_into().unwrap());
            h += 4;
        }
        if r.len() < h + len {
            break;
        }
        let p = &r[h..h + len];
        let payload = if masked { mask(p, key) } else { p.to_vec() };
        out.push(ClientFrame {
            fin,
            opcode,
            masked,
            key,
            payload,
        });
        off += h + len;
    }
    (out, off)
}
