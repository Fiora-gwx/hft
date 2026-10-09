//! Python side of `ledger(j, account, t0, t1)` and `pnl(j, by)`: the views of a `ledger::Run` as
//! Arrow record batches (`hft_py.RecordBatch`).

use std::sync::Arc;

use hft_ledger::PnlBreakdown;
use hft_types::{AccountId, GROUP_NONE};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyString;

use crate::cols::{Batch, ColType, Column, FieldSpec, SchemaSpec};
use crate::error::Error;
use crate::ledger::{self, By, Run, account_kind, account_venues};
use crate::py_journal::{PyBatch, PyJournal, int_arg};

fn col_type(c: &Column) -> ColType {
    match c {
        Column::U8(_) => ColType::U8,
        Column::U16(_) => ColType::U16,
        Column::U32(_) => ColType::U32,
        Column::U64(_) => ColType::U64,
        Column::I8(_) => ColType::I8,
        Column::I16(_) => ColType::I16,
        Column::I32(_) => ColType::I32,
        Column::I64(_) => ColType::I64,
        Column::F64(..) => ColType::F64,
        Column::Bool(_) => ColType::Bool,
        Column::Str(_) => ColType::Utf8,
        Column::Fixed(w, _) => ColType::FixedBinary(*w),
        Column::Bin(..) => ColType::LargeBinary,
        Column::Dec(_) => ColType::Decimal128,
        Column::List(..) => ColType::ListOfStruct(Vec::new()),
    }
}

fn batch(cols: Vec<(&str, Column)>) -> PyResult<PyBatch> {
    let rows = cols.first().map_or(0, |(_, c)| match c {
        Column::Fixed(w, v) => v.len() / (*w).max(1),
        Column::Bin(o, _) => o.len().saturating_sub(1),
        Column::List(o, _) => o.len().saturating_sub(1),
        Column::U8(v) => v.len(),
        Column::U16(v) => v.len(),
        Column::U32(v) => v.len(),
        Column::U64(v) => v.len(),
        Column::I8(v) => v.len(),
        Column::I16(v) => v.len(),
        Column::I32(v) => v.len(),
        Column::I64(v) => v.len(),
        Column::F64(v, _) => v.len(),
        Column::Bool(v) => v.len(),
        Column::Str(v) => v.len(),
        Column::Dec(v) => v.len(),
    });
    let fields = cols
        .iter()
        .map(|(n, c)| {
            let f = FieldSpec::new(n, col_type(c));
            if matches!(c, Column::Str(_)) {
                f.nullable()
            } else {
                f
            }
        })
        .collect();
    let b = Batch {
        schema: Arc::new(SchemaSpec {
            fields,
            metadata: Vec::new(),
        }),
        rows,
        cols: cols.into_iter().map(|(_, c)| c).collect(),
    };
    Ok(PyBatch::new(b))
}

fn i64s(v: Vec<i64>) -> Column {
    Column::I64(v)
}

fn dec(v: Vec<i128>) -> PyResult<Column> {
    Ok(Column::Dec(v))
}

fn kinds(rd: &hft_refdata::RefData, accounts: &[u8]) -> Column {
    Column::Str(
        accounts
            .iter()
            .map(|a| account_kind(rd, *a).map(str::to_string))
            .collect(),
    )
}

const PNL_COLS: [&str; 8] = [
    "price_realized",
    "funding",
    "fee_cex",
    "gas",
    "bribe",
    "unrealized",
    "adjust",
    "external",
];

fn pnl_cols(rows: &[PnlBreakdown]) -> Vec<(&'static str, Column)> {
    let get: [fn(&PnlBreakdown) -> i64; 8] = [
        |p| p.price_realized.0,
        |p| p.funding.0,
        |p| p.fee_cex.0,
        |p| p.gas.0,
        |p| p.bribe.0,
        |p| p.unrealized.0,
        |p| p.adjust.0,
        |p| p.external.0,
    ];
    PNL_COLS
        .iter()
        .zip(get)
        .map(|(n, f)| (*n, i64s(rows.iter().map(f).collect())))
        .collect()
}

/// Cold ledger of one account over `[t0, t1)` (README "2. ledger / pnl").
#[pyclass(name = "Ledger", module = "hft_py", frozen)]
pub struct PyLedger {
    run: Run,
    account: u8,
}

