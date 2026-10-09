//! BIN-200–BIN-202: subscription frames and stream names.
#![allow(clippy::disallowed_types)] // serde_json::Value: reference parser of the frames under test

use crate::support;

use hft_types::InstId;
use hft_venue_proto::{ProtoErr, RouteId, StreamKind, SubFrames, SubSpec, VenueProtocol};
use support::{ACCT, S_BTC, U_BTC, cfg, rd};
use venue_binance::{Spot, Usdm};

fn spec(inst: InstId, stream: StreamKind, levels: u8) -> SubSpec {
    SubSpec {
        inst,
        stream,
        levels,
        route: RouteId(1),
    }
}

fn frames<P: VenueProtocol>(subs: &[SubSpec]) -> Result<Vec<(String, u16)>, ProtoErr> {
    let mut out = SubFrames::with_capacity(64 * 1024, 16);
    P::subscribe(rd(), subs, false, &cfg(1500), &mut out)?;
    Ok((0..out.len())
        .map(|i| {
            let (t, n) = out.get(i).unwrap();
            (String::from_utf8(t.to_vec()).unwrap(), n)
        })
        .collect())
}

fn params(f: &str) -> Vec<String> {
    let v: serde_json::Value = serde_json::from_str(f).unwrap();
    v["params"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn bin_200_subscribe_batching() {
    // 450 USDT-M insts of RD-BIN (fillers 1..=450).
    let subs: Vec<SubSpec> = (1..=450u32)
        .map(|i| spec(InstId(i), StreamKind::Bbo, 0))
        .collect();
    let f = frames::<Usdm>(&[spec(U_BTC, StreamKind::Bbo, 0)]).unwrap();
    assert_eq!(
        f,
        vec![(
            r#"{"method":"SUBSCRIBE","params":["btcusdt@bookTicker"],"id":1}"#.to_string(),
            1
        )],
        "BIN-200.1"
    );
    for (row, k, want) in [
        ("BIN-200.2", 200usize, vec![200usize]),
        ("BIN-200.3", 201, vec![200, 1]),
        ("BIN-200.4", 450, vec![200, 200, 50]),
    ] {
        let f = frames::<Usdm>(&subs[..k]).unwrap();
        let sizes: Vec<usize> = f.iter().map(|(t, _)| params(t).len()).collect();
        assert_eq!(sizes, want, "{row}");
        for (i, (t, n)) in f.iter().enumerate() {
            let v: serde_json::Value = serde_json::from_str(t).unwrap();
            assert_eq!(v["id"], (i + 1) as u64, "{row}");
            assert_eq!(usize::from(*n), want[i], "{row}");
            assert!(!t.contains("SET_PROPERTY"), "BIN-200.6");
        }
    }
    assert!(frames::<Usdm>(&[]).unwrap().is_empty(), "BIN-200.5");
}

fn names<P: VenueProtocol>(subs: &[SubSpec]) -> Result<Vec<String>, ProtoErr> {
    Ok(frames::<P>(subs)?.iter().flat_map(|(t, _)| params(t)).collect())
}

#[test]
fn bin_201_stream_names() {
    let u = |k, l| names::<Usdm>(&[spec(U_BTC, k, l)]);
    assert_eq!(
        u(StreamKind::Bbo, 0).unwrap(),
        ["btcusdt@bookTicker"],
        "BIN-201.1"
    );
    assert_eq!(
        u(StreamKind::DepthTopN, 5).unwrap(),
        ["btcusdt@depth5@100ms"],
        "BIN-201.2"
    );
    assert_eq!(
        u(StreamKind::DepthTopN, 20).unwrap(),
        ["btcusdt@depth20@100ms"],
        "BIN-201.3"
    );
    assert_eq!(
        u(StreamKind::DepthTopN, 15),
        Err(ProtoErr::Unsupported),
        "BIN-201.4"
    );
    assert_eq!(
        u(StreamKind::Trade, 0).unwrap(),
        ["btcusdt@aggTrade"],
        "BIN-201.5"
    );
    let mif = names::<Usdm>(&[
        spec(U_BTC, StreamKind::Mark, 0),
        spec(U_BTC, StreamKind::Index, 0),
        spec(U_BTC, StreamKind::Funding, 0),
    ])
    .unwrap();
    assert_eq!(mif, ["btcusdt@markPrice@1s"], "BIN-201.6");
    assert_eq!(
        u(StreamKind::Liquidation, 0).unwrap(),
        ["btcusdt@forceOrder"],
        "BIN-201.7"
    );
    let s = |k, l| names::<Spot>(&[spec(S_BTC, k, l)]);
    assert_eq!(
        s(StreamKind::Bbo, 0).unwrap(),
        ["btcusdt@bookTicker"],
        "BIN-201.8"
    );
    assert_eq!(
        s(StreamKind::DepthTopN, 10).unwrap(),
        ["btcusdt@depth10@100ms"],
        "BIN-201.9"
    );
    assert_eq!(s(StreamKind::Trade, 0).unwrap(), ["btcusdt@trade"], "BIN-201.10");
    assert_eq!(
        s(StreamKind::Funding, 0),
        Err(ProtoErr::Unsupported),
        "BIN-201.11"
    );
    // BIN-201.12: no stream name carries rpiDepth, whatever the combination.
    for k in [
        StreamKind::Bbo,
        StreamKind::DepthTopN,
        StreamKind::DepthIncr,
        StreamKind::Trade,
        StreamKind::Mark,
        StreamKind::Index,
        StreamKind::Funding,
        StreamKind::Liquidation,
    ] {
        for l in [0u8, 5, 10, 20] {
            for n in u(k, l).into_iter().chain(s(k, l)).flatten() {
                assert!(!n.contains("rpiDepth"), "BIN-201.12");
            }
        }
    }
}

#[test]
fn bin_202_private_subscribe() {
    let run = |spot: bool, unsub: bool| {
        let mut out = SubFrames::with_capacity(4096, 4);
        if spot {
            Spot::private_subscribe(&cfg(1500), ACCT, unsub, &mut out).unwrap();
        } else {
            Usdm::private_subscribe(&cfg(1500), ACCT, unsub, &mut out).unwrap();
        }
        (0..out.len())
            .map(|i| String::from_utf8(out.get(i).unwrap().0.to_vec()).unwrap())
            .collect::<Vec<_>>()
    };
    assert!(run(false, false).is_empty(), "BIN-202.1");
    assert_eq!(
        run(true, false),
        [r#"{"id":"u0000000000000000","method":"userDataStream.subscribe"}"#],
        "BIN-202.2"
    );
    assert_eq!(
        run(true, true),
        [r#"{"id":"u0000000000000000","method":"userDataStream.unsubscribe"}"#],
        "BIN-202.3"
    );
}
