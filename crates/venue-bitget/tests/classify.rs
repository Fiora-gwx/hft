use hft_venue_proto::{MsgOp, VenueProtocol};
use proptest::prelude::*;
use venue_bitget::{BitgetSpot, BitgetUsdtFutures as Fut};
proptest! {
    #![proptest_config(ProptestConfig::with_cases(5000))]
    #[test]
    fn bgt_012_classify_reads_64_bytes(prefix in any::<[u8;64]>(), a in prop::collection::vec(any::<u8>(),0..4097), b in prop::collection::vec(any::<u8>(),0..4097), channel in 0..7usize) {
        let channels=["ticker","books1","books5","books","trade","order",""];
        let mut p=prefix;
        if channel<6 {let s=format!("{{\"arg\":{{\"channel\":\"{}\"}},",channels[channel]);p[..s.len()].copy_from_slice(s.as_bytes());}
        let x=[&p[..],&a].concat();let y=[&p[..],&b].concat();
        for op in [MsgOp::Text,MsgOp::Binary] {
            for r in Fut::routes() {prop_assert_eq!(Fut::classify(r.id,op,&x),Fut::classify(r.id,op,&p));prop_assert_eq!(Fut::classify(r.id,op,&x),Fut::classify(r.id,op,&y));}
            for r in BitgetSpot::routes() {prop_assert_eq!(BitgetSpot::classify(r.id,op,&x),BitgetSpot::classify(r.id,op,&p));prop_assert_eq!(BitgetSpot::classify(r.id,op,&x),BitgetSpot::classify(r.id,op,&y));}
            prop_assert_eq!(Fut::classify_report(op,&x),Fut::classify_report(op,&y));
            prop_assert_eq!(BitgetSpot::classify_report(op,&x),BitgetSpot::classify_report(op,&y));
        }
    }
}
