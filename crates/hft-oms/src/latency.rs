//! Latency samples: synthesized on the first exchange response of a stamped operation
//! (README "延迟样本").

use hft_event::{LatencySample, OrderEvent, OrderEventKind, RecordFlags, RecordHeader};
use hft_types::{SlotId, Tsc};

use crate::Oms;
use crate::types::{OmsOut, SlotStamps};

impl Oms {
    pub(crate) fn maybe_sample(
        &mut self,
        ti: usize,
        s: u32,
        ev: &OrderEvent,
        hdr: &RecordHeader,
        out: &mut OmsOut,
    ) {
        let Some(t) = self.t(ti) else { return };
        let (Some(sl), Some(ex), Some(st)) = (t.slot(s), t.ext.get(s as usize), t.stamps.get(s as usize))
        else {
            return;
        };
        use OrderEventKind as K;
        let first = match st.op {
            SlotStamps::OP_NEW => matches!(
                ev.what,
                K::Ack | K::Reject | K::PartialFill | K::Fill | K::CancelAck | K::Expired
            ),
            SlotStamps::OP_CANCEL => matches!(ev.what, K::CancelAck | K::CancelReject),
            SlotStamps::OP_AMEND => matches!(ev.what, K::AmendAck | K::AmendReject),
            _ => false,
        };
        if !first {
            return;
        }
        let ls = LatencySample {
            trigger_seq: st.trigger_seq,
            ack_seq: hdr.seq,
            slot: SlotId(s),
            r#gen: sl.r#gen,
            venue: sl.venue,
            account: sl.account,
            strategy: sl.strategy,
            _pad: 0,
            src: point(st.trig_ex_ts_ns, 2, 0)
                | point(st.trig_ex_send_ns, 2, 1)
                | point(st.trig_hw_rx_ns, rx_source(u16::from(st.rx_flags)), 2)
                | point(st.trig_rx_tsc as i64, 3, 3)
                | point(st.trig_proc_tsc as i64, 3, 4)
                | point(st.decided_tsc as i64, 3, 5)
                | point(st.sent_tsc as i64, 3, 6)
                | point(ev.hw_rx_ns, rx_source(hdr.flags), 7),
            ex_ts_ns: st.trig_ex_ts_ns,
            ex_send_ns: st.trig_ex_send_ns,
            hw_rx_ns: st.trig_hw_rx_ns,
            rx_tsc: Tsc(st.trig_rx_tsc),
            proc_tsc: Tsc(st.trig_proc_tsc),
            decided_tsc: Tsc(st.decided_tsc),
            sent_tsc: Tsc(st.sent_tsc),
            ack_hw_rx_ns: ev.hw_rx_ns,
            ack_rx_tsc: Tsc(ev.rx_tsc),
            ack_ex_ts_ns: ev.ex_ts_ns,
            sw_rx_ns: st.trig_sw_rx_ns,
            link_role: ex.role,
            action: st.op,
            _pad2: [0; 6],
            trigger_kind: st.trig_kind,
            _pad3: [0; 6],
        };
        let r = out.latency.push(ls);
        debug_assert!(r.is_ok(), "OmsOut.latency overflow");
        self.clear_stamp(ti, s);
    }
}

fn rx_source(flags: u16) -> u32 {
    if flags & RecordFlags::SW_TS.0 != 0 {
        2
    } else if flags & RecordFlags::HW_TS.0 != 0 {
        1
    } else {
        0
    }
}

fn point(value: i64, source: u32, index: u32) -> u32 {
    if value == 0 { 0 } else { source << (index * 2) }
}
