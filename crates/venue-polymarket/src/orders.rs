//! Order entry over REST (README "下单"): `POST /order` with a Deposit Wallet (signature type 3)
//! order, post-only and GTD; cancels by order hash or by token; the order query; the L2 request
//! headers. Credentials are borrowed per request (contract 7): `Templates` holds addresses only.

use hft_event::{GwCmd, GwOp, OrderFlags};
use hft_refdata::RefData;
use hft_types::{AccountId, Side, Tif};
use hft_venue_proto::{Credential, EncodeErr, Method, ProtoErr, RestReqBuf, SignAlg, TargetRef, WallClock};

use crate::reports::GwCtx;
use crate::sign::{self, CHAIN_ID, EXCHANGE_V2, OrderV2, SignErr, Wallet};

/// Non-sensitive order configuration of one account.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Templates {
    pub account: AccountId,
    /// The API key's wallet (EOA): `POLY_ADDRESS` and the order signer's key.
    pub eoa: [u8; 20],
    /// The Deposit Wallet: order `maker` and `signer`.
    pub wallet: [u8; 20],
    /// Exchange EIP-712 domain separator (CTF Exchange V2, Polygon).
    pub domain: [u8; 32],
    /// GTD lifetime of an order (s); the exchange requires more than 60 s.
    pub gtd_s: u32,
}

/// GTD lifetime: one 5-minute window; the strategy cancels earlier, the exchange expires a
/// leftover order of a dead process.
pub const GTD_S: u32 = 300;

/// The three parts of `Credential.secret` (README "凭证"): `<api secret>|<wallet key hex>|<deposit
/// wallet address>`.
pub(crate) struct Keys<'a> {
    pub api_secret: &'a str,
    pub wallet: Wallet,
    pub deposit: [u8; 20],
}

pub(crate) fn keys(cred: &Credential) -> Result<Keys<'_>, ProtoErr> {
    if !matches!(cred.alg, SignAlg::Secp256k1 | SignAlg::HmacSha256) {
        return Err(ProtoErr::Cfg);
    }
    let s = core::str::from_utf8(cred.secret.0.as_slice()).map_err(|_| ProtoErr::Cfg)?;
    let mut it = s.split('|');
    let (Some(api_secret), Some(key), Some(deposit), None) = (it.next(), it.next(), it.next(), it.next())
    else {
        return Err(ProtoErr::Cfg);
    };
    let wallet = Wallet::from_hex(key).map_err(|_| ProtoErr::Cfg)?;
    let deposit = parse_addr(deposit).ok_or(ProtoErr::Cfg)?;
    if api_secret.is_empty() || cred.api_key.is_empty() || cred.passphrase.is_empty() {
        return Err(ProtoErr::Cfg);
    }
    Ok(Keys {
        api_secret,
        wallet,
        deposit,
    })
}

/// `0x` + 40 hex digits.
pub(crate) fn parse_addr(s: &str) -> Option<[u8; 20]> {
    let h = s.trim().strip_prefix("0x")?.as_bytes();
    if h.len() != 40 {
        return None;
    }
    let mut out = [0u8; 20];
    for (i, o) in out.iter_mut().enumerate() {
        let hi = (h[2 * i] as char).to_digit(16)?;
        let lo = (h[2 * i + 1] as char).to_digit(16)?;
        *o = (hi << 4 | lo) as u8;
    }
    Some(out)
}

pub(crate) fn build_templates(acct: AccountId, rest: &Credential) -> Result<Templates, ProtoErr> {
    let k = keys(rest)?;
    Ok(Templates {
        account: acct,
        eoa: k.wallet.address(),
        wallet: k.deposit,
        domain: sign::exchange_domain("2", CHAIN_ID, &EXCHANGE_V2),
        gtd_s: GTD_S,
    })
}

/// `POLY_*` L2 headers over `out`'s method, path (no query) and body, signed at `clk` (unix s);
/// every line ends with CRLF. `content_type`: add `Content-Type: application/json` to a request
/// with a body (the gw's REST lanes do not add it; the account client does).
pub(crate) fn sign_headers(
    eoa: &[u8; 20],
    api_secret: &str,
    cred: &Credential,
    clk: &WallClock,
    content_type: bool,
    out: &mut RestReqBuf,
) -> Result<(), ProtoErr> {
    let mut ts = [0u8; 20];
    let ts = dec(clk.unix_ms.div_euclid(1000).max(0) as u64, &mut ts);
    let method = match out.method {
        Method::Get => "GET",
        Method::Post => "POST",
        Method::Put => "PUT",
        Method::Delete => "DELETE",
    };
    let pq = core::str::from_utf8(out.path_query.as_slice()).map_err(|_| ProtoErr::Cfg)?;
    let path = pq.split('?').next().unwrap_or("");
    let body = core::str::from_utf8(out.body.as_slice()).map_err(|_| ProtoErr::Cfg)?;
    let mut mac = [0u8; 44];
    let sig = sign::l2_hmac(api_secret, ts, method, path, body, &mut mac).map_err(|e| match e {
        SignErr::Short => ProtoErr::BufFull,
        _ => ProtoErr::Cfg,
    })?;
    let mut addr = [0u8; 42];
    let addr = sign::checksum_address(eoa, &mut addr);
    let h = &mut out.headers;
    h.clear();
    h.push(b"POLY_ADDRESS: ")?;
    h.push(addr.as_bytes())?;
    h.push(b"\r\nPOLY_SIGNATURE: ")?;
    h.push(sig.as_bytes())?;
    h.push(b"\r\nPOLY_TIMESTAMP: ")?;
    h.push(ts.as_bytes())?;
    h.push(b"\r\nPOLY_API_KEY: ")?;
    h.push(cred.api_key.as_slice())?;
    h.push(b"\r\nPOLY_PASSPHRASE: ")?;
    h.push(cred.passphrase.as_slice())?;
    h.push(b"\r\n")?;
    if content_type && !out.body.is_empty() {
        h.push(b"Content-Type: application/json\r\n")?;
    }
    out.redacted = true;
    Ok(())
}

