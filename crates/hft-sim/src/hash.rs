//! `state_hash`: xxh3-64 over a little-endian rendering of the whole simulated state (books,
//! funding/mark state, accounts, pending queues in heap order). Counters are not hashed.

use hft_event::RepMsg;
use xxhash_rust::xxh3::Xxh3;
use zerocopy::IntoBytes;

use crate::SimExchange;
use crate::state::Bulk;

fn rep_bytes(h: &mut Xxh3, m: &RepMsg) {
    match m {
        RepMsg::OrderEvent(x, _) => {
            h.update(&[1]);
            h.update(x.as_bytes());
        }
        RepMsg::SendReport(x) => {
            h.update(&[2]);
            h.update(x.as_bytes());
        }
        RepMsg::PositionReport(x) => {
            h.update(&[3]);
            h.update(x.as_bytes());
        }
        RepMsg::BalanceReport(x) => {
            h.update(&[4]);
            h.update(x.as_bytes());
        }
        RepMsg::FundingSettle(x) => {
            h.update(&[5]);
            h.update(x.as_bytes());
        }
        RepMsg::ConnState(x) => {
            h.update(&[6]);
            h.update(x.as_bytes());
        }
        RepMsg::ExternalFill(x) => {
            h.update(&[7]);
            h.update(x.as_bytes());
        }
        RepMsg::AccountConfigReport(x) => {
            h.update(&[8]);
            h.update(x.as_bytes());
        }
        RepMsg::MarginReport(x) => {
            h.update(&[9]);
            h.update(x.as_bytes());
        }
        RepMsg::Note(x) => {
            h.update(&[10]);
            h.update(x.as_bytes());
        }
    }
}

pub(crate) fn state_hash(ex: &SimExchange) -> u64 {
    let mut h = Xxh3::new();
    let u = |h: &mut Xxh3, v: u64| h.update(&v.to_le_bytes());
    let i = |h: &mut Xxh3, v: i64| h.update(&v.to_le_bytes());
    let w = |h: &mut Xxh3, v: i128| h.update(&v.to_le_bytes());
    for r in &ex.insts {
        u(&mut h, u64::from(r.id.0));
        i(&mut h, r.bbo.bid_px);
        i(&mut h, r.bbo.bid_qty);
        i(&mut h, r.bbo.ask_px);
        i(&mut h, r.bbo.ask_qty);
        i(&mut h, r.bbo.ex_ts);
        h.update(&[
            u8::from(r.bbo.set),
            u8::from(r.book_set),
            u8::from(r.mark_set),
            u8::from(r.reduce_only),
        ]);
        h.update(&r.book.state_hash().to_le_bytes());
        i(&mut h, r.book_ex_ts);
        i(&mut h, r.mark);
        i(&mut h, r.rate);
        i(&mut h, r.next_funding_ns);
        i(&mut h, r.settled_ns);
    }
    for a in &ex.accts {
        h.update(&[a.id.0]);
        for b in &a.bal {
            i(&mut h, b.total);
            i(&mut h, b.locked);
            h.update(&[u8::from(b.reported)]);
        }
        for p in &a.pos {
            i(&mut h, p.qty);
            w(&mut h, p.cost);
            h.update(&[u8::from(p.reported)]);
        }
        for o in a.orders.iter().filter(|o| o.used) {
            h.update(&[u8::from(o.resting), o.status, o.side as u8, o.tif as u8]);
            u(&mut h, u64::from(o.slot));
            u(&mut h, u64::from(o.r#gen));
            u(&mut h, u64::from(o.inst));
            i(&mut h, o.px);
            i(&mut h, o.qty);
            i(&mut h, o.filled);
            w(&mut h, o.fill_val);
            i(&mut h, o.cash_qty);
            i(&mut h, o.cash_sum);
            u(&mut h, o.exch_id);
            i(&mut h, o.lock);
        }
        for e in a.mem.iter().filter(|e| e.used) {
            i(&mut h, e.px);
            i(&mut h, e.qty);
            u(&mut h, e.t0);
        }
        h.update(&a.rng.get_word_pos().to_le_bytes());
        u(&mut h, a.next_exch_id);
        u(&mut h, a.next_trade_id);
        for c in a.conn_up.iter() {
            h.update(&[u8::from(*c)]);
        }
        for m in &a.mrep {
            h.update(&[u8::from(m.has)]);
            u(&mut h, m.last_tsc);
            i(&mut h, m.last_ratio);
        }
    }
    for c in ex.cmds.iter() {
        u(&mut h, c.0.arrive);
        u(&mut h, c.0.seq);
        h.update(c.0.cmd.as_bytes());
    }
    for r in ex.replies.iter() {
        u(&mut h, r.0.due);
        h.update(&[r.0.account.0]);
        rep_bytes(&mut h, &r.0.msg);
        if let Bulk::Margin(k) = r.0.bulk {
            let n = usize::from(ex.margin_n.get(usize::from(k)).copied().unwrap_or(0));
            if let Some(items) = ex.margin_slab.get(usize::from(k)) {
                for it in items.iter().take(n) {
                    h.update(it.as_bytes());
                }
            }
        }
    }
    for x in &ex.transfers {
        u(&mut h, x.at);
        h.update(&[x.acct, x.venue.0]);
        i(&mut h, x.amount);
    }
    for x in &ex.late_fees {
        u(&mut h, x.at);
        h.update(&[x.acct, x.venue.0]);
        i(&mut h, x.fee);
    }
    u(&mut h, ex.next_truth);
    h.digest()
}
