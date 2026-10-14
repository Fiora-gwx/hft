//! Typed fixture adapter for the shared matching core. Parsing fixture files and constructing
//! refdata/setup belong to the caller; every OrderEvent field, including timing, is compared.
use hft_event::{EventKind, GwCmd, OrderEvent, RecordHeader, RepMsg, kind_info};
use hft_sim::{SimExchange, SimInput};
use hft_types::Tsc;
use zerocopy::IntoBytes;

#[derive(Debug)]
pub enum SimStep<'a> {
    Record(&'a RecordHeader, SimInput<'a>),
    Command(Tsc, &'a GwCmd),
    Advance(Tsc),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoldenDiff {
    /// Zero-based reply index; for malformed ordering this is the input step index.
    pub event: usize,
    pub field: String,
    pub expected: Vec<u8>,
    pub actual: Vec<u8>,
}

/// Run an ordered market/command script through SimExchange and compare its complete order
/// report stream. Other reports (balances, margins, startup status) are outside this stream.
/// A final Advance step must reach the response deadline; missing/extra replies are differences.
pub fn run_sim_case(
    exchange: &mut SimExchange,
    steps: &[SimStep<'_>],
    expected: &[OrderEvent],
) -> Result<(), GoldenDiff> {
    let mut actual = Vec::new();
    let mut previous = 0;
    for (index, step) in steps.iter().enumerate() {
        let at = match step {
            SimStep::Record(header, _) => header.tsc,
            SimStep::Command(t, _) | SimStep::Advance(t) => t.0,
        };
        if at < previous {
            return Err(GoldenDiff {
                event: index,
                field: "script.tsc".into(),
                expected: previous.to_le_bytes().to_vec(),
                actual: at.to_le_bytes().to_vec(),
            });
        }
        previous = at;
        match step {
            SimStep::Record(header, input) => exchange.on_record(header, *input),
            SimStep::Command(t, cmd) => {
                exchange.advance(*t);
                exchange.on_command(cmd, *t);
            }
            SimStep::Advance(t) => exchange.advance(*t),
        }
        exchange.drain_due(Tsc(at), |reply| {
            if let RepMsg::OrderEvent(event) = reply.msg {
                actual.push(event);
            }
        });
    }
    for (index, (want, got)) in expected.iter().zip(&actual).enumerate() {
        if want.as_bytes() == got.as_bytes() {
            continue;
        }
        if let Some(info) = kind_info(EventKind::ORDER_EVENT) {
            for field in info.fields {
                let start = usize::from(field.offset);
                let end = start + usize::from(field.ty.width());
                let (a, b) = (&want.as_bytes()[start..end], &got.as_bytes()[start..end]);
                if a != b {
                    return Err(GoldenDiff {
                        event: index,
                        field: field.name.into(),
                        expected: a.to_vec(),
                        actual: b.to_vec(),
                    });
                }
            }
        }
        return Err(GoldenDiff {
            event: index,
            field: "body".into(),
            expected: want.as_bytes().to_vec(),
            actual: got.as_bytes().to_vec(),
        });
    }
    if actual.len() != expected.len() {
        return Err(GoldenDiff {
            event: actual.len().min(expected.len()),
            field: "reply_count".into(),
            expected: (expected.len() as u64).to_le_bytes().to_vec(),
            actual: (actual.len() as u64).to_le_bytes().to_vec(),
        });
    }
    Ok(())
}
