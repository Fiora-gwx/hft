//! Gate ⑦ minor-change check (`minor_change_check`).

use hft_types::{GROUP_NONE, GroupId};

use super::{MinorChange, RawParams, SchemaView};

/// First major change found: `group = GROUP_NONE` for a default value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MajorChange {
    pub field_ix: u16,
    pub group: GroupId,
}

/// Whether `old → new` is a minor change under rule `m`.
fn is_minor(m: MinorChange, old: i64, new: i64) -> bool {
    let d = (i128::from(new) - i128::from(old)).abs();
    match m {
        MinorChange::None => d == 0,
        MinorChange::Any => true,
        MinorChange::Abs(v) => d <= i128::from(v),
        MinorChange::RelPct(n) => d == 0 || (old != 0 && d * 100 <= i128::from(old).abs() * i128::from(n)),
    }
}

/// Effective value of field `fx` for the group at `gix` (override, else the default).
fn effective(raw: &RawParams, gix: usize, fx: u16, default: i64) -> i64 {
    raw.overrides
        .iter()
        .rev()
        .find(|o| usize::from(o.group_ix) == gix && o.field_ix == fx)
        .map_or(default, |o| o.value)
}

/// Compares the baseline (the version in force when gate ⑦'s timer started) with the current
/// parameters field by field: defaults with defaults, and for every `GroupId` present in both
/// versions (aligned by id, not gix) the effective values; groups present in only one version
/// are not compared. All minor → `Ok`; otherwise the first major change by `field_ix`, then
/// defaults before groups, then ascending `GroupId`. Pure, cold path.
pub fn minor_change_check<S: SchemaView + ?Sized>(
    s: &S,
    base: (&RawParams, &[GroupId]),
    cur: (&RawParams, &[GroupId]),
) -> Result<(), MajorChange> {
    let (braw, bgroups) = base;
    let (craw, cgroups) = cur;
    // Groups in both versions, ascending by GroupId, with their gix on each side.
    let mut common: Vec<(GroupId, usize, usize)> = bgroups
        .iter()
        .enumerate()
        .filter_map(|(bi, g)| cgroups.iter().position(|c| c == g).map(|ci| (*g, bi, ci)))
        .collect();
    common.sort_by_key(|(g, _, _)| g.0);
    for fx in 0..s.n_fields() {
        let Some(f) = s.field(fx) else { break };
        let bd = braw.defaults.get(usize::from(fx)).copied().unwrap_or(f.default);
        let cd = craw.defaults.get(usize::from(fx)).copied().unwrap_or(f.default);
        if !is_minor(f.minor_change, bd, cd) {
            return Err(MajorChange {
                field_ix: fx,
                group: GROUP_NONE,
            });
        }
        for &(g, bi, ci) in &common {
            let bv = effective(braw, bi, fx, bd);
            let cv = effective(craw, ci, fx, cd);
            if !is_minor(f.minor_change, bv, cv) {
                return Err(MajorChange {
                    field_ix: fx,
                    group: g,
                });
            }
        }
    }
    Ok(())
}
