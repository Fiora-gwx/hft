//! The only concrete venue registration table. Protocol code remains outside the app.
use crate::Error;
use crate::{
    config::{Config, Thread, Venue},
    io,
    runtime::{ThreadContext, Workers, byte_ring, idle, policy, ring},
    storage,
};
use hft_event::{ConnRole, FeedCmd, FeedMsg};
use hft_net::{ConnArena, Connector, Endpoint, EndpointId};
use hft_refdata::{EndpointEnv, EndpointRole, RefData};
use hft_rt::{ByteProducer, Consumer, Producer, TscClock};
use hft_types::AccountId;
use hft_types::VenueId;
use hft_venue::{FeedCfg, FeedEndpoints, GwCfg};
use hft_venue::{FeedDeps, FeedRuntime, GwAccountSpec, GwDeps, GwRuntime, Handover};
use hft_venue::{SessEvent, SessionCfg, SessionDriver};
use hft_venue_proto::account::{AccountProtocol, State};
use hft_venue_proto::{ArrayBuf, Credential, RouteKind, SecretBuf, SignAlg, VenueCfg, VenueEnv};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use zerocopy::{FromZeros, IntoBytes};

type TaskFn = Box<dyn FnOnce(Arc<AtomicBool>, ThreadContext) -> Result<(), Error> + Send>;
pub struct VenueTasks(Vec<(Thread, TaskFn)>);
pub type BuiltVenue = (VenueTasks, Option<io::GatewayInput>, Vec<storage::Side>, State);
impl std::fmt::Debug for VenueTasks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VenueTasks")
            .field("count", &self.0.len())
            .finish()
    }
}
impl VenueTasks {
    pub fn spawn(self, workers: &mut Workers) -> Result<(), Error> {
        for (t, f) in self.0 {
            workers.spawn(t, f)?;
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub fn assemble(
    c: &Config,
    setup: &hft_sequencer::EngineSetup,
    name: &str,
    v: &Venue,
    rd: &'static RefData,
    clock: TscClock,
    epoch: u32,
    bbo: Producer<FeedMsg>,
    md: Producer<FeedMsg>,
    bulk: ByteProducer,
    commands: Consumer<FeedCmd>,
    secrets: &crate::secrets::SecretFiles,
) -> Result<BuiltVenue, Error> {
    match v.protocol.as_str() {
        "binance-usdm" => build::<venue_binance::Usdm>(
            c,
            setup,
            name,
            v,
            rd,
            clock,
            epoch,
            bbo,
            md,
            bulk,
            commands,
            venue_binance::FeedCtx::USDM,
            venue_binance::GwCtx::default,
            SignAlg::Ed25519,
            secrets,
        ),
        "binance-spot" => build::<venue_binance::Spot>(
            c,
            setup,
            name,
            v,
            rd,
            clock,
            epoch,
            bbo,
            md,
            bulk,
            commands,
            venue_binance::FeedCtx::SPOT,
            venue_binance::GwCtx::default,
            SignAlg::Ed25519,
            secrets,
        ),
        #[cfg(feature = "bitget")]
        "bitget-usdt-futures" => build::<venue_bitget::BitgetUsdtFutures>(
            c,
            setup,
            name,
            v,
            rd,
            clock,
            epoch,
            bbo,
            md,
            bulk,
            commands,
            venue_bitget::FeedCtx::new(),
            venue_bitget::GwCtx::default,
            SignAlg::HmacSha256,
            secrets,
        ),
        #[cfg(feature = "bitget")]
        "bitget-spot" => build::<venue_bitget::BitgetSpot>(
            c,
            setup,
            name,
            v,
            rd,
            clock,
            epoch,
            bbo,
            md,
            bulk,
            commands,
            venue_bitget::FeedCtx::new(),
            venue_bitget::GwCtx::default,
            SignAlg::HmacSha256,
            secrets,
        ),
        _ => Err(Error::config("VenueNotBuilt", &v.protocol)),
    }
}

/// Local demo credentials must never be sent to a production host hidden under a testnet tag.
pub fn local_demo_endpoint(protocol: &str, role: EndpointRole, url: &str) -> bool {
    let scheme = if role == EndpointRole::Rest {
        "https://"
    } else {
        "wss://"
    };
    let Some(address) = url.strip_prefix(scheme) else {
        return false;
    };
    let host = address.split('/').next().unwrap_or("");
    match (protocol, role) {
        ("binance-usdm", EndpointRole::Rest) => host == "demo-fapi.binance.com",
        ("binance-usdm", EndpointRole::OrderWs) => host == "testnet.binancefuture.com",
        ("binance-usdm", _) => host == "demo-fstream.binance.com",
        ("binance-spot", EndpointRole::Rest) => host == "demo-api.binance.com",
        ("binance-spot", EndpointRole::OrderWs | EndpointRole::UserData) => host == "demo-ws-api.binance.com",
        ("binance-spot", _) => host == "demo-stream.binance.com",
        ("bitget-usdt-futures" | "bitget-spot", EndpointRole::Rest) => host == "api.bitget.com",
        ("bitget-usdt-futures" | "bitget-spot", _) => host == "wspap.bitget.com",
        _ => false,
    }
}

fn endpoint(rd: &RefData, venue: VenueId, role: EndpointRole) -> Result<Endpoint, Error> {
    let e = rd
        .endpoints(venue, EndpointEnv::Testnet)
        .iter()
        .find(|e| e.role == role)
        .ok_or_else(|| Error::config("Endpoint", format!("venue={} role={role:?}", venue.0)))?;
    let scheme = if role == EndpointRole::Rest {
        "https://"
    } else {
        "wss://"
    };
    let url = e
        .url
        .strip_prefix(scheme)
        .ok_or_else(|| Error::config("Endpoint", "requires verified TLS"))?;
    let (host, path) = url.split_once('/').unwrap_or((url, ""));
    if host.is_empty() || host.contains(['@', ':', '?', '#']) {
        return Err(Error::config("Endpoint", "requires a DNS host on port 443"));
    }
    Ok(Endpoint {
        host: host.into(),
        sni: host.into(),
        port: 443,
        path: format!("/{path}"),
        ..Endpoint::default()
    })
}

pub fn credential(
    key: &std::path::Path,
    secret: &std::path::Path,
    passphrase: Option<&std::path::Path>,
    alg: SignAlg,
    secrets: &crate::secrets::SecretFiles,
) -> Result<Credential, Error> {
    use zeroize::Zeroizing;
    let read = |path: &std::path::Path, max: usize| {
        let bytes = secrets.get(path)?;
        if bytes.is_empty() || bytes.len() > max {
            return Err(Error::config("CredentialShape", "invalid credential length"));
        }
        Ok(bytes)
    };
    let key = read(key, 128)?;
    let secret = read(secret, 4096)?;
    let passphrase = passphrase.map(|p| read(p, 64)).transpose()?;
    let key = key.trim_ascii();
    let seed = if alg == SignAlg::Ed25519 {
        Some(Zeroizing::new(venue_binance::session_seed(secret).ok_or_else(
            || Error::config("CredentialShape", "invalid Ed25519 raw/PKCS8 key"),
        )?))
    } else {
        None
    };
    let secret = if let Some(seed) = &seed {
        seed.as_slice()
    } else {
        secret.trim_ascii()
    };
    if key.is_empty()
        || !key
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_')
        || secret.is_empty()
        || (alg == SignAlg::Ed25519 && secret.len() != 32)
        || passphrase.as_ref().is_some_and(|p| {
            p.trim_ascii().is_empty()
                || !p
                    .trim_ascii()
                    .iter()
                    .all(|b| b.is_ascii_graphic() && *b != b'"' && *b != b'\\')
        })
    {
        return Err(Error::config("CredentialShape", "invalid API key or signing key"));
    }
    Ok(Credential {
        api_key: ArrayBuf::from_bytes(key).ok_or_else(|| Error::config("CredentialShape", "key length"))?,
        secret: SecretBuf(
            ArrayBuf::from_bytes(secret).ok_or_else(|| Error::config("CredentialShape", "secret length"))?,
        ),
        alg,
        passphrase: ArrayBuf::from_bytes(passphrase.as_ref().map_or(&[], |p| p.trim_ascii()))
            .ok_or_else(|| Error::config("CredentialShape", "passphrase length"))?,
    })
}

#[allow(clippy::too_many_arguments)]
fn build<P: AccountProtocol + Send>(
    c: &Config,
    setup: &hft_sequencer::EngineSetup,
    name: &str,
    v: &Venue,
    rd: &'static RefData,
    clock: TscClock,
    epoch: u32,
    bbo: Producer<FeedMsg>,
    md: Producer<FeedMsg>,
    bulk: ByteProducer,
    commands: Consumer<FeedCmd>,
    mut ctx: P::FeedCtx,
    gw_ctx: fn() -> P::GwCtx,
    session_alg: SignAlg,
    secrets: &crate::secrets::SecretFiles,
) -> Result<BuiltVenue, Error>
where
    P::FeedCtx: Send + Clone,
    P::GwCtx: Send,
    P::Templates: Send,
{
    // Complete the self-test before even the account-baseline REST connection is opened.
    let kill: Option<&'static hft_rt::KillWord> = if v.accounts.is_empty() {
        None
    } else {
        let word = Box::leak(Box::new(hft_rt::CachePadded(std::sync::atomic::AtomicU32::new(
            0,
        ))));
        for k in &v.accounts {
            let ix = rd
                .venue_account_ix(P::VENUE, AccountId(k.account))
                .ok_or_else(|| Error::config("AccountBinding", name))?;
            selftest_word(word, ix)?;
        }
        Some(word)
    };
    let venue = VenueCfg {
        env: VenueEnv::Testnet,
        stale_guard_ms: v.stale_guard_ms,
    };
    let mut net = hft_net::NetConfig::default();
    let ip = v.src_ip.ok_or_else(|| Error::config("SourceBind", name))?;
    net.src_ips.push(ip);
    net.bind_device.push((
        ip,
        v.bind_device
            .clone()
            .ok_or_else(|| Error::config("SourceBind", name))?,
    ));
    net.ts_expect_hw = false;
    let mut targets = FeedEndpoints::default();
    for route in P::routes() {
        let role = if route.kind == RouteKind::Md {
            EndpointRole::MdMarket
        } else {
            EndpointRole::MdPublic
        };
        let mut e = endpoint(rd, P::VENUE, role)?;
        let mut spec = hft_venue_proto::EndpointSpec::default();
        P::endpoint(ConnRole::Md, route.id, &venue, None, None, 0, &mut spec)
            .map_err(|_| Error::config("Endpoint", name))?;
        e.path = std::str::from_utf8(spec.path_query.as_slice())
            .map_err(|_| Error::config("Endpoint", "invalid path"))?
            .into();
        if !spec.headers.is_empty() {
            return Err(Error::config(
                "Endpoint",
                "public route requires unsupported headers",
            ));
        }
        let id = EndpointId(net.endpoints.len() as u8);
        net.endpoints.push(e);
        match route.kind {
            RouteKind::Bbo => targets.bbo = id,
            RouteKind::DepthTopN => targets.depth = id,
            RouteKind::Md => {
                targets.md_routes[usize::from(route.id.0)] = Some(id);
            }
        }
    }
    let mut gwcfg = GwCfg {
        env: VenueEnv::Testnet,
        oms_slots: v
            .accounts
            .iter()
            .map(|k| setup.oms.slots.get(P::VENUE, AccountId(k.account)))
            .max()
            .unwrap_or(1),
        ..GwCfg::default()
    };
    let mut accounts = Vec::new();
    let mut creds = Vec::new();
    let mut account_client = None;
    let mut baseline = State::default();
    for k in &v.accounts {
        let a = AccountId(k.account);
        let rest = credential(
            &k.rest_api_key_path,
            &k.rest_secret_path,
            k.passphrase_path.as_deref(),
            if k.rest_sign_alg.as_deref() == Some("ed25519") {
                SignAlg::Ed25519
            } else {
                SignAlg::HmacSha256
            },
            secrets,
        )?;
        let session_key = credential(
            &k.api_key_path,
            &k.session_seed_path,
            k.passphrase_path.as_deref(),
            session_alg,
            secrets,
        )?;
        if rd.account(a).uid_policy == hft_refdata::UidPolicy::Unverifiable
            && rest.api_key != session_key.api_key
        {
            return Err(Error::config(
                "AccountIdentity",
                "unverifiable demo identity requires the same API key for REST and session",
            ));
        }
        let params = v.account_io.ok_or_else(|| Error::config("AccountIo", name))?;
        let mut rest_net = net.clone();
        let rest_endpoint = endpoint(rd, P::VENUE, EndpointRole::Rest)?;
        let host = rest_endpoint.host.clone();
        rest_net.endpoints = vec![rest_endpoint];
        let rest_arena = ConnArena::new(&[(ConnRole::Rest, 2)], &rest_net, clock);
        let rest_connector =
            Connector::new(rest_arena, &rest_net).map_err(|_| Error::config("AccountNetwork", name))?;
        let mut client = hft_venue::account::Client::<P>::new(
            hft_net::rest::Client::new(rest_connector, 8 << 20),
            host,
            hft_venue::account::Config {
                timeout_ms: params.timeout_ms,
                interval_ms: params.interval_ms,
                request_gap_ms: params.request_gap_ms,
                max_cancel_rounds: params.max_cancel_rounds,
                listen_keepalive_ms: params.listen_keepalive_ms,
                funding_window_ms: setup.ledger.funding_recon_window_ms,
                funding_lag_ms: setup.ledger.funding_recon_lag_ms,
            },
            venue,
            rd,
            a,
            rest,
            session_key,
        );
        client.start();
        // Serial startup only: no strategy or gateway can place an order before the baseline.
        loop {
            client
                .poll(clock.to_utc_ns(hft_rt::tsc_now()))
                .map_err(|e| account_error(name, &e))?;
            if let Some(state) = client.take() {
                baseline = state;
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        // The first live Funding event records this REST-derived interval in seq journal.
        // Rates/timestamps still come from the live frame; failed REST keeps the existing wait.
        P::seed_funding(&mut ctx, &baseline.funding_schedule)
            .map_err(|_| Error::config("FundingBootstrap", "feed schedule capacity or shape"))?;
        account_client = Some(client);
        let tpl = P::build_templates(&venue, rd, a, &rest)
            .map_err(|_| Error::config("Templates", "cannot build account templates"))?;
        accounts.push(GwAccountSpec::<P> {
            account: a,
            src_ip_idx: 0,
            rest_credential: rest,
            tpl,
            ctx: gw_ctx(),
        });
        creds.push((a, session_key));
    }
    if !accounts.is_empty() {
        gwcfg.order_endpoint = EndpointId(net.endpoints.len() as u8);
        net.endpoints.push(endpoint(rd, P::VENUE, EndpointRole::OrderWs)?);
        gwcfg.user_endpoint = EndpointId(net.endpoints.len() as u8);
        if P::USER_STREAM {
            net.endpoints
                .push(endpoint(rd, P::VENUE, EndpointRole::UserData)?);
        }
    }
    let mut gateway_rest = None;
    if !accounts.is_empty() {
        let ep = endpoint(rd, P::VENUE, EndpointRole::Rest)?;
        let id = EndpointId(net.endpoints.len() as u8);
        gateway_rest = Some((id, ep.host.clone()));
        net.endpoints.push(ep);
    }
    let arena = ConnArena::new(
        &[
            (ConnRole::Md, 32),
            (ConnRole::Order, 4),
            (ConnRole::UserStream, 2),
            (ConnRole::Rest, 2),
        ],
        &net,
        clock,
    );
    let mut connector =
        Connector::new(arena, &net).map_err(|e| Error::config("Network", format!("{e:?}")))?;
    let mut session = SessionDriver::<P>::new(
        SessionCfg {
            venue,
            ready_bbo: None,
            ready_md: None,
        },
        creds,
    );
    let (mut fh_tx, fh_rx) = ring(256)?;
    let (fb_tx, mut fb_rx) = ring(256)?;
    let (fr_tx, mut fr_rx) = ring(256)?;
    let (mut gh_tx, gh_rx) = ring(256)?;
    let (gb_tx, mut gb_rx) = ring(256)?;
    let (gr_tx, mut gr_rx) = ring(256)?;
    let raw = |suffix: &str| -> Result<_, Error> {
        let (mut w, side) = storage::open(
            storage::config(
                c,
                &format!("raw-{suffix}-{name}"),
                hft_journal::JournalKind::Raw,
                P::VENUE,
            ),
            epoch,
        )?;
        w.roll(
            hft_journal::RollReason::Startup,
            &hft_journal::RollInfo {
                utc_ns: clock.base.realtime_ns,
                calib: clock.base,
                tsc_hz_milli: clock.hz_milli,
                configs: &[],
            },
        )
        .map_err(|e| Error::runtime("RawBoot", format!("{e:?}"), 4))?
        .finish()
        .map_err(|e| Error::runtime("RawBoot", format!("{e:?}"), 4))?;
        Ok((w, side))
    };
    let (snapreq_tx, snapreq_rx) = ring(64)?;
    let (snap_tx, snap_rx) = byte_ring(c.rings.bytes)?;
    let mut snap_net = net.clone();
    let public_rest = endpoint(rd, P::VENUE, EndpointRole::Rest)?;
    let snap_host = public_rest.host.clone();
    snap_net.endpoints = vec![public_rest];
    let snap_arena = ConnArena::new(&[(ConnRole::Rest, 1)], &snap_net, clock);
    let snap_connector =
        Connector::new(snap_arena, &snap_net).map_err(|_| Error::config("SnapshotNetwork", name))?;
    let snapshot_pool = hft_net::RestPool::<1>::new(snap_host.as_bytes(), &snap_net, clock);
    let mut snapshots = hft_venue::PooledSnapFetchTask::<P, 1>::new(
        hft_venue::SnapCfg {
            parallel: 1,
            timeout_ms: snap_net.rest_timeout_ms,
            max_record_bytes: (c.rings.bytes / 4)
                .try_into()
                .map_err(|_| Error::config("SnapshotRing", name))?,
            ..hft_venue::SnapCfg::default()
        },
        hft_venue::SnapDeps {
            rd,
            reqs: [Some(snapreq_rx), None],
            out: [Some(snap_tx), None],
            clock: clock.tsc_utc(&clock.base),
        },
        ctx.clone(),
        snapshot_pool,
    )
    .map_err(|_| Error::config("SnapshotConfig", name))?;
    snapshots
        .attach_connector(snap_connector, EndpointId(0), 0)
        .map_err(|_| Error::config("SnapshotHost", name))?;
    let (fw, fs) = raw("feed")?;
    let mut sides = vec![fs];
    let mut feed = FeedRuntime::<P>::new(
        FeedCfg {
            max_sub_insts: v.max_sub_insts,
            venue,
            endpoints: targets,
            ..FeedCfg::default()
        },
        FeedDeps {
            rd,
            bbo: Some(bbo),
            md: Some(md),
            bulk: Some(bulk),
            cmds: commands,
            snapreq: Some(snapreq_tx),
            snap: Some(snap_rx),
            ready: None,
            raw: fw,
            handoff: fh_rx,
            retire: fr_tx,
            build: fb_tx,
            clock: clock.tsc_utc(&clock.base),
        },
        ctx,
    )
    .map_err(|e| Error::config("Feed", format!("{e:?}")))?;
    let mut tasks: Vec<(Thread, TaskFn)> = vec![(
        policy(c, &format!("feed:{name}"))?,
        Box::new(move |stop, t| {
            while !stop.load(Ordering::Acquire) {
                t.heartbeat.beat();
                if !feed.poll_once(hft_rt::tsc_now().0).did_work {
                    idle(&t);
                }
            }
            feed.retire_all(hft_rt::tsc_now().0);
            feed.seal_and_close()
                .map_err(|e| Error::runtime("RawClose", format!("{e:?}"), 4))
        }),
    )];
    let mut gateway = None;
    let (mut account_tx, account_rx) = byte_ring(c.rings.bytes)?;
    let (rest_order_tx, mut rest_order_rx) = ring(c.rings.elements)?;
    let (mut rest_reply_tx, rest_reply_rx) = ring(c.rings.elements)?;
    publish_account(&mut account_tx, &baseline)?;
    if !accounts.is_empty() {
        let (orders, cmds) = ring(c.rings.elements)?;
        let (out, reports) = ring(c.rings.elements)?;
        let (rep_bulk_tx, rep_bulk) = byte_ring(c.rings.bytes)?;
        let (_acct_tx, acct) = byte_ring(c.rings.bytes)?;
        let kill = kill.ok_or_else(|| Error::config("KillSelfTest", name))?;
        for k in &v.accounts {
            let a = AccountId(k.account);
            let (owner, timer_id, group) =
                hft_event::TimerCode::Sys(hft_event::SysTimer::KillSelfTestOk { account: a })
                    .encode()
                    .map_err(|_| Error::config("KillSelfTest", name))?;
            let timer = hft_event::Timer {
                owner,
                timer_id,
                group,
                ..hft_event::Timer::new_zeroed()
            };
            if !io::write_record(&mut account_tx, hft_event::EventKind::TIMER, &[timer.as_bytes()]) {
                return Err(Error::config("AccountRing", name));
            }
            let report = hft_event::ReconcileReport {
                report_id: u64::from(epoch) << 32 | u64::from(a.0) << 8 | u64::from(P::VENUE.0),
                venue: P::VENUE,
                account: a,
                item: 5,
                severity: 1,
                count: 1,
                ..hft_event::ReconcileReport::new_zeroed()
            };
            if !io::write_record(
                &mut account_tx,
                hft_event::EventKind::RECONCILE_REPORT,
                &[report.as_bytes()],
            ) {
                return Err(Error::config("AccountRing", name));
            }
        }
        let (gw, gs) = raw("gw")?;
        sides.push(gs);
        let mut gw = GwRuntime::<P, 8192>::new(
            gwcfg,
            GwDeps {
                rd,
                cmds,
                out,
                acct,
                raw: gw,
                kill,
                kill_mask: hft_types::KillBits::GLOBAL.0
                    | hft_types::KillBits::VENUE.0
                    | hft_types::KillBits::HARD.0,
                tx_tags: None,
                handoff: gh_rx,
                retire: gr_tx,
                build: gb_tx,
                clock: clock.tsc_utc(&clock.base),
            },
            accounts,
        )
        .map_err(|e| Error::config("Gateway", format!("{e:?}")))?;
        gw.set_rest(rest_order_tx, rest_reply_rx);
        gw.set_rep_bulk(rep_bulk_tx);
        if let Some((endpoint, host)) = gateway_rest {
            let pool = hft_net::RestPool::<2>::new(host.as_bytes(), &net, clock);
            gw.set_rest_pools(AccountId(v.accounts[0].account), vec![(endpoint, pool)])
                .map_err(|_| Error::config("RestPool", name))?;
        }
        gateway = Some(io::GatewayInput {
            venue: P::VENUE,
            reports,
            rep_bulk,
            pending_report: None,
            orders,
            kill,
            account_reports: io::Inputs::new(account_rx),
        });
        tasks.push((
            policy(c, &format!("gw:{name}"))?,
            Box::new(move |stop, t| {
                while !stop.load(Ordering::Acquire) {
                    t.heartbeat.beat();
                    if !gw.poll_once(hft_rt::tsc_now().0).did_work {
                        idle(&t);
                    }
                }
                gw.retire_all(hft_rt::tsc_now().0);
                gw.seal_and_close()
                    .map_err(|e| Error::runtime("RawClose", format!("{e:?}"), 4))
            }),
        ));
    }
    let account = v.accounts.first().map(|k| AccountId(k.account));
    let task_name = name.to_owned();
    let v_timeout_ms = v.account_io.as_ref().map_or(10_000, |x| x.timeout_ms);
    tasks.push((
        policy(c, &format!("conn-builder:{name}"))?,
        Box::new(move |stop, t| {
            let mut stopping = None;
            let mut pending_private = None;
            let mut reauth_started = false;
            let mut pending_rest = None;
            loop {
                t.heartbeat.beat();
                let stopping_now = stop.load(Ordering::Acquire);
                if stopping_now && stopping.is_none() {
                    stopping = Some(Instant::now());
                }
                while let Some(r) = fr_rx.try_pop() {
                    session.retire(&mut connector, r);
                }
                while let Some(r) = gr_rx.try_pop() {
                    session.retire(&mut connector, r);
                }
                if stopping.is_some_and(|s| s.elapsed() >= Duration::from_millis(100)) {
                    return Ok(());
                }
                if !stopping_now {
                    snapshots.poll_once(hft_rt::tsc_now().0);
                    let now = clock.to_utc_ns(hft_rt::tsc_now());
                    if let Some(r) = fb_rx.try_pop()
                        && session.start(&mut connector, r, None, now).is_err()
                    {
                        fh_tx
                            .try_push(Handover::Failed(r.conn))
                            .map_err(|_| Error::runtime("HandoffFull", "public connection", 3))?;
                    }
                    if pending_private.is_none() {
                        pending_private = gb_rx.try_pop();
                        reauth_started = false;
                    }
                    if pending_private.is_some_and(|r| r.role == ConnRole::Rest)
                        && let Some(r) = pending_private.take()
                        && session.start(&mut connector, r, None, now).is_err()
                    {
                        gh_tx
                            .try_push(Handover::Failed(r.conn))
                            .map_err(|_| Error::runtime("HandoffFull", "REST connection", 3))?;
                    }
                    if let Some(client) = &mut account_client {
                        if pending_rest.is_none() {
                            pending_rest = rest_order_rx.try_pop();
                        }
                        if pending_private.is_none()
                            && client.idle()
                            && let Some(r) = pending_rest.take()
                        {
                            client.order(r, now).map_err(|e| account_error(&task_name, &e))?;
                        }
                        if pending_private.is_some() && !reauth_started && client.idle() {
                            client
                                .reauthenticate()
                                .map_err(|e| account_error(&task_name, &e))?;
                            reauth_started = true;
                        }
                        if let Err(e) = client.poll(now) {
                            eprintln!("{}", account_error(&task_name, &e));
                            let h = hft_event::AccountConfigReportHdr {
                                venue: P::VENUE,
                                account: account.unwrap_or(AccountId(0)),
                                mismatches: 256,
                                source: 3,
                                ex_ts_ns: now,
                                ..hft_event::AccountConfigReportHdr::new_zeroed()
                            };
                            if !io::write_record(
                                &mut account_tx,
                                hft_event::EventKind::ACCOUNT_CONFIG_REPORT,
                                &[h.as_bytes()],
                            ) {
                                return Err(Error::runtime("AccountRing", "failed observation", 3));
                            }
                            if let Some(r) = pending_private.take() {
                                gh_tx
                                    .try_push(Handover::Failed(r.conn))
                                    .map_err(|_| Error::runtime("HandoffFull", "private connection", 3))?;
                            }
                            reauth_started = false;
                            client.retry(now);
                        }
                        if let Some(r) = client.take_order() {
                            rest_reply_tx
                                .try_push(r)
                                .map_err(|_| Error::runtime("RestRing", "REST response overflow", 3))?;
                        }
                        if let Some(state) = client.take() {
                            publish_account(&mut account_tx, &state)?;
                            if reauth_started && let Some(mut r) = pending_private.take() {
                                r.deadline_ns = now + i64::from(v_timeout_ms) * 1_000_000;
                                let started = session.start_prepared(
                                    &mut connector,
                                    r,
                                    account,
                                    now,
                                    client.listen_body(),
                                );
                                match started {
                                    Ok(job) => {
                                        if let Some(h) = state.config
                                            && h.uid_keys & 1 != 0
                                            && h.venue_uid[0] != 0
                                        {
                                            session.verified_identity(job, h.venue_uid).map_err(|_| {
                                                Error::config("SessionIdentity", "invalid verified UID")
                                            })?;
                                        }
                                    }
                                    Err(_) => {
                                        gh_tx.try_push(Handover::Failed(r.conn)).map_err(|_| {
                                            Error::runtime("HandoffFull", "private connection", 3)
                                        })?;
                                    }
                                }
                                reauth_started = false;
                            }
                        }
                    }
                    if let Some(ev) = session.drive(&mut connector, now) {
                        let (id, handover) = match ev {
                            SessEvent::Ready(h) => (h.id(), Handover::Ready(h)),
                            SessEvent::Failed { conn, .. } => (conn, Handover::Failed(conn)),
                        };
                        let tx = if id.0 < hft_venue::GW_FIRST {
                            &mut fh_tx
                        } else {
                            &mut gh_tx
                        };
                        tx.try_push(handover)
                            .map_err(|_| Error::runtime("HandoffFull", "connection ready", 3))?;
                    }
                }
                idle(&t);
            }
        }),
    ));
    Ok((VenueTasks(tasks), gateway, sides, baseline))
}

fn account_error(name: &str, e: &hft_venue::account::Error) -> Error {
    // Never format wire responses, signatures or credentials. Query labels identify the failed check.
    let detail = match e {
        hft_venue::account::Error::Protocol(q) => match q {
            hft_venue_proto::account::Query::Instruments => "instrument rules mismatch or missing symbol",
            hft_venue_proto::account::Query::Settings => {
                "account/position/margin/asset mode or identity check failed"
            }
            hft_venue_proto::account::Query::SymbolSettings => {
                "cross margin or instrument leverage check failed"
            }
            hft_venue_proto::account::Query::Permissions => "identity or API key permission check failed",
            _ => "required account response invalid",
        },
        hft_venue::account::Error::Transport(_) => "REST transport failed; exchange outcome unknown",
        hft_venue::account::Error::CancelNotQuiescent => "startup open orders did not converge to empty",
        hft_venue::account::Error::Busy => "account request already active",
        hft_venue::account::Error::Funding(reason) => {
            return Error::config("AccountBootstrap", format!("{name}: funding history {reason:?}"));
        }
    };
    Error::config("AccountBootstrap", format!("{name}: {detail}"))
}

fn publish_account(tx: &mut ByteProducer, state: &State) -> Result<(), Error> {
    let mut records = Vec::new();
    if let Some(mut h) = state.config {
        h.n_lev =
            u16::try_from(state.leverage.len()).map_err(|_| Error::config("AccountCapacity", "leverage"))?;
        let mut body = h.as_bytes().to_vec();
        for l in &state.leverage {
            body.extend_from_slice(l.as_bytes());
        }
        records.push((hft_event::EventKind::ACCOUNT_CONFIG_REPORT, body));
    }
    for b in &state.balances {
        records.push((hft_event::EventKind::BALANCE_REPORT, b.as_bytes().to_vec()));
    }
    for p in &state.positions {
        records.push((hft_event::EventKind::POSITION_REPORT, p.as_bytes().to_vec()));
    }
    for f in &state.funding {
        records.push((hft_event::EventKind::FUNDING_SETTLE, f.as_bytes().to_vec()));
    }
    if let Some(mut m) = state.margin {
        // Event-body capacity, including its header; a missing ADL value stays 255.
        let max = (16_384 - size_of::<hft_event::MarginReportHdr>()) / size_of::<hft_event::MarginItem>();
        let n = state.margin_items.len().min(max);
        m.n = n as u16;
        if n != state.margin_items.len() {
            m.mflags |= hft_event::MarginReportHdr::ITEMS_TRUNCATED;
        }
        let mut body = m.as_bytes().to_vec();
        for item in state.margin_items.iter().take(n) {
            body.extend_from_slice(item.as_bytes());
        }
        records.push((hft_event::EventKind::MARGIN_REPORT, body));
    }
    for (kind, body) in records {
        if !io::write_record(tx, kind, &[&body]) {
            return Err(Error::runtime("AccountRing", "account observations overflow", 3));
        }
    }
    Ok(())
}
pub fn registered(protocol: &str, id: VenueId) -> Result<(), Error> {
    let expected = match protocol {
        "binance-usdm" => VenueId::BINANCE_USDM,
        "binance-spot" => VenueId::BINANCE_SPOT,
        #[cfg(feature = "bitget")]
        "bitget-usdt-futures" => VenueId::BITGET_UTA_USDT_FUT,
        #[cfg(feature = "bitget")]
        "bitget-spot" => VenueId::BITGET_UTA_SPOT,
        #[cfg(not(feature = "bitget"))]
        "bitget-usdt-futures" | "bitget-spot" => {
            return Err(Error::config(
                "VenueNotBuilt",
                "Bitget requires the bitget Cargo feature",
            ));
        }
        _ => return Err(Error::config("UnknownVenue", protocol)),
    };
    if id != expected {
        return Err(Error::config("UnknownVenue", "protocol and VenueId differ"));
    }
    Ok(())
}

/// Cold self-test of the same atomic word handed to the live gateway.
pub fn selftest_word(kill: &hft_rt::KillWord, ix: u8) -> Result<(), Error> {
    if ix >= 12 {
        return Err(Error::config("KillSelfTest", "account index out of range"));
    }
    let bits = hft_types::KillBits::account(ix).0 | hft_types::KillBits::account_hard(ix).0;
    for hard in [false, true] {
        kill.store(
            hft_types::KillBits::account(ix).0
                | if hard {
                    hft_types::KillBits::account_hard(ix).0
                } else {
                    0
                },
            Ordering::Release,
        );
        let word = kill.load(Ordering::Acquire);
        if hft_venue::kill_admit(word, 7, bits, ix, hft_event::GwOp::Place, 0)
            || hft_venue::kill_admit(
                word,
                7,
                bits,
                ix,
                hft_event::GwOp::Place,
                hft_event::OrderFlags::RISK_REDUCING.0,
            ) == hard
        {
            return Err(Error::config("KillSelfTest", "atomic word/admission mismatch"));
        }
    }
    for bit in 0..32 {
        kill.store(1u32 << bit, Ordering::Release);
        if kill.load(Ordering::Acquire) != 1u32 << bit {
            return Err(Error::config("KillSelfTest", "atomic word/admission mismatch"));
        }
    }
    kill.store(0, Ordering::Release);
    Ok(())
}

#[cfg(test)]
mod account_ingress_tests {
    use super::*;
    use hft_event::{MarginItem, MarginReportHdr};
    use hft_types::{InstId, Px};
    use zerocopy::{FromBytes, FromZeros};
    #[test]
    fn core_169_margin_items_cross_account_ingress() {
        let (mut tx, mut rx) = hft_rt::byte_spsc(131_072, hft_rt::Backing::Heap).unwrap();
        let mut state = State {
            margin: Some(MarginReportHdr::new_zeroed()),
            ..State::default()
        };
        state.margin_items = (0..1100)
            .map(|i| MarginItem {
                inst: InstId(i + 1),
                adl_quantile: 255,
                _pad: [0; 3],
                liq_px: Px(100 + i64::from(i)),
            })
            .collect();
        publish_account(&mut tx, &state).unwrap();
        let (wire, body) = hft_event::CtlRecordHdr::read_from_prefix(rx.peek().unwrap()).unwrap();
        assert_eq!(wire.kind, hft_event::EventKind::MARGIN_REPORT);
        let (h, items) = MarginReportHdr::read_from_prefix(body).unwrap();
        assert_eq!(h.n, 1020);
        assert_ne!(h.mflags & MarginReportHdr::ITEMS_TRUNCATED, 0);
        assert_eq!(items.len(), 1020 * 16);
        let (item, _) = MarginItem::read_from_prefix(items).unwrap();
        assert_eq!(
            (item.inst, item.liq_px, item.adl_quantile),
            (InstId(1), Px(100), 255)
        );
    }
}
