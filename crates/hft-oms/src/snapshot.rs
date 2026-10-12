//! Snapshot sections 2 (`OrderSlots`), 3 (`OmsCounters`) and 12 (`OmsLinks`) and `restore`.

use hft_refdata::RefData;
use hft_strategy_sdk::{HedgePlan, LinkRole, RestoreError, SnapshotReader, SnapshotWriter};
use hft_types::{GroupId, Qty, Tsc};
use zerocopy::{FromBytes, IntoBytes, TryFromBytes};

use crate::Oms;
use crate::config::OmsConfig;
use crate::table::GroupEntry;
use crate::types::{
    LegRowBody, LegState, LinkEntry, LinkRowHdr, LinkState, OmsCountersHdr, OrderSlot, OrderState, OutItem,
    PlanRowBody, ROW_VERSION_COUNTERS, ROW_VERSION_LINKS, ROW_VERSION_SLOTS, SECTION_OMS_COUNTERS,
    SECTION_OMS_LINKS, SECTION_ORDER_SLOTS, SLOT_ROW_LEN, SLOT_ROW_LEN_V1, SlotExt, SlotFlags, TableRow,
    Tomb, TombRowBody, out_kind,
};

const ROW_LINK: u8 = 1;
const ROW_PLAN: u8 = 2;
const ROW_LEG: u8 = 3;
const ROW_TOMB: u8 = 4;
const ROW_OUT: u8 = 5;
const ROW_FOREIGN_CASH: u8 = 6;
const ROW_FOREIGN_RECEIPT: u8 = 7;

fn hdr(kind: u8, len: usize) -> LinkRowHdr {
    LinkRowHdr {
        kind,
        _pad: [0; 3],
        len: len as u32,
    }
}

