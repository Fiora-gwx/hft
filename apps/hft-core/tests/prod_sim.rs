//! CORE-449: Sim accounts (Shadow variants) in a prod instance — configuration, task and thread
//! placement. CORE-450: the simulated exchange's decision barrier holds only for Sim accounts.
mod support;
use hft_core::config::{Assembly, SIM_VENUE, load, sim_venue_placement};
use hft_types::AccountId;
use support::Shadow;

fn codes(path: &std::path::Path, cfg: &toml::Value) -> Vec<String> {
    std::fs::write(path, toml::to_string(cfg).unwrap()).unwrap();
    match load(path, false) {
        Ok(_) => Vec::new(),
        Err(e) => e.iter().map(|e| e.code.to_string()).collect(),
    }
}

fn read(path: &std::path::Path) -> toml::Value {
    toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn thread_mut<'a>(cfg: &'a mut toml::Value, task: &str) -> &'a mut toml::value::Table {
    cfg["threads"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|t| t["task"].as_str() == Some(task))
        .unwrap()
        .as_table_mut()
        .unwrap()
}

#[test]
fn core_449_prod_instance_with_sim_accounts() {
    let d = tempfile::tempdir().unwrap();
    let path = support::write_live_shadow(d.path(), &d.path().join("hot"));
    let base = read(&path);
    // .1 Live account 1 and Sim account 2 (own instance) load as a Live assembly with sim-venue;
    // the journal set is the same as without the Sim account (sim-venue writes no raw journal).
    let l = load(&path, false).unwrap();
    assert_eq!(l.assembly, Assembly::Live { prod_orders: false }, "CORE-449.1");
    assert_eq!(l.cfg.assembly(), l.assembly, "CORE-449.1");
    assert_eq!(l.cfg.sim_task(), Some(SIM_VENUE), "CORE-449.1");
    assert!(l.setup.role_prod, "CORE-449.1");
    assert!(
        matches!(l.rd.account(AccountId(2)).kind, hft_refdata::AccountKind::Sim(_)),
        "CORE-449.1"
    );
    assert_eq!(l.configs.params.len(), 2, "CORE-449.1");
    assert_eq!(
        l.setup.keys_required,
        [(AccountId(1), hft_types::VenueId::BINANCE_SPOT, 3)],
        "CORE-449.1 keys only for the Live account"
    );
    assert_eq!(
        hft_core::budget::journals(&l.cfg, l.assembly),
        ["seq", "raw-feed-public", "raw-gw-public"],
        "CORE-449.1"
    );
    // A prod instance without Sim accounts has no sim task.
    let d0 = tempfile::tempdir().unwrap();
    let plain = support::write_live(d0.path(), &d0.path().join("hot"), false);
    assert_eq!(load(&plain, false).unwrap().cfg.sim_task(), None, "CORE-449.1");
    // .2 Sim accounts without `[sim]`: SimConfig (and the sim-venue thread is then unknown).
    let mut c = base.clone();
    c.as_table_mut().unwrap().remove("sim");
    let got = codes(&path, &c);
    assert!(got.iter().any(|g| g == "ThreadPlan"), "CORE-449.2 {got:?}");
    c["threads"]
        .as_array_mut()
        .unwrap()
        .retain(|t| t["task"].as_str() != Some(SIM_VENUE));
    assert_eq!(codes(&path, &c), ["SimConfig"], "CORE-449.2");
    // .3 `[sim]` without the sim-venue thread: ThreadPlan.
    let mut c = base.clone();
    c["threads"]
        .as_array_mut()
        .unwrap()
        .retain(|t| t["task"].as_str() != Some(SIM_VENUE));
    assert_eq!(codes(&path, &c), ["ThreadPlan"], "CORE-449.3");
    // .4 placement: a nice below another thread's, or the core of a hot task: ThreadPlan.
    let mut c = base.clone();
    thread_mut(&mut c, SIM_VENUE).insert("nice".into(), 9.into());
    assert_eq!(codes(&path, &c), ["ThreadPlan"], "CORE-449.4 nice");
    let mut c = base.clone();
    thread_mut(&mut c, "gw:public").insert("core".into(), 3.into());
    thread_mut(&mut c, SIM_VENUE).insert("core".into(), 3.into());
    assert_eq!(codes(&path, &c), ["ThreadPlan"], "CORE-449.4 core");
    thread_mut(&mut c, SIM_VENUE).insert("core".into(), 4.into());
    assert_eq!(codes(&path, &c), Vec::<String>::new(), "CORE-449.4 other core");
    let threads: Vec<hft_core::config::Thread> = c["threads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.clone().try_into().unwrap())
        .collect();
    assert!(sim_venue_placement(&threads).is_none(), "CORE-449.4");
    assert!(
        hft_core::runtime::in_cpu_list("2-5,7", 4) && !hft_core::runtime::in_cpu_list("2-5,7", 6),
        "CORE-449.4 isolated list"
    );
    // .5 paper-feed settings in a prod instance's `[sim]`: SimConfig.
    for (key, value) in [
        ("quote_interval_ms", toml::Value::from(20)),
        ("leverage", toml::Value::from(0)),
    ] {
        let mut c = base.clone();
        c["sim"].as_table_mut().unwrap().insert(key.into(), value);
        assert_eq!(codes(&path, &c), ["SimConfig"], "CORE-449.5 {key}");
    }
    let mut c = base.clone();
    let quote: toml::Value = toml::from_str("inst=1\nbid_raw=1\nask_raw=2\nqty_raw=1").unwrap();
    c["sim"]
        .as_table_mut()
        .unwrap()
        .insert("quotes".into(), toml::Value::Array(vec![quote]));
    assert_eq!(codes(&path, &c), ["SimConfig"], "CORE-449.5 quotes");
    // .6 the SimExchange is built at load: an unreadable calibration table is SimConfig.
    let mut c = base.clone();
    c["sim"].as_table_mut().unwrap().insert(
        "calib".into(),
        d.path().join("missing.json").display().to_string().into(),
    );
    assert_eq!(codes(&path, &c), ["SimConfig"], "CORE-449.6");
    // .7 `[sim]` and sim-venue in a prod instance without Sim accounts: SimConfig.
    let mut c = read(&plain);
    c.as_table_mut()
        .unwrap()
        .insert("sim".into(), base["sim"].clone());
    c["threads"]
        .as_array_mut()
        .unwrap()
        .push(thread_mut(&mut base.clone(), SIM_VENUE).clone().into());
    assert_eq!(codes(&plain, &c), ["SimConfig"], "CORE-449.7");
    // .8 keys for the Sim account: AccountMismatch.
    let mut c = base.clone();
    let mut keys = c["venues"]["public"]["accounts"][0].clone();
    keys["account"] = 2.into();
    keys.as_table_mut()
        .unwrap()
        .insert("src_ip".into(), "127.0.0.2".into());
    c["venues"]["public"]["accounts"]
        .as_array_mut()
        .unwrap()
        .push(keys);
    let got = codes(&path, &c);
    assert!(got.iter().any(|g| g == "AccountMismatch"), "CORE-449.8 {got:?}");
    // .9 a Sim account not named `sim-…`: AccountNamePrefix.
    let d9 = tempfile::tempdir().unwrap();
    let p9 = support::write_live_shadow_with(
        d9.path(),
        &d9.path().join("hot"),
        Shadow {
            label: "shadow",
            instance: 2,
        },
    );
    assert_eq!(codes(&p9, &read(&p9)), ["AccountNamePrefix"], "CORE-449.9");
    // .10 a Sim account in the Live account's instance (mixed account set): EnvAccountMismatch.
    let d10 = tempfile::tempdir().unwrap();
    let p10 = support::write_live_shadow_with(
        d10.path(),
        &d10.path().join("hot"),
        Shadow {
            label: "sim-shadow",
            instance: 1,
        },
    );
    assert!(
        codes(&p10, &read(&p10)).iter().any(|g| g == "EnvAccountMismatch"),
        "CORE-449.10"
    );
}

#[test]
fn core_450_decision_barrier_only_for_sim_accounts() {
    use hft_event::{OrderAction, OrderDecision, RecordFlags};
    use hft_types::*;
    let d = tempfile::tempdir().unwrap();
    let path = support::write_live_shadow(d.path(), &d.path().join("hot"));
    let rd = load(&path, false).unwrap().rd;
    let decision = |account: u8, reject: u32| OrderDecision {
        trigger_seq: 0,
        decided_tsc: Tsc(0),
        px: Px(100),
        qty: Qty(3),
        group: GroupId(0),
        inst: InstId(1),
        slot: SlotId(1),
        r#gen: 1,
        target_slot: SlotId(1),
        reject,
        strategy: StrategyId(account),
        venue: VenueId::BINANCE_SPOT,
        account: AccountId(account),
        action: OrderAction::New,
        side: Side::Buy,
        tif: Tif::Gtc,
        oflags: 0,
        reason: 0,
        intent_no: 0,
        req_px: Px(0),
        link_role: 0,
        _pad: [0; 7],
    };
    let barrier = hft_core::paper::barrier;
    // .1 the Sim account's accepted decision is a barrier.
    assert!(barrier(&rd, &decision(2, 0), RecordFlags(0)), "CORE-450.1");
    // .2 the Live account's accepted decision is not (its command goes to gw).
    assert!(!barrier(&rd, &decision(1, 0), RecordFlags(0)), "CORE-450.2");
    // .3 a rejected or SHADOW decision of the Sim account is not.
    assert!(!barrier(&rd, &decision(2, 1), RecordFlags(0)), "CORE-450.3");
    assert!(!barrier(&rd, &decision(2, 0), RecordFlags::SHADOW), "CORE-450.3");
}
