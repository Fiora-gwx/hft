//! Core trades, public liquidations and bounded TopN snapshots. Validate before emitting.
use crate::{
    FeedCtx, Line,
    json::{fixed, from, time_ns},
    md::{Class, hdr, inst_of},
};
use hft_event::{L2SnapHdr, L2SnapshotPart, Level, Liquidation, Trade};
use hft_refdata::RefData;
use hft_types::{InstId, Px, Qty, Side};
use hft_venue_proto::{DecodeErr, MdOutcome, MdSink, RxMeta};
use serde::{
    Deserialize,
    de::{Deserializer, SeqAccess, Visitor},
};

struct Levels<'a>([Option<[&'a str; 2]>; 20]);
impl<'de: 'a, 'a> Deserialize<'de> for Levels<'a> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Levels<'de>;
            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str("at most twenty price/quantity pairs")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut result = Levels([None; 20]);
                let mut n = 0;
                while let Some(v) = a.next_element::<[&str; 2]>()? {
                    let slot = result
                        .0
                        .get_mut(n)
                        .ok_or_else(|| serde::de::Error::custom("too many levels"))?;
                    *slot = Some(v);
                    n += 1;
                }
                Ok(result)
            }
        }
        d.deserialize_seq(V)
    }
}
impl Levels<'_> {
    fn parse(&self, pe: i8, qe: i8, out: &mut [Level; 20]) -> Result<usize, DecodeErr> {
        let mut n = 0;
        for pair in self.0.iter().flatten() {
            let px = fixed(pair[0], pe)?;
            let qty = fixed(pair[1], qe)?;
            if px <= 0 || qty < 0 {
                return Err(DecodeErr::Number);
            }
            if qty != 0 {
                if let Some(slot) = out.get_mut(n) {
                    *slot = Level {
                        px: Px(px),
                        qty: Qty(qty),
                    };
                }
                n += 1;
            }
        }
        Ok(n)
    }
}
#[derive(Deserialize)]
struct Depth<'a> {
    #[serde(default, borrow)]
    s: &'a str,
    #[serde(rename = "u", alias = "lastUpdateId")]
    id: u64,
    #[serde(default, rename = "T")]
    time: i64,
    #[serde(default, rename = "E")]
    sent: i64,
    #[serde(borrow, rename = "b", alias = "bids")]
    bids: Levels<'a>,
    #[serde(borrow, rename = "a", alias = "asks")]
    asks: Levels<'a>,
}
#[derive(Deserialize)]
struct Combined<'a> {
    #[serde(borrow)]
    stream: &'a str,
    #[serde(borrow)]
    data: Depth<'a>,
}
#[derive(Deserialize)]
struct Print<'a> {
    #[serde(borrow)]
    s: &'a str,
    p: &'a str,
    q: &'a str,
    #[serde(rename = "a", alias = "t")]
    id: u64,
    #[serde(rename = "T")]
    time: i64,
    #[serde(rename = "E")]
    sent: i64,
    m: bool,
}
#[derive(Deserialize)]
struct ForceOrder<'a> {
    #[serde(borrow)]
    s: &'a str,
    ap: &'a str,
    z: &'a str,
    #[serde(rename = "S")]
    side: &'a str,
    #[serde(rename = "T")]
    time: i64,
}
#[derive(Deserialize)]
struct Force<'a> {
    #[serde(borrow)]
    o: ForceOrder<'a>,
    #[serde(rename = "E")]
    sent: i64,
}