impl Oms {
    /// Writes sections 2, 3 and 12 (README "快照行格式").
    pub fn snapshot(&self, w: &mut SnapshotWriter) {
        // Section 2.
        w.begin_section(SECTION_ORDER_SLOTS, ROW_VERSION_SLOTS);
        for t in self.tables.iter().flatten() {
            for s in 1..=t.n {
                let (Some(sl), Some(ex)) = (t.slots.get(s as usize), t.ext.get(s as usize)) else {
                    continue;
                };
                if sl.state == OrderState::Free {
                    continue;
                }
                w.put_bytes(sl.as_bytes());
                w.put_bytes(ex.as_bytes());
                w.add_rows(1);
            }
        }
        // Section 3.
        w.begin_section(SECTION_OMS_COUNTERS, ROW_VERSION_COUNTERS);
        let n_tables = self.tables.iter().flatten().count() as u32;
        w.put_row(&OmsCountersHdr {
            engine_epoch: self.engine_epoch,
            n_tables,
            resume_hold_until: self.resume_hold_until,
        });
        for t in self.tables.iter().flatten() {
            let start = w.len();
            w.put_row(&TableRow {
                venue: t.venue,
                account: t.account,
                conn_up: u8::from(t.conn_up),
                hedge_gate: u8::from(t.hedge_gate),
                n_slots: t.n,
                free_len: t.free.len() as u32,
                sweep_cursor: t.sweep_cursor,
                unknown_count: t.unknown_count,
                n_foreign: t.foreign.len() as u32,
                n_ext_held: t.ext_held.len() as u32,
                smoke_slot: t.smoke_slot,
            });
            for s in t.free.iter() {
                w.put_u32(s);
            }
            for sl in t.slots.iter().skip(1) {
                w.put_u32(sl.r#gen);
            }
            for (k, v) in t.foreign.iter().chain(t.ext_held.iter()) {
                w.put_u64(k);
                w.put_i64(v.0);
            }
            w.put_u8(t.decode_fails);
            w.put_bytes(&[0; 7]);
            let len = w.len() - start;
            w.put_bytes([0u8; 8].get(..len.next_multiple_of(8) - len).unwrap_or(&[]));
        }
        // Section 12.
        // Keep each established encoding when its appended row kind is absent.
        let version = if self
            .tables
            .iter()
            .flatten()
            .any(|t| t.foreign_receipts.iter().any(|r| r.exch_order_id != 0))
        {
            ROW_VERSION_LINKS
        } else if self
            .tables
            .iter()
            .flatten()
            .any(|t| t.foreign_cash.iter().any(|r| r.exch_order_id != 0))
        {
            3
        } else {
            2
        };
        w.begin_section(SECTION_OMS_LINKS, version);
        let live = |e: &&LinkEntry| !matches!(e.state, LinkState::Free | LinkState::Done);
        for e in self.links.iter().filter(live) {
            w.put_row(&hdr(ROW_LINK, 80));
            w.put_bytes(e.as_bytes());
        }
        for e in self.links.iter().filter(live) {
            let i = (e.link_id as usize).saturating_sub(1);
            if let Some((tol, plan)) = self.plans.get(i) {
                w.put_row(&hdr(ROW_PLAN, 64));
                w.put_bytes(
                    PlanRowBody {
                        link_id: e.link_id,
                        _pad: 0,
                        tolerance: *tol,
                        plan: *plan,
                    }
                    .as_bytes(),
                );
            }
        }
        for g in &self.groups {
            if g.st.is_zero_ignoring_legs_def() {
                continue;
            }
            w.put_row(&hdr(ROW_LEG, 200));
            w.put_bytes(
                LegRowBody {
                    account: g.account,
                    _pad: [0; 3],
                    group: g.group,
                    state: g.st,
                }
                .as_bytes(),
            );
        }
        let tg = u32::from(self.p.tomb_gens);
        for t in self.tables.iter().flatten() {
            for s in 1..=t.n {
                let mut v: [Option<&Tomb>; 4] = [None; 4];
                for (k, tb) in t
                    .tombs
                    .iter()
                    .skip(s as usize * tg as usize)
                    .take(tg as usize)
                    .enumerate()
                {
                    if tb.r#gen != 0
                        && let Some(x) = v.get_mut(k)
                    {
                        *x = Some(tb);
                    }
                }
                let mut l: [&Tomb; 4] = [&Tomb::EMPTY; 4];
                let mut n = 0;
                for tb in v.into_iter().flatten() {
                    if let Some(x) = l.get_mut(n) {
                        *x = tb;
                        n += 1;
                    }
                }
                let l = l.get_mut(..n).unwrap_or(&mut []);
                l.sort_by_key(|t| t.r#gen);
                for tb in l.iter() {
                    w.put_row(&hdr(ROW_TOMB, 80));
                    w.put_bytes(
                        TombRowBody {
                            venue: t.venue,
                            account: t.account,
                            _pad: 0,
                            slot: s,
                            tomb: **tb,
                        }
                        .as_bytes(),
                    );
                }
            }
        }
        for it in self.gq.iter() {
            w.put_row(&hdr(ROW_OUT, 32));
            w.put_bytes(it.as_bytes());
        }
        for t in self.tables.iter().flatten() {
            for it in t.q_rr.iter().chain(t.q_open.iter()) {
                w.put_row(&hdr(ROW_OUT, 32));
                w.put_bytes(it.as_bytes());
            }
        }
        for t in self.tables.iter().flatten() {
            for row in t.foreign_cash.iter().filter(|r| r.exch_order_id != 0) {
                w.put_row(&hdr(ROW_FOREIGN_CASH, 32));
                w.put_bytes(row.as_bytes());
            }
        }
        for t in self.tables.iter().flatten() {
            for row in t.foreign_receipts.iter().filter(|r| r.exch_order_id != 0) {
                w.put_row(&hdr(ROW_FOREIGN_RECEIPT, 72));
                w.put_bytes(row.as_bytes());
            }
        }
        w.end_section();
    }

    /// Loads sections 2, 3 and 12 (no TSC is changed; `recovery_end` shifts them).
    pub fn restore(r: &mut SnapshotReader, cfg: &OmsConfig, rd: &RefData) -> Result<Oms, RestoreError> {
        let mut o = Oms::new(cfg, rd).map_err(|_| RestoreError::BadValue)?;
        let s3 = r
            .find_section(SECTION_OMS_COUNTERS)?
            .ok_or(RestoreError::BadSection)?;
        let s2 = r
            .find_section(SECTION_ORDER_SLOTS)?
            .ok_or(RestoreError::BadSection)?;
        let s12 = r
            .find_section(SECTION_OMS_LINKS)?
            .ok_or(RestoreError::BadSection)?;
        if !(1..=ROW_VERSION_SLOTS).contains(&s2.version)
            || s3.version != ROW_VERSION_COUNTERS
            || !(1..=ROW_VERSION_LINKS).contains(&s12.version)
        {
            return Err(RestoreError::VersionUnsupported);
        }
        // ---- section 3
        let mut rr = s3.reader();
        let h: OmsCountersHdr = rr.get_row()?;
        o.resume_hold_until = h.resume_hold_until;
        let mut expect_unknown: Vec<(usize, u32)> = Vec::new();
        for _ in 0..h.n_tables {
            let start = rr.pos();
            let row: TableRow = rr.get_row()?;
            let ti = o.tix(row.venue, row.account).ok_or(RestoreError::BadValue)?;
            let t = o.tm(ti).ok_or(RestoreError::BadValue)?;
            if row.n_slots != t.n {
                return Err(RestoreError::Capacity);
            }
            if row.conn_up > 1 || row.hedge_gate > 1 || row.smoke_slot > t.n || row.free_len > t.n {
                return Err(RestoreError::BadValue);
            }
            t.conn_up = row.conn_up != 0;
            t.hedge_gate = row.hedge_gate != 0;
            t.sweep_cursor = row.sweep_cursor.min(t.n);
            t.smoke_slot = row.smoke_slot;
            t.free.clear();
            for _ in 0..row.free_len {
                let s = rr.get_u32()?;
                if s == 0 || s > t.n {
                    return Err(RestoreError::BadValue);
                }
                t.free.push(s);
            }
            for s in 1..=t.n {
                let g = rr.get_u32()?;
                if let Some(x) = t.slots.get_mut(s as usize) {
                    x.r#gen = g;
                }
            }
            if row.n_foreign as usize > t.foreign.q.cap() || row.n_ext_held as usize > t.ext_held.q.cap() {
                return Err(RestoreError::Capacity);
            }
            t.foreign.clear();
            t.ext_held.clear();
            for k in 0..(row.n_foreign + row.n_ext_held) {
                let key = rr.get_u64()?;
                let v = Qty(rr.get_i64()?);
                if v.0 <= 0 {
                    return Err(RestoreError::BadValue);
                }
                if k < row.n_foreign {
                    t.foreign.q.push((key, v));
                } else {
                    t.ext_held.q.push((key, v));
                }
            }
            t.decode_fails = rr.get_u8()?;
            let z = rr.get_bytes(7)?;
            let len = rr.pos() - start;
            let padn = len.next_multiple_of(8) - len;
            let p = rr.get_bytes(padn)?;
            if z.iter().chain(p.iter()).any(|b| *b != 0) {
                return Err(RestoreError::BadValue);
            }
            expect_unknown.push((ti, row.unknown_count));
        }
        // ---- section 2
        let mut rr = s2.reader();
        for _ in 0..s2.count {
            // Row version 1 has the 32-byte `SlotExt` without `fee_cum`: its fees are taken as
            // booked up to `filled` (README "快照行格式").
            let v1 = s2.version == 1;
            let b = rr.get_bytes(if v1 { SLOT_ROW_LEN_V1 } else { SLOT_ROW_LEN })?;
            let (b1, b2) = b.split_at(128);
            let sl = OrderSlot::try_read_from_bytes(b1).map_err(|_| RestoreError::BadValue)?;
            let mut ext = [0u8; SLOT_ROW_LEN - 128];
            ext.get_mut(..b2.len())
                .ok_or(RestoreError::BadValue)?
                .copy_from_slice(b2);
            let mut ex = SlotExt::read_from_bytes(&ext).map_err(|_| RestoreError::BadValue)?;
            if v1 {
                ex.fee_cum = sl.filled;
            }
            if s2.version < 3 {
                ex.cash_qty = hft_types::Qty(0);
                ex.cash_sum = hft_types::Qty(0);
            }
            if sl.state == OrderState::Free || ex.role > LinkRole::Reverse as u8 || ex.origin > 4 {
                return Err(RestoreError::BadValue);
            }
            if !crate::cash::valid_cursor(o.inst(sl.inst).perp_cash, sl.filled, ex.cash_qty, ex.cash_sum) {
                return Err(RestoreError::BadValue);
            }
            let ti = o.tix(sl.venue, sl.account).ok_or(RestoreError::BadValue)?;
            let t = o.tm(ti).ok_or(RestoreError::BadValue)?;
            let s = sl.slot_id;
            if s == 0 || s > t.n || t.slots.get(s as usize).is_none_or(|x| x.r#gen != sl.r#gen) {
                return Err(RestoreError::BadValue);
            }
            if let Some(x) = t.slots.get_mut(s as usize) {
                *x = sl;
                x.sflags.0 &= !SlotFlags::QUEUED;
            }
            if let Some(x) = t.ext.get_mut(s as usize) {
                *x = ex;
            }
        }
        // ---- section 12
        let mut rr = s12.reader();
        let mut leg_rows: Vec<(usize, LegState)> = Vec::new();
        while !rr.is_empty() {
            let hd: LinkRowHdr = rr.get_row()?;
            if hd._pad != [0; 3] {
                return Err(RestoreError::BadValue);
            }
            let want = match hd.kind {
                ROW_LINK => 80,
                ROW_PLAN => 64,
                ROW_LEG => 200,
                ROW_TOMB => {
                    if s12.version == 1 {
                        64
                    } else {
                        80
                    }
                }
                ROW_OUT => 32,
                ROW_FOREIGN_CASH if s12.version >= 3 => 32,
                ROW_FOREIGN_RECEIPT if s12.version >= 4 => 72,
                0 if hd.len == 0 => {
                    // Section tail padding.
                    break;
                }
                _ => return Err(RestoreError::BadValue),
            };
            if hd.len as usize != want {
                return Err(RestoreError::BadValue);
            }
            let b = rr.get_bytes(want)?;
            match hd.kind {
                ROW_FOREIGN_RECEIPT => {
                    let row = crate::types::ForeignCashReceipt::read_from_bytes(b)
                        .map_err(|_| RestoreError::BadValue)?;
                    if row.exch_order_id == 0
                        || row._pad != 0
                        || row._pad2 != [0; 6]
                        || !matches!(row.side, 1 | 2)
                        || row.qty.0 <= 0
                        || row.px.0 <= 0
                        || (row.trade_id == 0 && row.fill_seq != row.qty.0 as u64)
                        || (row.trade_id != 0 && row.fill_seq != 0)
                        || (row.fee_asset.0 == 0 && row.fee.0 != 0)
                        || rd.asset(row.fee_asset).id != row.fee_asset
                    {
                        return Err(RestoreError::BadValue);
                    }
                    let ti = o.tix(row.venue, row.account).ok_or(RestoreError::BadValue)?;
                    let t = o.tm(ti).ok_or(RestoreError::BadValue)?;
                    let ix = t
                        .foreign_cash
                        .iter()
                        .position(|r| {
                            r.side == 0 && r.inst == row.inst && r.exch_order_id == row.exch_order_id
                        })
                        .ok_or(RestoreError::BadValue)?;
                    let dst = t.foreign_receipts.get_mut(ix).ok_or(RestoreError::Capacity)?;
                    if dst.exch_order_id != 0 {
                        return Err(RestoreError::BadValue);
                    }
                    *dst = row;
                }
                ROW_FOREIGN_CASH => {
                    let row = crate::types::ForeignCashRow::read_from_bytes(b)
                        .map_err(|_| RestoreError::BadValue)?;
                    let inst = o.inst(row.inst);
                    if row.exch_order_id == 0
                        || row._pad != 0
                        || !matches!(row.side, 0..=2)
                        || (row.side == 0 && (row.filled.0 != 0 || row.cash_sum.0 != 0))
                        || (row.side != 0 && row.filled.0 <= 0)
                        || !inst.valid
                        || !inst.perp_cash
                        || inst.venue != row.venue
                    {
                        return Err(RestoreError::BadValue);
                    }
                    let ti = o.tix(row.venue, row.account).ok_or(RestoreError::BadValue)?;
                    let t = o.tm(ti).ok_or(RestoreError::BadValue)?;
                    if t.foreign_cash
                        .iter()
                        .any(|r| r.exch_order_id == row.exch_order_id && r.inst == row.inst)
                    {
                        return Err(RestoreError::BadValue);
                    }
                    let dst = t
                        .foreign_cash
                        .iter_mut()
                        .find(|r| r.exch_order_id == 0)
                        .ok_or(RestoreError::Capacity)?;
                    *dst = row;
                }
                ROW_LINK => {
                    let e = LinkEntry::try_read_from_bytes(b).map_err(|_| RestoreError::BadValue)?;
                    let i = (e.link_id as usize)
                        .checked_sub(1)
                        .ok_or(RestoreError::BadValue)?;
                    if matches!(e.state, LinkState::Free | LinkState::Done)
                        || (e.plan & 0x00FF_FFFF) as usize != i
                    {
                        return Err(RestoreError::BadValue);
                    }
                    let x = o.links.get_mut(i).ok_or(RestoreError::Capacity)?;
                    *x = e;
                }
                ROW_PLAN => {
                    let p = PlanRowBody::try_read_from_bytes(b).map_err(|_| RestoreError::BadValue)?;
                    let i = (p.link_id as usize)
                        .checked_sub(1)
                        .ok_or(RestoreError::BadValue)?;
                    let x = o.plans.get_mut(i).ok_or(RestoreError::Capacity)?;
                    *x = (p.tolerance, p.plan);
                }
                ROW_LEG => {
                    let l = LegRowBody::try_read_from_bytes(b).map_err(|_| RestoreError::BadValue)?;
                    if o.groups.len() >= o.groups.capacity() {
                        return Err(RestoreError::Capacity);
                    }
                    let key = (l.account, l.group);
                    let at = match o.groups.binary_search_by_key(&key, |e| (e.account, e.group)) {
                        Ok(_) => return Err(RestoreError::BadValue),
                        Err(at) => at,
                    };
                    o.groups.insert(
                        at,
                        GroupEntry {
                            account: l.account,
                            group: l.group,
                            n_legs: l.state.view.n_legs.min(4),
                            insts: [hft_types::InstId(0); 4],
                            in_spec: false,
                            st: l.state,
                        },
                    );
                    leg_rows.push((0, l.state));
                }
                ROW_TOMB => {
                    let mut bytes = [0u8; 80];
                    bytes
                        .get_mut(..b.len())
                        .ok_or(RestoreError::BadValue)?
                        .copy_from_slice(b);
                    let tr = TombRowBody::try_read_from_bytes(&bytes).map_err(|_| RestoreError::BadValue)?;
                    if !crate::cash::valid_cursor(
                        o.inst(tr.tomb.inst).perp_cash,
                        tr.tomb.filled,
                        tr.tomb.cash_qty,
                        tr.tomb.cash_sum,
                    ) {
                        return Err(RestoreError::BadValue);
                    }
                    let tg = o.p.tomb_gens;
                    let ti = o.tix(tr.venue, tr.account).ok_or(RestoreError::BadValue)?;
                    let t = o.tm(ti).ok_or(RestoreError::BadValue)?;
                    if tr.slot == 0 || tr.slot > t.n || tr.tomb.r#gen == 0 {
                        return Err(RestoreError::BadValue);
                    }
                    let ix = t.tomb_ix(tr.slot, tr.tomb.r#gen, tg);
                    let x = t.tombs.get_mut(ix).ok_or(RestoreError::BadValue)?;
                    *x = tr.tomb;
                }
                _ => {
                    let it = OutItem::try_read_from_bytes(b).map_err(|_| RestoreError::BadValue)?;
                    if it.kind == 0 || it.kind > out_kind::AGED {
                        return Err(RestoreError::BadValue);
                    }
                    if it.kind == out_kind::DECISION {
                        let ti = o
                            .tix(it.slot.venue, it.slot.account)
                            .ok_or(RestoreError::BadValue)?;
                        o.enqueue_decision(ti, it);
                    } else if !o.gq.push(it) {
                        return Err(RestoreError::Capacity);
                    }
                }
            }
        }
        // ---- derived state: in-flight, LegTracker in-flight, counters
        let saved: Vec<(hft_types::AccountId, GroupId, LegState)> =
            o.groups.iter().map(|g| (g.account, g.group, g.st)).collect();
        for g in o.groups.iter_mut() {
            for l in g.st.view.leg.iter_mut() {
                l.inflight_immediate = Qty(0);
                l.inflight_resting = Qty(0);
            }
            g.st.pending_slots = 0;
            g.st.unknown_slots = 0;
        }
        o.inflight.clear();
        for t in o.tables.iter_mut().flatten() {
            t.unknown_count = 0;
        }
        for a in o.aux.iter_mut() {
            *a = crate::LinkAux::default();
        }
        for ti in 0..o.tables.len() {
            let n = o.t(ti).map_or(0, |t| t.n);
            for s in 1..=n {
                let c = o.contrib(ti, s);
                o.apply(ti, &c, 1);
            }
        }
        for (a, g, st) in saved {
            let Some(gi) = o.group_ix(a, g) else { continue };
            let Some(e) = o.groups.get(gi) else { continue };
            let ok = e.st.view.leg.iter().zip(st.view.leg.iter()).all(|(x, y)| {
                x.inflight_immediate == y.inflight_immediate && x.inflight_resting == y.inflight_resting
            }) && e.st.pending_slots == st.pending_slots
                && e.st.unknown_slots == st.unknown_slots;
            if !ok {
                return Err(RestoreError::BadValue);
            }
            if let Some(e) = o.groups.get_mut(gi) {
                e.st.view = st.view;
            }
        }
        for (ti, u) in expect_unknown {
            if o.t(ti).is_some_and(|t| t.unknown_count != u) {
                return Err(RestoreError::BadValue);
            }
        }
        // Validate link/plan pairs.
        for (i, e) in o.links.iter().enumerate() {
            if !matches!(e.state, LinkState::Free | LinkState::Done)
                && o.plans
                    .get(i)
                    .is_none_or(|(_, p): &(hft_types::Rate, HedgePlan)| p.ratio_den == 0)
            {
                return Err(RestoreError::BadValue);
            }
        }
        let _ = leg_rows;
        let _ = Tsc(0);
        Ok(o)
    }
}
