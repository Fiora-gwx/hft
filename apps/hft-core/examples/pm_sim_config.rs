//! Generates the Polymarket sim configuration (`--dry-run`, `live_feed` venues), with no
//! credentials: book-observer on 16 slot insts, or with `--maker` the binary-maker simulation
//! (three BTC slot pairs plus the Binance spot BTCUSDT reference, queue fill model). With
//! `--live UID KEYS_DIR` the live maker instead (`order_env = "prod"`, account `pm-live` with venue
//! UID `UID`, credential files in `KEYS_DIR`, micro parameters and risk limits).
//! `--day-loss-carry YYYY-MM-DD USDC` (live only): the day P&L (< 0) of earlier runs on that UTC
//! day, booked at the first day roll of the new journal so the daily loss limit counts it.
#[path = "../tests/support/mod.rs"]
mod support;

/// `YYYY-MM-DD` → days since 1970-01-01 (proleptic Gregorian).
fn utc_day(s: &str) -> Option<u32> {
    let mut it = s.split('-');
    let (y, m, d): (i64, i64, i64) = (
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
    );
    if it.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    u32::try_from(era * 146_097 + doe - 719_468).ok()
}

/// A USDC decimal (at most 6 places) → raw 1e-6.
fn usdc(s: &str) -> Option<i64> {
    let (neg, b) = s.strip_prefix('-').map_or((false, s), |r| (true, r));
    let (i, f) = b.split_once('.').unwrap_or((b, ""));
    if f.len() > 6 || i.is_empty() {
        return None;
    }
    let raw = i.parse::<i64>().ok()? * 1_000_000 + format!("{f:0<6}").parse::<i64>().ok()?;
    Some(if neg { -raw } else { raw })
}

fn main() {
    let usage = "usage: pm_sim_config [--maker | --live UID KEYS_DIR [--day-loss-carry YYYY-MM-DD USDC]] OUTPUT_DIR TMPFS_DIR SRC_IP BIND_DEVICE [USER] [PIPELINE_USER]";
    let mut all: Vec<String> = std::env::args().skip(1).collect();
    let carry = all.iter().position(|a| a == "--day-loss-carry").map(|i| {
        let v: Vec<String> = all.drain(i..i + 3).collect();
        let day = v.get(1).and_then(|d| utc_day(d)).expect(usage);
        let pnl = v
            .get(2)
            .and_then(|p| usdc(p))
            .filter(|p| *p < 0)
            .expect("--day-loss-carry: USDC must be < 0");
        (day, pnl)
    });
    let live = all.iter().position(|a| a == "--live").map(|i| {
        let mut v: Vec<String> = all.drain(i..i + 3).collect();
        let keys_dir = std::path::PathBuf::from(v.pop().expect(usage));
        let uid = v.pop().expect(usage);
        support::PmLive {
            uid,
            keys_dir,
            day_loss_carry: carry,
        }
    });
    assert!(carry.is_none() || live.is_some(), "--day-loss-carry needs --live");
    let maker = all.iter().any(|a| a == "--maker");
    let mut args = all.into_iter().filter(|a| a != "--maker");
    let root = std::path::PathBuf::from(args.next().expect(usage));
    let hot = std::path::PathBuf::from(args.next().expect(usage));
    let src_ip = args.next().expect(usage);
    let dev = args.next().expect(usage);
    let user = args.next();
    let pipeline = args.next();
    let path = if let Some(l) = &live {
        support::write_polymarket_maker_live(
            &root,
            &hot,
            user.as_deref(),
            pipeline.as_deref(),
            &src_ip,
            &dev,
            l,
        )
    } else {
        let write = if maker {
            support::write_polymarket_maker_sim
        } else {
            support::write_polymarket_sim
        };
        write(&root, &hot, user.as_deref(), pipeline.as_deref(), &src_ip, &dev)
    };
    println!("{}", path.display());
}