fn of(e: ProtoErr) -> EncodeErr {
    match e {
        ProtoErr::BufFull => EncodeErr::FieldOverflow,
        _ => EncodeErr::Unsupported(GwOp::Place),
    }
}

/// `raw · 10^e` as an exact integer (`None` when inexact, negative or out of range).
fn scaled(raw: i128, e: i32) -> Option<u64> {
    let p = 10i128.checked_pow(e.unsigned_abs())?;
    let v = if e >= 0 {
        raw.checked_mul(p)?
    } else {
        if raw % p != 0 {
            return None;
        }
        raw / p
    };
    u64::try_from(v).ok().filter(|v| *v > 0)
}

/// Every order operation of the gw (contract 29); all are REST.
#[allow(clippy::too_many_arguments)]
pub(crate) fn encode(
    rd: &RefData,
    t: &Templates,
    ctx: &GwCtx,
    cred: &Credential,
    c: &GwCmd,
    tgt: &TargetRef,
    clk: &WallClock,
    out: &mut RestReqBuf,
) -> Result<(), EncodeErr> {
    *out = RestReqBuf::default();
    let k = keys(cred).map_err(|_| EncodeErr::Unsupported(c.op))?;
    if k.wallet.address() != t.eoa || k.deposit != t.wallet {
        return Err(EncodeErr::Unsupported(c.op));
    }
    match c.op {
        GwOp::Place => place(rd, t, ctx, &k, cred, c, clk, out)?,
        GwOp::Cancel => {
            out.method = Method::Delete;
            match ctx.order_hash(tgt.exch_order_id) {
                Some(h) => {
                    let mut hex = [0u8; 66];
                    let h = sign::hex_bytes(&h, &mut hex).ok_or(EncodeErr::FieldOverflow)?;
                    out.path_query.push(b"/order").map_err(of)?;
                    out.body.push(b"{\"orderID\":\"").map_err(of)?;
                    out.body.push(h.as_bytes()).map_err(of)?;
                    out.body.push(b"\"}").map_err(of)?;
                }
                // Not acked yet (or before a restart): every order of the account on the token.
                None => cancel_token(ctx, c, out)?,
            }
        }
        GwOp::CancelAll => {
            out.method = Method::Delete;
            if c.inst.0 == 0 {
                out.path_query.push(b"/cancel-all").map_err(of)?;
            } else {
                cancel_token(ctx, c, out)?;
            }
        }
        GwOp::Query => {
            let h = ctx
                .order_hash(tgt.exch_order_id)
                .ok_or(EncodeErr::UnknownTarget)?;
            let mut hex = [0u8; 66];
            let h = sign::hex_bytes(&h, &mut hex).ok_or(EncodeErr::FieldOverflow)?;
            out.path_query.push(b"/data/order/").map_err(of)?;
            out.path_query.push(h.as_bytes()).map_err(of)?;
        }
        _ => return Err(EncodeErr::Unsupported(c.op)),
    }
    sign_headers(&t.eoa, k.api_secret, cred, clk, true, out).map_err(of)
}

/// `DELETE /cancel-market-orders {"asset_id": <token of c.inst>}`.
fn cancel_token(ctx: &GwCtx, c: &GwCmd, out: &mut RestReqBuf) -> Result<(), EncodeErr> {
    let token = ctx.token_of(c.inst).ok_or(EncodeErr::UnknownInst)?;
    out.path_query.push(b"/cancel-market-orders").map_err(of)?;
    out.body.push(b"{\"asset_id\":\"").map_err(of)?;
    out.body.push(token).map_err(of)?;
    out.body.push(b"\"}").map_err(of)
}