pub(crate) fn decode(
    line: Line,
    rd: &RefData,
    ctx: &FeedCtx,
    class: Class,
    p: &[u8],
    rx: &RxMeta,
    out: &mut impl MdSink,
) -> Result<MdOutcome, DecodeErr> {
    let unit = ctx.time_unit.ns();
    match class {
        Class::DepthTopN => {
            let mut upper = [0u8; 32];
            let (d, inst) = if line == Line::Spot {
                let m: Combined<'_> = from(p)?;
                let (symbol, channel) = m.stream.split_once('@').ok_or(DecodeErr::Shape)?;
                if !matches!(channel, "depth5@100ms" | "depth10@100ms" | "depth20@100ms")
                    || symbol.len() > upper.len()
                {
                    return Err(DecodeErr::Shape);
                }
                for (dst, src) in upper.iter_mut().zip(symbol.bytes()) {
                    *dst = src.to_ascii_uppercase();
                }
                (m.data, inst_of(line, rd, &upper[..symbol.len()]))
            } else {
                let d: Depth<'_> = from(p)?;
                let inst = inst_of(line, rd, d.s.as_bytes());
                (d, inst)
            };
            if inst == InstId(0) {
                return Ok(MdOutcome { events: 0, inst });
            }
            let i = rd.inst(inst);
            let mut bids = [Level {
                px: Px(0),
                qty: Qty(0),
            }; 20];
            let mut asks = bids;
            let nb = d.bids.parse(i.px_wire_exp, i.qty_wire_exp, &mut bids)?;
            let na = d.asks.parse(i.px_wire_exp, i.qty_wire_exp, &mut asks)?;
            let mut h = hdr(line, inst, rx);
            if line == Line::Usdm {
                h.ex_ts_ns = time_ns(d.time, unit)?;
                h.ex_send_ns = time_ns(d.sent, unit)?;
            }
            out.l2_snapshot(&L2SnapshotPart {
                hdr: L2SnapHdr {
                    h,
                    snap_id: d.id,
                    origin: L2SnapHdr::TOP_N,
                    _pad: [0; 3],
                    n_bids: nb as u16,
                    n_asks: na as u16,
                },
                bids: &bids[..nb],
                asks: &asks[..na],
                part: 0,
                parts: 1,
            });
            Ok(MdOutcome { events: 1, inst })
        }
        Class::AggTrade | Class::SpotTrade => {
            let m: Print<'_> = from(p)?;
            let inst = inst_of(line, rd, m.s.as_bytes());
            if inst == InstId(0) {
                return Ok(MdOutcome { events: 0, inst });
            }
            let i = rd.inst(inst);
            let mut h = hdr(line, inst, rx);
            h.ex_ts_ns = time_ns(m.time, unit)?;
            h.ex_send_ns = time_ns(m.sent, unit)?;
            let px = fixed(m.p, i.px_wire_exp)?;
            let qty = fixed(m.q, i.qty_wire_exp)?;
            if px <= 0 || qty <= 0 {
                return Err(DecodeErr::Number);
            }
            out.trade(&Trade {
                h,
                px: Px(px),
                qty: Qty(qty),
                trade_id: m.id,
                taker_side: if m.m { Side::Sell } else { Side::Buy },
                _pad: [0; 7],
            });
            Ok(MdOutcome { events: 1, inst })
        }
        Class::ForceOrder if line == Line::Usdm => {
            let m: Force<'_> = from(p)?;
            let inst = inst_of(line, rd, m.o.s.as_bytes());
            if inst == InstId(0) {
                return Ok(MdOutcome { events: 0, inst });
            }
            let i = rd.inst(inst);
            let mut h = hdr(line, inst, rx);
            h.ex_ts_ns = time_ns(m.o.time, unit)?;
            h.ex_send_ns = time_ns(m.sent, unit)?;
            let side = match m.o.side {
                "BUY" => Side::Buy,
                "SELL" => Side::Sell,
                _ => return Err(DecodeErr::Shape),
            };
            out.liquidation(&Liquidation {
                h,
                px: Px(fixed(m.o.ap, i.px_wire_exp)?),
                qty: Qty(fixed(m.o.z, i.qty_wire_exp)?),
                side,
                _pad: [0; 7],
            });
            Ok(MdOutcome { events: 1, inst })
        }
        _ => Err(DecodeErr::UnknownClass),
    }
}
