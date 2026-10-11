//! Offline acceptance of a recorded recovery's old tail. A Recovery segment's normal
//! trading is never an accepted range; only validated pre-EngineClock Notes identify one.

use hft_event::{BlobKind, EventKind, Note, NoteLevel, NoteSource, Role, SnapshotHdr};
use hft_refdata::RefData;
use hft_types::{AccountId, VenueId};

use super::replay::{Log, Mismatch};
use super::verify::VerifyError;

#[derive(Clone, Copy, Debug)]
struct Anchor {
    venue: VenueId,
    account: AccountId,
    first: u64,
}

#[derive(Clone, Debug)]
pub(super) struct RecoveryRange {
    from: u64,
    to: u64,
    pairs: Vec<(VenueId, AccountId)>,
    anchors: Vec<Anchor>,
}

fn scoped(kind: EventKind, venue: VenueId, account: AccountId, pair: (VenueId, AccountId)) -> bool {
    if matches!(
        kind,
        EventKind::ORDER_DECISION | EventKind::EXEC_DECISION | EventKind::FUNDING_SETTLE
    ) && account.0 != 0
    {
        account == pair.1 && (venue.0 == 0 || venue == pair.0)
    } else {
        true
    }
}

impl RecoveryRange {
    fn known_scope(&self, kind: EventKind, venue: VenueId, account: AccountId) -> bool {
        !matches!(
            kind,
            EventKind::ORDER_DECISION | EventKind::EXEC_DECISION | EventKind::FUNDING_SETTLE
        ) || account.0 == 0
            || self
                .pairs
                .iter()
                .any(|&(v, a)| a == account && (venue.0 == 0 || v == venue))
    }
    pub fn accepts(&self, log: &Log, m: &Mismatch) -> bool {
        if m.seq < self.from || m.seq > self.to {
            return false;
        }
        let Ok(ix) = log.recs.binary_search_by_key(&m.seq, |r| r.seq) else {
            return false;
        };
        let r = &log.recs[ix];
        // An extra generated Output cannot authorize consuming an Input or crossing the tail.
        if r.role() != Role::Output {
            return false;
        }
        let (venue, account) = super::replay::venue_account(r.kind, &r.body);
        if !self.known_scope(m.kind, m.venue, m.account) || !self.known_scope(r.kind, venue, account) {
            return false;
        }
        let mut affected = false;
        for &pair in &self.pairs {
            if scoped(m.kind, m.venue, m.account, pair) || scoped(r.kind, venue, account, pair) {
                affected = true;
                if !self
                    .anchors
                    .iter()
                    .any(|a| (a.venue, a.account) == pair && a.first <= m.seq)
                {
                    return false;
                }
            }
        }
        affected
    }
}

/// `first` starts the recovery segment; `notes` are its complete, pre-EngineClock Notes.
/// The source is the previous run's latest snapshot, exactly as normal live recovery selects.
pub(super) fn range(log: &Log, first: usize, notes: &[usize]) -> Result<RecoveryRange, VerifyError> {
    let bad = |ix: usize| VerifyError::RecoveryRange {
        seq: log.recs.get(ix).map_or(0, |r| r.seq),
    };
    let snap = log.recs[..first]
        .iter()
        .rposition(|r| r.kind == EventKind::STATE_SNAPSHOT)
        .ok_or_else(|| bad(first))?;
    let end = log.recs[..first]
        .iter()
        .rposition(|r| r.flags.contains(hft_event::RecordFlags::EVENT_END))
        .ok_or_else(|| bad(first))?;
    if end <= snap {
        return Err(bad(first));
    }
    let bytes = log.recs[snap]
        .body
        .get(size_of::<SnapshotHdr>()..)
        .ok_or_else(|| bad(snap))?;
    let sections = crate::snapshot::read_seq_sections(bytes).map_err(|_| bad(snap))?;
    let refs: Vec<_> = sections
        .active
        .iter()
        .filter(|(b, _)| b.kind == BlobKind::Refdata)
        .collect();
    if refs.len() != 1 {
        return Err(bad(snap));
    }
    let content = crate::config_blob::content_before(log, snap, &refs[0].0).map_err(|_| bad(snap))?;
    let rd = RefData::load(content, None).map_err(|_| bad(snap))?;
    let mut pairs = Vec::new();
    for a in 1..=crate::engine::account_count(&rd) {
        let account = AccountId(a as u8);
        for v in 0..64u8 {
            if rd.account(account).venues & (1u64 << v) != 0 {
                pairs.push((VenueId(v), account));
            }
        }
    }
    let mut anchors: Vec<Anchor> = Vec::new();
    for &ix in notes {
        let n = log.recs[ix].read::<Note>().ok_or_else(|| bad(ix))?;
        if n.level != NoteLevel::P0
            || n.source != NoteSource::Sequencer
            || n.strategy.0 != 0
            || n._pad != 0
            || n.group.0 != 0
            || n.inst.0 != 0
            || n.v[0] >> 16 != 0
            || n.v[2..].iter().any(|v| *v != 0)
            || log.recs[ix].body.len() != size_of::<Note>()
            || log.recs[ix].flags.0 & !crate::journal::NONCMP_FLAGS != 0
        {
            return Err(bad(ix));
        }
        let venue = VenueId(n.v[0] as u8);
        let account = AccountId((n.v[0] >> 8) as u8);
        if !pairs.contains(&(venue, account))
            || anchors.iter().any(|a| a.venue == venue && a.account == account)
            || n.v[1] <= log.recs[snap].seq
            || n.v[1] > log.recs[end].seq
        {
            return Err(bad(ix));
        }
        let target = log
            .recs
            .binary_search_by_key(&n.v[1], |r| r.seq)
            .map_err(|_| bad(ix))?;
        let r = &log.recs[target];
        let (v, a) = super::replay::venue_account(r.kind, &r.body);
        if r.role() != Role::Output || !scoped(r.kind, v, a, (venue, account)) {
            return Err(bad(ix));
        }
        anchors.push(Anchor {
            venue,
            account,
            first: n.v[1],
        });
    }
    // A global mismatch requires evidence for every affected pair. An earlier anchor for a
    // pair is valid because the live recorder retains only its first mismatch.
    for &ix in notes {
        let n = log.recs[ix].read::<Note>().ok_or_else(|| bad(ix))?;
        let target = log
            .recs
            .binary_search_by_key(&n.v[1], |r| r.seq)
            .map_err(|_| bad(ix))?;
        let r = &log.recs[target];
        let (v, a) = super::replay::venue_account(r.kind, &r.body);
        if pairs.iter().any(|&pair| {
            scoped(r.kind, v, a, pair)
                && !anchors
                    .iter()
                    .any(|p| (p.venue, p.account) == pair && p.first <= n.v[1])
        }) {
            return Err(bad(ix));
        }
    }
    Ok(RecoveryRange {
        from: log.recs[snap].seq + 1,
        to: log.recs[end].seq,
        pairs,
        anchors,
    })
}
