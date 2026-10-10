//! Advice records (SDK-175).

use hft_event::EventKind;
use hft_strategy_sdk::{AdvicePayload, EvMask, EventRef, Part, PayloadError, PayloadFull, PayloadWriter};

use super::*;
use crate::common::*;

/// A 16 B test payload (`ver`, `flags`, padding, `band`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct Band {
    flags: u8,
    band: i64,
}

impl AdvicePayload for Band {
    const LOCAL_KIND: u16 = 1;
    const MAX_LEN: usize = 16;
    fn encode(&self, w: &mut PayloadWriter<'_>) -> Result<(), PayloadFull> {
        w.put_u8(1)?;
        w.put_u8(self.flags)?;
        w.put_bytes(&[0; 6])?;
        w.put_i64(self.band)
    }
    fn decode(b: &[u8]) -> Result<Self, PayloadError> {
        let band = b.get(8..16).ok_or(PayloadError::Truncated)?;
        let mut x = [0u8; 8];
        x.copy_from_slice(band);
        Ok(Band {
            flags: b.get(1).copied().ok_or(PayloadError::Truncated)?,
            band: i64::from_le_bytes(x),
        })
    }
}

#[test]
fn sdk_175_advice_header_fields() {
    let b = b_gt3();
    set_subs(S1, vec![(A1, 0, EvMask::BBO, Part::Model)]);
    let mut h = b.boot();
    h.steady();
    let mut a = adv(&h, MemCkpt::default());
    catch_up(&mut a, &h);
    let p = Band {
        flags: 0,
        band: 1_234,
    };
    set_mhook(move |hx, ev, out| {
        if let EventRef::Bbo { .. } = ev
            && hx.s.strategy == S1
        {
            let _ = out.emit(GT2, &p);
        }
    });
    let m = h.mark();
    h.adv(77_000);
    h.quote(A1, "50000", "50004");
    h.run();
    let bbo = h
        .since(m)
        .into_iter()
        .find(|r| r.kind == EventKind::BBO)
        .expect("Bbo");
    let n0 = a.ring.recs.len();
    catch_up(&mut a, &h);
    SCRIPT.with(|x| x.borrow_mut().mhook = None);
    let adv: Vec<_> = a
        .ring
        .recs
        .iter()
        .skip(n0)
        .filter(|r| r.0 == EventKind::ADVICE)
        .cloned()
        .collect();
    assert_eq!(adv.len(), 1);
    // The record as it enters the journal.
    let m2 = h.mark();
    feed_advice(&a, &mut h, n0);
    let rec = h.kinds_since(m2, EventKind::ADVICE).pop().expect("Advice record");
    let hd = hft_sequencer::types::read_body::<hft_event::AdviceHdr>(&rec.body).expect("AdviceHdr");
    assert_eq!(hd.strategy, S1);
    assert_eq!(hd.account, AC1);
    assert_eq!(hd.local_kind, 1);
    assert_eq!(hd.group, GT2);
    assert_eq!(hd.len, 16);
    assert_eq!(hd.basis_seq, bbo.seq);
    assert_eq!(hd.basis_tsc.0, bbo.tsc);
    let mut buf = [0u8; 16];
    let mut w = PayloadWriter::new(&mut buf, 16);
    p.encode(&mut w).expect("encode");
    assert_eq!(&rec.body[32..], w.as_bytes());
    assert_eq!(adv[0].1, rec.body);
}
