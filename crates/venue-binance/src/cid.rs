//! Client ids (`newClientOrderId`) and WS API request ids (README "下单:WS API").

use hft_event::GwOp;
use hft_types::ClientOrderId;
use hft_venue_proto::ReqRef;

/// Canonical 24-hex text; Binance adds no prefix (README "使用本组件").
pub(crate) fn render_cid(id: &ClientOrderId, out: &mut [u8; 48]) -> usize {
    let mut t = [0u8; 24];
    id.encode(&mut t);
    if let Some(d) = out.get_mut(..24) {
        d.copy_from_slice(&t);
    }
    24
}

/// Ours only: exactly the 24-character canonical form that decodes.
pub(crate) fn parse_cid(b: &[u8]) -> Option<ClientOrderId> {
    if b.len() != 24 {
        return None;
    }
    ClientOrderId::decode(b).ok()
}

/// Op character of a request id; `l` (logon) and `u` (user-data stream) are control requests.
fn op_char(op: GwOp) -> Option<u8> {
    match op {
        GwOp::Place => Some(b'p'),
        GwOp::Cancel => Some(b'c'),
        GwOp::Amend => Some(b'm'),
        GwOp::CancelAll => Some(b'x'),
        GwOp::Query => Some(b'q'),
        _ => None,
    }
}

/// `"<op><slot 8 hex><gen 8 hex>"` (17 bytes); `None` for an op without a request id.
pub fn encode_req_id(op: GwOp, slot: u32, r#gen: u32) -> Option<[u8; 17]> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut b = [0u8; 17];
    b[0] = op_char(op)?;
    for (k, v) in [slot, r#gen].into_iter().enumerate() {
        for j in 0..8usize {
            let shift = 28u32.wrapping_sub((j as u32).wrapping_mul(4));
            if let Some(c) = b.get_mut(1 + k * 8 + j) {
                *c = HEX[((v >> shift) & 0xF) as usize];
            }
        }
    }
    Some(b)
}

/// What a request id is.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum IdKind {
    /// An order-class request (`p`, `c`, `m`, `x`, `q`).
    Order(ReqRef),
    /// `l` (`session.logon`) or `u` (`userDataStream.*`): control, never restored.
    Control,
    /// Not one of ours.
    Foreign,
}

fn hex_val(c: u8) -> Option<u32> {
    match c {
        b'0'..=b'9' => Some(u32::from(c.wrapping_sub(b'0'))),
        b'a'..=b'f' => Some(u32::from(c.wrapping_sub(b'a')).wrapping_add(10)),
        _ => None,
    }
}

fn hex8(s: &[u8]) -> Option<u32> {
    if s.len() != 8 {
        return None;
    }
    s.iter().try_fold(0u32, |a, &c| Some((a << 4) | hex_val(c)?))
}

pub(crate) fn id_kind(id: &[u8]) -> IdKind {
    if id.len() != 17 {
        return IdKind::Foreign;
    }
    let (Some(slot), Some(r#gen)) = (hex8(&id[1..9]), hex8(&id[9..17])) else {
        return IdKind::Foreign;
    };
    let op = match id[0] {
        b'p' => GwOp::Place,
        b'c' => GwOp::Cancel,
        b'm' => GwOp::Amend,
        b'x' => GwOp::CancelAll,
        b'q' => GwOp::Query,
        b'l' | b'u' => return IdKind::Control,
        _ => return IdKind::Foreign,
    };
    IdKind::Order(ReqRef { slot, r#gen, op })
}

/// Restores `(slot, gen, op)` of an order-class request id; anything else → `None`.
pub fn decode_req_id(id: &[u8]) -> Option<ReqRef> {
    match id_kind(id) {
        IdKind::Order(r) => Some(r),
        _ => None,
    }
}