impl std::fmt::Debug for PyLedger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ledger")
            .field("account", &self.account)
            .finish_non_exhaustive()
    }
}

#[pymethods]
impl PyLedger {
    /// Non-zero positions at `t1`: `venue`, `inst`, `qty`, `cost`, `realized_quote`, `fees_quote`,
    /// `last_fill_ens`, `account`, `account_kind`.
    fn positions(&self) -> PyResult<PyBatch> {
        let rows = ledger::positions(&self.run, self.account);
        let acc: Vec<u8> = rows.iter().map(|_| self.account).collect();
        batch(vec![
            (
                "venue",
                Column::U8(rows.iter().map(|r| r.0.0).collect::<Vec<_>>()),
            ),
            ("inst", Column::U32(rows.iter().map(|r| r.1).collect::<Vec<_>>())),
            ("qty", i64s(rows.iter().map(|r| r.2.qty.0).collect())),
            ("cost", dec(rows.iter().map(|r| r.2.cost).collect())?),
            (
                "realized_quote",
                dec(rows.iter().map(|r| r.2.realized_quote).collect())?,
            ),
            ("fees_quote", i64s(rows.iter().map(|r| r.2.fees_quote).collect())),
            (
                "last_fill_ens",
                i64s(rows.iter().map(|r| r.2.last_fill_ens).collect()),
            ),
            ("account", Column::U8(acc.clone())),
            ("account_kind", kinds(&self.run.rd, &acc)),
        ])
    }

    /// Spot inventory at `t1` per venue, asset and group (`unassigned` = true for the part not
    /// assigned to any group): `venue`, `asset`, `group`, `unassigned`, `qty`, `account`,
    /// `account_kind`.
    fn inventory(&self) -> PyResult<PyBatch> {
        let a = AccountId(self.account);
        let (mut v, mut asset, mut group, mut un, mut qty) = (vec![], vec![], vec![], vec![], vec![]);
        for ven in account_venues(&self.run.rd, self.account) {
            let book = self.run.jl.ledger().venue_book(a, ven);
            for e in book.inv.iter().filter(|e| e.qty.0 != 0) {
                v.push(ven.0);
                asset.push(e.asset.0);
                group.push(e.group.0);
                un.push(false);
                qty.push(e.qty.0);
            }
            for e in book.cash.iter().filter(|e| e.unassigned.0 != 0) {
                v.push(ven.0);
                asset.push(e.asset.0);
                group.push(GROUP_NONE.0);
                un.push(true);
                qty.push(e.unassigned.0);
            }
        }
        let acc: Vec<u8> = v.iter().map(|_| self.account).collect();
        batch(vec![
            ("venue", Column::U8(v)),
            ("asset", Column::U16(asset)),
            ("group", Column::U32(group)),
            ("unassigned", Column::Bool(un)),
            ("qty", i64s(qty)),
            ("account", Column::U8(acc.clone())),
            ("account_kind", kinds(&self.run.rd, &acc)),
        ])
    }

    /// Per-venue equity view data at `t1` (`VenueEquityView`): `venue`, `flags`, `equity`,
    /// `quote_cash`, `spot_value`, `perp_upnl`, `margin_balance`, `as_of_ens`, `account`,
    /// `account_kind`.
    fn portfolio(&self) -> PyResult<PyBatch> {
        let a = AccountId(self.account);
        let views: Vec<_> = account_venues(&self.run.rd, self.account)
            .into_iter()
            .map(|v| self.run.jl.ledger().venue_book(a, v).summary)
            .collect();
        let acc: Vec<u8> = views.iter().map(|_| self.account).collect();
        batch(vec![
            (
                "venue",
                Column::U8(views.iter().map(|x| x.venue.0).collect::<Vec<_>>()),
            ),
            (
                "flags",
                Column::U16(views.iter().map(|x| x.flags).collect::<Vec<_>>()),
            ),
            ("equity", i64s(views.iter().map(|x| x.equity.0).collect())),
            ("quote_cash", i64s(views.iter().map(|x| x.quote_cash.0).collect())),
            ("spot_value", i64s(views.iter().map(|x| x.spot_value.0).collect())),
            ("perp_upnl", i64s(views.iter().map(|x| x.perp_upnl.0).collect())),
            (
                "margin_balance",
                i64s(views.iter().map(|x| x.margin_balance.0).collect()),
            ),
            ("as_of_ens", i64s(views.iter().map(|x| x.as_of_ens).collect())),
            ("account", Column::U8(acc.clone())),
            ("account_kind", kinds(&self.run.rd, &acc)),
        ])
    }