#[allow(clippy::too_many_arguments)]
fn place(
    rd: &RefData,
    t: &Templates,
    ctx: &GwCtx,
    k: &Keys<'_>,
    cred: &Credential,
    c: &GwCmd,
    clk: &WallClock,
    out: &mut RestReqBuf,
) -> Result<(), EncodeErr> {
    let row = rd.inst(c.inst);
    if c.inst.0 == 0 || row.venue != crate::VENUE {
        return Err(EncodeErr::UnknownInst);
    }
    let flags = OrderFlags(c.oflags);
    if flags.contains(OrderFlags::REDUCE_ONLY) || flags.contains(OrderFlags::POSITION_CLOSE) {
        return Err(EncodeErr::Unsupported(c.op));
    }
    // Resting orders only: post-only (rejected if it would match) or a plain GTD bid/ask.
    let post_only = match c.tif {
        Tif::PostOnly => true,
        Tif::Gtc => false,
        Tif::Ioc | Tif::Fok => return Err(EncodeErr::Unsupported(c.op)),
    };
    let token = ctx.token_of(c.inst).ok_or(EncodeErr::UnknownInst)?;
    let token_s = core::str::from_utf8(token).map_err(|_| EncodeErr::UnknownInst)?;
    let token_id = sign::parse_u256(token_s).ok_or(EncodeErr::UnknownInst)?;
    // Amounts in 1e-6: shares = qty·10^(qty_exp+6); USDC = px·qty·10^(px_exp+qty_exp+6).
    let qe = i32::from(row.qty_exp) + 6;
    let ue = i32::from(row.px_exp) + i32::from(row.qty_exp) + 6;
    let shares = scaled(i128::from(c.qty.0), qe).ok_or(EncodeErr::FieldOverflow)?;
    let usdc = scaled(i128::from(c.px.0) * i128::from(c.qty.0), ue).ok_or(EncodeErr::FieldOverflow)?;
    let (side, maker_amount, taker_amount, side_s): (u8, u64, u64, &[u8]) = match c.side {
        Side::Buy => (0, usdc, shares, b"BUY"),
        Side::Sell => (1, shares, usdc, b"SELL"),
    };
    let now_ms = u64::try_from(clk.unix_ms).map_err(|_| EncodeErr::FieldOverflow)?;
    // Unique per order and below 2^53 (the body carries it as a JSON number).
    let salt = (now_ms & ((1 << 33) - 1)) << 20 | u64::from(c.slot.0 & 0xF_FFFF);
    let o = OrderV2 {
        salt,
        maker: t.wallet,
        signer: t.wallet,
        token_id,
        maker_amount,
        taker_amount,
        side,
        signature_type: 3,
        timestamp_ms: now_ms,
        metadata: [0; 32],
        builder: [0; 32],
    };
    let mut sig = [0u8; 512];
    let n = sign::sign_order_1271(&k.wallet, &t.domain, CHAIN_ID, &o, &mut sig);
    let mut sig_hex = [0u8; 1026];
    let sig_hex =
        sign::hex_bytes(sig.get(..n).unwrap_or(&[]), &mut sig_hex).ok_or(EncodeErr::FieldOverflow)?;
    let mut maker = [0u8; 42];
    let maker = sign::checksum_address(&t.wallet, &mut maker);
    let expiration = now_ms / 1000 + u64::from(t.gtd_s);
    out.method = Method::Post;
    out.path_query.push(b"/order").map_err(of)?;
    let (mut n1, mut n2, mut n3, mut n4, mut n5) = ([0u8; 20], [0u8; 20], [0u8; 20], [0u8; 20], [0u8; 20]);
    let parts: [&[u8]; 27] = [
        b"{\"order\":{\"salt\":",
        dec(salt, &mut n1).as_bytes(),
        b",\"maker\":\"",
        maker.as_bytes(),
        b"\",\"signer\":\"",
        maker.as_bytes(),
        b"\",\"tokenId\":\"",
        token,
        b"\",\"makerAmount\":\"",
        dec(maker_amount, &mut n2).as_bytes(),
        b"\",\"takerAmount\":\"",
        dec(taker_amount, &mut n3).as_bytes(),
        b"\",\"side\":\"",
        side_s,
        b"\",\"expiration\":\"",
        dec(expiration, &mut n4).as_bytes(),
        b"\",\"signatureType\":3,\"timestamp\":\"",
        dec(now_ms, &mut n5).as_bytes(),
        b"\",\"metadata\":\"0x0000000000000000000000000000000000000000000000000000000000000000\"",
        b",\"builder\":\"0x0000000000000000000000000000000000000000000000000000000000000000\"",
        b",\"signature\":\"",
        sig_hex.as_bytes(),
        b"\"},\"owner\":\"",
        cred.api_key.as_slice(),
        b"\",\"orderType\":\"GTD\",\"deferExec\":false,\"postOnly\":",
        if post_only { b"true" } else { b"false" },
        b"}",
    ];
    for p in parts {
        out.body.push(p).map_err(of)?;
    }
    Ok(())
}

/// Decimal text of `v` in `buf`.
pub(crate) fn dec(mut v: u64, buf: &mut [u8; 20]) -> &str {
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    core::str::from_utf8(&buf[i..]).unwrap_or("0")
}
