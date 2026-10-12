use hft_event::*;
use hft_types::{AccountId, InstId, Px, Qty, Side, Tsc, VenueId};
use zerocopy::{FromZeros, IntoBytes};

#[test]
fn evt_204_reconciliation_batch_is_bounded_and_complete() {
    let mut summary = ReconcileReport::new_zeroed();
    summary.report_id = 7;
    summary.account = AccountId(1);
    summary.venue = VenueId(2);
    summary.count = 2;
    summary.item = 5;
    let mut position = summary;
    position.idx = 1;
    position.item = 1;
    position.inst = InstId(1);
    position.engine_qty = Qty(2);
    position.exch_qty = Qty(1);
    position.sflags = ReconcileReport::AUTO_EXTERNAL;
    let e = ExternalFill {
        account: summary.account,
        venue: summary.venue,
        inst: position.inst,
        side: Side::Sell,
        cause: ExternalCause::Adl,
        qty: Qty(1),
        px: Px(1),
        fee: Qty(0),
        fee_asset: hft_types::ASSET_NONE,
        _pad: [0; 6],
        exch_order_id: 0,
        trade_id: 9,
        ex_ts_ns: 7,
        rx_tsc: Tsc(0),
        fill_seq: 0,
        cash_qty: Qty(1),
        cash_amount: Qty(-1),
    };
    let h = ReconcileBatchHdr {
        report_id: 7,
        n_reports: 2,
        n_fills: 1,
        venue: summary.venue,
        account: summary.account,
        _pad: 0,
    };
    let body = [
        h.as_bytes(),
        summary.as_bytes(),
        position.as_bytes(),
        e.as_bytes(),
    ]
    .concat();
    let b = ReconcileBatchView::read(&body).unwrap();
    assert_eq!(b.reports().collect::<Vec<_>>(), vec![summary, position]);
    assert_eq!(b.fills().collect::<Vec<_>>(), vec![e]);
    let frame = CtlRecord {
        kind: EventKind::RECONCILE_BATCH,
        flags: RecordFlags(0),
        body: &body,
    };
    let mut framed = vec![0; frame.encoded_len()];
    frame.encode_into(&mut framed).unwrap();
    let read = CtlRecord::read(&framed).unwrap();
    assert_eq!(read.kind, frame.kind);
    assert_eq!(read.body, frame.body);
    for at in [
        8,
        10,
        14,
        16,
        16 + 18,
        16 + 20,
        16 + 104 + 9,
        16 + 104 + 10,
        16 + 208 + 2,
    ] {
        let mut bad = body.clone();
        bad[at] ^= 255;
        assert!(ReconcileBatchView::read(&bad).is_err(), "offset {at}");
    }
    for n in 0..body.len() {
        assert!(ReconcileBatchView::read(&body[..n]).is_err());
    }
    let mut longer = body.clone();
    longer.push(0);
    assert!(ReconcileBatchView::read(&longer).is_err());
    let mut encoded = vec![0; MAX_RECORD];
    let n = encode_var(
        &mut encoded,
        1,
        2,
        RecordFlags::EVENT_END | RecordFlags::CRC,
        &h,
        &body[16..],
    )
    .unwrap();
    let rec = decode_record(&encoded[..n as usize]).unwrap();
    assert_eq!(
        classify(EventKind::RECONCILE_BATCH, rec.body).unwrap(),
        Role::Input
    );
    assert_eq!(rec.exact_body().unwrap(), body);
}
