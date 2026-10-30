//! Order entry over REST (README "下单"): `POST /order` with a Deposit Wallet (signature type 3)
//! order, post-only or plain GTD, or a FAK / FOK taker order (README "吃单"); cancels by order hash
//! or by token; the order query; the L2 request headers. Credentials are borrowed per request
//! (contract 7): `Templates` holds addresses only.

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
    let k = l2_keys(cred)?;
    Ok(Keys {
        api_secret: k.api_secret,
        wallet: k.wallet()?,
        deposit: k.deposit,
    })
}

/// The parts of `Credential.secret` a request needs without the wallet key's derivation (the
/// public key costs a curve multiplication): the API secret and the Deposit Wallet; the key text
/// stays borrowed until an order is signed.
pub(crate) struct L2Keys<'a> {
    pub api_secret: &'a str,
    pub deposit: [u8; 20],
    key: &'a str,
}

impl L2Keys<'_> {
    /// The wallet (EOA) key: derives its address.
    pub(crate) fn wallet(&self) -> Result<Wallet, ProtoErr> {
        Wallet::from_hex(self.key).map_err(|_| ProtoErr::Cfg)
    }
}

pub(crate) fn l2_keys(cred: &Credential) -> Result<L2Keys<'_>, ProtoErr> {
    if !matches!(cred.alg, SignAlg::Secp256k1 | SignAlg::HmacSha256) {
        return Err(ProtoErr::Cfg);
    }
    let s = core::str::from_utf8(cred.secret.0.as_slice()).map_err(|_| ProtoErr::Cfg)?;
    let mut it = s.split('|');
    let (Some(api_secret), Some(key), Some(deposit), None) = (it.next(), it.next(), it.next(), it.next())
    else {
        return Err(ProtoErr::Cfg);
    };
    let deposit = parse_addr(deposit).ok_or(ProtoErr::Cfg)?;
    if api_secret.is_empty() || key.trim().is_empty() || cred.api_key.is_empty() || cred.passphrase.is_empty()
    {
        return Err(ProtoErr::Cfg);
    }
    Ok(L2Keys {
        api_secret,
        deposit,
        key,
    })
}

