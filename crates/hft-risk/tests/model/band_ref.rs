//! Reference of the risk-reducing cross cap of check #5 (README "检查链" #5), RSK-030.

/// Whether a risk-reducing order at `px` (raw) crosses `mid` (raw, exact) by at most `rr`
/// (Rate raw, 1e-9) in the adverse direction: buy `(px − mid) / mid`, sell `(mid − px) / mid`.
pub fn rr_ok(buy: bool, px: i64, mid: i64, rr: i64) -> bool {
    let adverse = if buy {
        i128::from(px) - i128::from(mid)
    } else {
        i128::from(mid) - i128::from(px)
    };
    adverse * 1_000_000_000 <= i128::from(rr) * i128::from(mid)
}
