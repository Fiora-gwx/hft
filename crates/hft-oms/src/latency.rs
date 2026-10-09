//! Latency samples: synthesized on the first exchange response of a stamped operation
//! (README "延迟样本").

use hft_event::{LatencySample, OrderEvent, OrderEventKind, RecordHeader};
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
            src: 0,
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
