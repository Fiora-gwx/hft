//! kind_id ledger (TESTS.md REG-004): the README's "kind_id 登记" table against `Strategies::kinds()`.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

use hft_strategy_registry::Strategies;
use hft_strategy_sdk::{StrategyDesc, StrategySet};

const README: &str = include_str!("../README.md");

/// One ledger row: kind_id, crate column text (link text), retired, `non_prod_only` in the scope column.
#[derive(Debug)]
struct Row {
    kind: u16,
    name: String,
    retired: bool,
    non_prod_only: bool,
}

/// Rows of the ledger table (the test-strategy range row is skipped).
fn ledger(text: &str) -> Result<Vec<Row>, String> {
    let start = text.find("**kind_id 登记**").ok_or("no ledger heading")?;
    let mut rows = Vec::new();
    let mut in_table = false;
    for l in text[start..].lines().skip(1) {
        let l = l.trim();
        if !l.starts_with('|') {
            if in_table {
                break;
            }
            continue;
        }
        in_table = true;
        let cells: Vec<&str> = l.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() < 3 || cells[0] == "kind_id" || cells[0].starts_with("---") {
            continue;
        }
        if cells[0].starts_with("0xFF00") {
            continue;
        }
        let kind: u16 = cells[0]
            .parse()
            .map_err(|_| format!("bad kind_id cell {:?}", cells[0]))?;
        let c = cells[1];
        let retired = c.contains("退役");
        let name = match (c.find('['), c.find(']')) {
            (Some(a), Some(b)) if a < b => c[a + 1..b].to_string(),
            _ => c.trim_start_matches("退役:").trim().to_string(),
        };
        rows.push(Row {
            kind,
            name,
            retired,
            non_prod_only: cells[2].contains("non_prod_only"),
        });
    }
    Ok(rows)
}

/// The ledger check of README "实现说明".
fn check_ledger(text: &str, kinds: &[StrategyDesc]) -> Result<(), String> {
    let rows = ledger(text)?;
    let mut ids: Vec<u16> = rows.iter().map(|r| r.kind).collect();
    ids.sort_unstable();
    for (i, w) in ids.windows(2).enumerate() {
        if w[0] == w[1] {
            return Err(format!("duplicate kind_id {}", w[0]));
        }
        let _ = i;
    }
    for (i, id) in ids.iter().enumerate() {
        if usize::from(*id) != i + 1 {
            return Err(format!("kind_ids not consecutive from 1: missing {}", i + 1));
        }
    }
    for d in kinds {
        let Some(r) = rows.iter().find(|r| r.kind == d.kind_id) else {
            return Err(format!("kind_id {} of {} not in the ledger", d.kind_id, d.name));
        };
        if r.name != d.name {
            return Err(format!(
                "kind_id {}: ledger name {} ≠ {}",
                d.kind_id, r.name, d.name
            ));
        }
        if r.retired {
            return Err(format!("kind_id {} is compiled in but marked retired", d.kind_id));
        }
        if r.non_prod_only != d.non_prod_only {
            return Err(format!("kind_id {}: non_prod_only differs", d.kind_id));
        }
    }
    Ok(())
}

/// The README with the ledger line starting with `| <kind> |` edited by `f` (`None` deletes it).
fn edit_row(kind: &str, f: impl Fn(&str) -> Option<String>) -> String {
    let mut out = Vec::new();
    for l in README.lines() {
        if l.starts_with(&format!("| {kind} |")) {
            if let Some(n) = f(l) {
                out.push(n);
            }
        } else {
            out.push(l.to_string());
        }
    }
    out.join("\n")
}

fn append_row(line: &str) -> String {
    edit_row("5", |l| Some(format!("{l}\n{line}")))
}

#[test]
fn reg_004_ledger() {
    let k = Strategies::kinds();
    let rows: Vec<(&str, String, bool)> = vec![
        ("REG-004.1", README.to_string(), true),
        ("REG-004.2", edit_row("4", |_| None), false),
        (
            "REG-004.3",
            append_row("| 5 | [x](../x/README.md) | core | 默认 |"),
            false,
        ),
        ("REG-004.4", edit_row("5", |_| None), false),
        (
            "REG-004.5",
            edit_row("5", |l| {
                Some(l.replacen("[latency-probe]", "[latency-probe2]", 1))
            }),
            false,
        ),
        (
            "REG-004.6",
            edit_row("5", |l| {
                Some(l.replacen(
                    "[latency-probe](../latency-probe/README.md)",
                    "退役:latency-probe",
                    1,
                ))
            }),
            false,
        ),
        (
            "REG-004.7",
            edit_row("5", |l| Some(l.replacen("non_prod_only", "prod", 1))),
            false,
        ),
        (
            "REG-004.8",
            append_row("| 9 | [future](../future/README.md) | core | 默认 |"),
            true,
        ),
        (
            "REG-004.9",
            append_row("| 10 | [future](../future/README.md) | core | 默认 |"),
            false,
        ),
        ("REG-004.10", edit_row("0xFF00–0xFFFF", |_| None), true),
    ];
    for (row, text, ok) in rows {
        let r = check_ledger(&text, k);
        assert_eq!(r.is_ok(), ok, "{row}: {r:?}");
    }
}