/// The wallet key of `k` signs for this account (`Templates.eoa`), deriving it once.
pub(crate) fn signer(k: &L2Keys<'_>, t: &Templates) -> Option<Wallet> {
    k.wallet().ok().filter(|w| w.address() == t.eoa)
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

/// Every order operation of the gw (contract 29); all are REST. A `Place` remembers the hash of
/// the order it signs under its client id `tgt.id` (README "下单").
#[allow(clippy::too_many_arguments)]
pub(crate) fn encode(
    rd: &RefData,
    t: &Templates,
    ctx: &mut GwCtx,
    cred: &Credential,
    c: &GwCmd,
    tgt: &TargetRef,
    clk: &WallClock,
    out: &mut RestReqBuf,
) -> Result<(), EncodeErr> {
    *out = RestReqBuf::default();
    // The wallet key is derived only to sign an order (a curve multiplication, README "预签名").
    let k = l2_keys(cred).map_err(|_| EncodeErr::Unsupported(c.op))?;
    if k.deposit != t.wallet {
        return Err(EncodeErr::Unsupported(c.op));
    }
    match c.op {
        GwOp::Place => {
            let (hash, taker) = place(rd, t, ctx, &k, cred, c, clk, out)?;
            ctx.placed(&hash, tgt.id);
            if let Some(tk) = taker {
                ctx.taker_placed(&hash, tk);
            }
        }
        GwOp::Cancel => {
            out.method = Method::Delete;
            // By hash also before the place answer: the hash is known from signing (README "下单").
            match ctx.cancel_hash(tgt.exch_order_id, &tgt.id) {
                Some(h) => {
                    let mut hex = [0u8; 66];
                    let h = sign::hex_bytes(&h, &mut hex).ok_or(EncodeErr::FieldOverflow)?;
                    out.path_query.push(b"/order").map_err(of)?;
                    out.body.push(b"{\"orderID\":\"").map_err(of)?;
                    out.body.push(h.as_bytes()).map_err(of)?;
                    out.body.push(b"\"}").map_err(of)?;
                }
                // An order of an earlier process: every order of the account on the token.
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
            // Not acked yet (no REST answer): the hash signed for the target's client id.
            let h = ctx
                .order_hash(tgt.exch_order_id)
                .or_else(|| ctx.placed_hash(&tgt.id))
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

/// Side code and amounts (1e-6) of an order of `qty` at `px` on `inst`: a buy pays USDC
/// (`makerAmount` = px·qty) for shares (`takerAmount` = qty), a sell the reverse. `None` when
/// not a whole number of 1e-6 units.
pub(crate) fn amounts(
    rd: &RefData,
    inst: hft_types::InstId,
    side: Side,
    px: i64,
    qty: i64,
) -> Option<(u8, u64, u64)> {
    let row = rd.inst(inst);
    // shares = qty·10^(qty_exp+6); USDC = px·qty·10^(px_exp+qty_exp+6).
    let qe = i32::from(row.qty_exp) + 6;
    let ue = i32::from(row.px_exp) + i32::from(row.qty_exp) + 6;
    let shares = scaled(i128::from(qty), qe)?;
    let usdc = scaled(i128::from(px) * i128::from(qty), ue)?;
    Some(match side {
        Side::Buy => (0, usdc, shares),
        Side::Sell => (1, shares, usdc),
    })
}

/// What the gw remembers of a FAK / FOK order it signed (README "吃单"): the order quantity
/// (inst qty units), the signed maker amount (1e-6: USDC of a buy, shares of a sell) and the side.
/// Every taker buy is a spend order (`OrderFlags::SPEND`): it ends when its maker amount is used.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub(crate) struct TakerSpec {
    pub qty: i64,
    pub maker_amount: u64,
    pub sell: bool,
}

/// 0.01 in 1e-6 units: the market-order maker amount (USDC of a buy, shares of a sell) has at
/// most 2 decimals.
const CENT_E6: u64 = 10_000;

/// `raw · 10^e` rounded toward zero (`None` when negative or out of range).
fn floor_scaled(raw: i128, e: i32) -> Option<u64> {
    let p = 10i128.checked_pow(e.unsigned_abs())?;
    let v = if e >= 0 { raw.checked_mul(p)? } else { raw / p };
    u64::try_from(v).ok()
}

/// Decimals `d` of a tick that is a power of ten (`tick · 10^px_exp = 10^-d`), 1–4 (the official
/// clients' tick sizes 0.1, 0.01, 0.001, 0.0001).
fn tick_decimals(tick: i64, px_exp: i8) -> Option<u32> {
    let mut k = 0i32;
    let mut t = tick;
    while t >= 10 && t % 10 == 0 {
        t /= 10;
        k += 1;
    }
    if t != 1 {
        return None;
    }
    u32::try_from(-(i32::from(px_exp) + k))
        .ok()
        .filter(|d| (1..=4).contains(d))
}

/// Side code and amounts (1e-6) of a FAK / FOK order of `qty` at worst price `px` (README "吃单",
/// the official clients' market-order amounts): a buy pays `makerAmount` = px·qty USDC rounded
/// down to 0.01 for at least `takerAmount` = makerAmount / px shares rounded down to the tick's
/// amount decimals (tick decimals + 2, at most 4); a sell gives `makerAmount` = qty shares rounded down to
/// 0.01 for at least `takerAmount` = makerAmount · px USDC. `None` when `px` is off the inst tick,
/// the tick is not a power of ten, or an amount rounds to 0.
pub(crate) fn taker_amounts(
    rd: &RefData,
    inst: hft_types::InstId,
    side: Side,
    px: i64,
    qty: i64,
) -> Option<(u8, u64, u64)> {
    let row = rd.inst(inst);
    if px <= 0 || qty <= 0 || row.tick <= 0 || px % row.tick != 0 {
        return None;
    }
    let dp = tick_decimals(row.tick, row.px_exp)?;
    // Amount decimals: the tick's dp + 2, but at most 4 — the exchange refuses a market order whose
    // taker amount has more ("taker amount a max of 4 decimals", live 2026-10-30 on a 0.01-tick
    // market whose refdata slot tick is 0.001). The amount's step in 1e-6 units.
    let step = 10u64.checked_pow(6u32.checked_sub((dp + 2).min(4))?)?;
    let pe = i32::from(row.px_exp);
    let qe = i32::from(row.qty_exp);
    let p10 = 10i128.checked_pow(pe.unsigned_abs())?;
    let (maker, taker) = match side {
        Side::Buy => {
            let usdc = floor_scaled(i128::from(px) * i128::from(qty), pe + qe + 6)?;
            let maker = usdc / CENT_E6 * CENT_E6;
            // shares (1e-6) = maker / (px · 10^pe).
            let (num, den) = if pe <= 0 {
                (i128::from(maker).checked_mul(p10)?, i128::from(px))
            } else {
                (i128::from(maker), i128::from(px).checked_mul(p10)?)
            };
            let shares = u64::try_from(num / den).ok()?;
            (maker, shares / step * step)
        }
        Side::Sell => {
            let shares = floor_scaled(i128::from(qty), qe + 6)?;
            let maker = shares / CENT_E6 * CENT_E6;
            let usdc = floor_scaled(i128::from(maker) * i128::from(px), pe)?;
            (maker, usdc / step * step)
        }
    };
    if maker == 0 || taker == 0 {
        return None;
    }
    Some((if side == Side::Buy { 0 } else { 1 }, maker, taker))
}

/// Signs a V2 order of the account's Deposit Wallet (signature type 3) into `sig`; returns its
/// hash (the exchange's `orderID`) and the signature length.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sign_v2(
    w: &Wallet,
    t: &Templates,
    token_id: &[u8; 32],
    side: u8,
    maker_amount: u64,
    taker_amount: u64,
    salt: u64,
    ts_ms: u64,
    sig: &mut [u8; 512],
) -> ([u8; 32], usize) {
    let o = OrderV2 {
        salt,
        maker: t.wallet,
        signer: t.wallet,
        token_id: *token_id,
        maker_amount,
        taker_amount,
        side,
        signature_type: 3,
        timestamp_ms: ts_ms,
        metadata: [0; 32],
        builder: [0; 32],
    };
    let n = sign::sign_order_1271(w, &t.domain, CHAIN_ID, &o, sig);
    (sign::order_digest(&t.domain, &o), n)
}

/// Writes the signed `POST /order`; returns the order hash (the exchange's `orderID`) and, for a
/// FAK / FOK order, what its reports need (README "吃单"). A resting order uses the matching
/// pre-signed order when there is one (README "预签名"), else signs now; a FAK / FOK order is
/// always signed now.
#[allow(clippy::too_many_arguments)]
fn place(
    rd: &RefData,
    t: &Templates,
    ctx: &mut GwCtx,
    k: &L2Keys<'_>,
    cred: &Credential,
    c: &GwCmd,
    clk: &WallClock,
    out: &mut RestReqBuf,
) -> Result<([u8; 32], Option<TakerSpec>), EncodeErr> {
    let row = rd.inst(c.inst);
    if c.inst.0 == 0 || row.venue != crate::VENUE {
        return Err(EncodeErr::UnknownInst);
    }
    let flags = OrderFlags(c.oflags);
    if flags.contains(OrderFlags::REDUCE_ONLY) || flags.contains(OrderFlags::POSITION_CLOSE) {
        return Err(EncodeErr::Unsupported(c.op));
    }
    // A taker buy spends USDC and may receive more shares than its size: only a spend order
    // (`OrderFlags::SPEND`) may be one; the flag means nothing on any other order (README "吃单").
    let taker_buy = matches!(c.tif, Tif::Ioc | Tif::Fok) && c.side == Side::Buy;
    if taker_buy != flags.contains(OrderFlags::SPEND) {
        return Err(EncodeErr::Unsupported(c.op));
    }
    // Resting: post-only (rejected if it would match) or a plain GTD bid/ask. Taker (README
    // "吃单"): FAK (`Ioc`: what does not match at once is killed) or FOK (all or nothing).
    let (post_only, order_type): (bool, &[u8]) = match c.tif {
        Tif::PostOnly => (true, b"GTD"),
        Tif::Gtc => (false, b"GTD"),
        Tif::Ioc => (false, b"FAK"),
        Tif::Fok => (false, b"FOK"),
    };
    let taker = matches!(c.tif, Tif::Ioc | Tif::Fok);
    let token = ctx.token_of(c.inst).ok_or(EncodeErr::UnknownInst)?;
    let token_s = core::str::from_utf8(token).map_err(|_| EncodeErr::UnknownInst)?;
    let token_id = sign::parse_u256(token_s).ok_or(EncodeErr::UnknownInst)?;
    let (side, maker_amount, taker_amount) = if taker {
        taker_amounts(rd, c.inst, c.side, c.px.0, c.qty.0)
    } else {
        amounts(rd, c.inst, c.side, c.px.0, c.qty.0)
    }
    .ok_or(EncodeErr::FieldOverflow)?;
    let side_s: &[u8] = if side == 0 { b"BUY" } else { b"SELL" };
    let now_ms = u64::try_from(clk.unix_ms).map_err(|_| EncodeErr::FieldOverflow)?;
    let mut sig = [0u8; 512];
    // Pre-signed orders are anchored on resting orders: a taker order never takes one.
    let pre = if taker {
        None
    } else {
        ctx.pre.take(c.inst, &token_id, c.side, c.px.0, c.qty.0, now_ms)
    };
    let (salt, ts, hash, n) = match pre {
        Some(p) => {
            sig = p.sig;
            (p.salt, p.ts_ms, p.hash, usize::from(p.sig_len))
        }
        None => {
            // `timestamp` is the V2 nonce: unique per account (README "下单").
            let ts = ctx.pre.next_ts(now_ms);
            // Below 2^53 (the body carries it as a JSON number).
            let salt = (ts & ((1 << 33) - 1)) << 20 | u64::from(c.slot.0 & 0xF_FFFF);
            let w = signer(k, t).ok_or(EncodeErr::Unsupported(c.op))?;
            let (hash, n) = sign_v2(
                &w,
                t,
                &token_id,
                side,
                maker_amount,
                taker_amount,
                salt,
                ts,
                &mut sig,
            );
            (salt, ts, hash, n)
        }
    };
    if !taker {
        ctx.pre.anchor(c.inst, c.side, c.px.0, c.qty.0);
    }
    let token = ctx.token_of(c.inst).ok_or(EncodeErr::UnknownInst)?;
    let mut sig_hex = [0u8; 1026];
    let sig_hex =
        sign::hex_bytes(sig.get(..n).unwrap_or(&[]), &mut sig_hex).ok_or(EncodeErr::FieldOverflow)?;
    let mut maker = [0u8; 42];
    let maker = sign::checksum_address(&t.wallet, &mut maker);
    // Not signed: the GTD lifetime counts from the place, also for a pre-signed order; a taker
    // order has none (`"0"`).
    let expiration = if taker {
        0
    } else {
        now_ms / 1000 + u64::from(t.gtd_s)
    };
    out.method = Method::Post;
    out.path_query.push(b"/order").map_err(of)?;
    let (mut n1, mut n2, mut n3, mut n4, mut n5) = ([0u8; 20], [0u8; 20], [0u8; 20], [0u8; 20], [0u8; 20]);
    // A taker order's body has no `postOnly` (the official clients' market orders).
    let tail: &[u8] = if taker {
        b"\",\"deferExec\":false}"
    } else if post_only {
        b"\",\"deferExec\":false,\"postOnly\":true}"
    } else {
        b"\",\"deferExec\":false,\"postOnly\":false}"
    };
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
        dec(ts, &mut n5).as_bytes(),
        b"\",\"metadata\":\"0x0000000000000000000000000000000000000000000000000000000000000000\"",
        b",\"builder\":\"0x0000000000000000000000000000000000000000000000000000000000000000\"",
        b",\"signature\":\"",
        sig_hex.as_bytes(),
        b"\"},\"owner\":\"",
        cred.api_key.as_slice(),
        b"\",\"orderType\":\"",
        order_type,
        tail,
    ];
    for p in parts {
        out.body.push(p).map_err(of)?;
    }
    let spec = taker.then_some(TakerSpec {
        qty: c.qty.0,
        maker_amount,
        sell: side == 1,
    });
    Ok((hash, spec))
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