    /// Σ `PnlDelta` booked in `[t0, t1)` (`PnlBreakdown` columns, USDT 1e-6 raw), one row.
    fn breakdown(&self) -> PyResult<PyBatch> {
        let s = ledger::breakdown(&self.run, self.account);
        let mut cols = pnl_cols(&[s]);
        cols.push(("account", Column::U8(vec![self.account])));
        cols.push(("account_kind", kinds(&self.run.rd, &[self.account])));
        batch(cols)
    }
}

impl PyLedger {
    /// Final ledger sections (testing hook).
    pub fn snapshot(&self) -> Vec<u8> {
        ledger::snapshot_bytes(&self.run)
    }
}

/// `ledger(j, account, t0=None, t1=None)`.
#[pyfunction]
#[pyo3(name = "ledger", signature = (j, account, t0=None, t1=None))]
pub fn ledger_py(
    py: Python<'_>,
    j: PyRef<'_, PyJournal>,
    account: &Bound<'_, PyAny>,
    t0: Option<&Bound<'_, PyAny>>,
    t1: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyLedger> {
    let (label, id) = match account.cast::<PyString>() {
        Ok(s) => (Some(s.to_string()), None),
        Err(_) => (None, Some(int_arg(account, "account")?)),
    };
    let a = j.inner.account_id(label.as_deref(), id)?;
    let t0 = crate::py_journal::time_arg(t0, "t0")?;
    let t1 = crate::py_journal::time_arg(t1, "t1")?;
    if let (Some(x), Some(y)) = (t0, t1)
        && x > y
    {
        return Err(Error::value(format!("t0 {x} > t1 {y}")).into());
    }
    let jj = j.inner.clone();
    let run = py.detach(move || ledger::run(&jj, Some(vec![a]), t0, t1))?;
    Ok(PyLedger { run, account: a })
}

/// `pnl(j, by)`: one row per key over the whole journal and every account.
#[pyfunction]
#[pyo3(name = "pnl", signature = (j, by))]
pub fn pnl_py(py: Python<'_>, j: PyRef<'_, PyJournal>, by: &Bound<'_, PyAny>) -> PyResult<PyBatch> {
    let by: String = by
        .cast::<PyString>()
        .map(|s| s.to_string())
        .map_err(|_| PyValueError::new_err("by must be one of \"group\", \"account\", \"round\", \"day\""))?;
    let by = By::parse(&by)?;
    let jj = j.inner.clone();
    let run = py.detach(move || ledger::run(&jj, None, None, None))?;
    let rows = ledger::pnl_rows(&run, by);
    let acc: Vec<u8> = rows.iter().map(|r| r.0.0).collect();
    let mut cols: Vec<(&str, Column)> = vec![("account", Column::U8(acc.clone()))];
    cols.push(("account_kind", kinds(&run.rd, &acc)));
    match by {
        By::Group => cols.push((
            "group",
            Column::U32(rows.iter().map(|r| r.0.1).collect::<Vec<_>>()),
        )),
        By::Round => {
            cols.push((
                "group",
                Column::U32(rows.iter().map(|r| r.0.1).collect::<Vec<_>>()),
            ));
            cols.push((
                "round",
                Column::U32(rows.iter().map(|r| r.0.2).collect::<Vec<_>>()),
            ));
        }
        By::Day => {
            let days: Vec<String> = rows
                .iter()
                .map(|r| {
                    let d = crate::journal::utc_yyyymmdd(r.0.3.saturating_mul(86_400_000_000_000));
                    format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..])
                })
                .collect();
            cols.push(("day", Column::Str(days.into_iter().map(Some).collect())));
        }
        By::Account => {}
    }
    let sums: Vec<PnlBreakdown> = rows.iter().map(|r| r.1).collect();
    cols.extend(pnl_cols(&sums));
    batch(cols)
}
