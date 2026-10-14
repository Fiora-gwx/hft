//! The only concrete venue registration table. Protocol code remains outside the app.
use crate::Error;
use crate::{
    config::{Config, Thread, Venue},
    io,
    runtime::{Workers, byte_ring, idle, policy, ring},
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
use hft_venue_proto::{
    ArrayBuf, Credential, RouteKind, SecretBuf, SignAlg, VenueCfg, VenueEnv, VenueProtocol,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

type TaskFn = Box<dyn FnOnce(Arc<AtomicBool>, Thread) -> Result<(), Error> + Send>;
pub struct VenueTasks(Vec<(Thread, TaskFn)>);
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
) -> Result<(VenueTasks, Option<io::GatewayInput>, Vec<storage::Side>), Error> {
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
            || venue_binance::GwCtx,
            SignAlg::Ed25519,
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
            || venue_binance::GwCtx,
            SignAlg::Ed25519,
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
        ),
        _ => Err(Error::config("VenueNotBuilt", &v.protocol)),
    }
}

fn endpoint(rd: &RefData, venue: VenueId, role: EndpointRole) -> Result<Endpoint, Error> {
    let e = rd
        .endpoints(venue, EndpointEnv::Testnet)
        .iter()
        .find(|e| e.role == role)
        .ok_or_else(|| Error::config("Endpoint", format!("venue={} role={role:?}", venue.0)))?;
    let url = e
        .url
        .strip_prefix("wss://")
        .ok_or_else(|| Error::config("Endpoint", "requires wss://"))?;
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

fn credential(
    key: &std::path::Path,
    secret: &std::path::Path,
    passphrase: Option<&std::path::Path>,
    alg: SignAlg,
) -> Result<Credential, Error> {
    use std::io::Read;
    use zeroize::Zeroizing;
    fn read(path: &std::path::Path, max: usize) -> Result<Zeroizing<Vec<u8>>, Error> {
        let mut bytes = Zeroizing::new(Vec::new());
        std::fs::File::open(path)
            .and_then(|f| f.take((max + 1) as u64).read_to_end(&mut bytes))
            .map_err(|_| Error::config("CredentialRead", "cannot read credential file"))?;
        if bytes.is_empty() || bytes.len() > max {
            return Err(Error::config("CredentialShape", "invalid credential length"));
        }
        Ok(bytes)
    }
    let key = read(key, 128)?;
    let secret = read(secret, 256)?;
    let passphrase = passphrase.map(|p| read(p, 64)).transpose()?;
    let key = key.trim_ascii();
    let secret = if alg == SignAlg::Ed25519 {
        secret.as_slice()
    } else {
        secret.trim_ascii()
    };
    if key.is_empty()
        || !key.iter().all(u8::is_ascii_graphic)
        || secret.is_empty()
        || (alg == SignAlg::Ed25519 && secret.len() != 32)
    {
        return Err(Error::config(
            "CredentialShape",
            "API key must be ASCII; session seed must be 32 raw bytes",
        ));
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
fn build<P: VenueProtocol + Send>(
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
    ctx: P::FeedCtx,
    gw_ctx: fn() -> P::GwCtx,
    session_alg: SignAlg,
) -> Result<(VenueTasks, Option<io::GatewayInput>, Vec<storage::Side>), Error>
where
    P::FeedCtx: Send,
    P::GwCtx: Send,
    P::Templates: Send,
{
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
            RouteKind::Md => targets.md = id,
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
    for k in &v.accounts {
        let a = AccountId(k.account);
        let rest = credential(
            &k.rest_api_key_path,
            &k.rest_secret_path,
            k.passphrase_path.as_deref(),
            SignAlg::HmacSha256,
        )?;
        let tpl = P::build_templates(&venue, rd, a, &rest)
            .map_err(|_| Error::config("Templates", "cannot build account templates"))?;
        accounts.push(GwAccountSpec::<P> {
            account: a,
            src_ip_idx: 0,
            tpl,
            ctx: gw_ctx(),
        });
        creds.push((
            a,
            credential(
                &k.api_key_path,
                &k.session_seed_path,
                k.passphrase_path.as_deref(),
                session_alg,
            )?,
        ));
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
    let arena = ConnArena::new(
        &[
            (ConnRole::Md, 32),
            (ConnRole::Order, 4),
            (ConnRole::UserStream, 2),
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
    if !accounts.is_empty() {
        let (orders, cmds) = ring(c.rings.elements)?;
        let (out, reports) = ring(c.rings.elements)?;
        let (_acct_tx, acct) = byte_ring(c.rings.bytes)?;
        let kill = Box::leak(Box::new(hft_rt::CachePadded(std::sync::atomic::AtomicU32::new(
            0,
        ))));
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
        gateway = Some(io::GatewayInput {
            venue: P::VENUE,
            reports,
            orders,
            kill,
        });
        tasks.push((
            policy(c, &format!("gw:{name}"))?,
            Box::new(move |stop, t| {
                while !stop.load(Ordering::Acquire) {
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
    tasks.push((
        policy(c, &format!("conn-builder:{name}"))?,
        Box::new(move |stop, t| {
            // The M1.5 runtime has no REST snapshot producer; retain both endpoints.
            // FeedRuntime does not submit SnapReq until that upstream feature exists.
            let _snapshot_endpoints = (snapreq_rx, snap_tx);
            let mut stopping = None;
            loop {
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
                    let now = clock.to_utc_ns(hft_rt::tsc_now());
                    for (rx, tx, acct) in [(&mut fb_rx, &mut fh_tx, None), (&mut gb_rx, &mut gh_tx, account)]
                    {
                        if let Some(r) = rx.try_pop()
                            && session.start(&mut connector, r, acct, now).is_err()
                        {
                            tx.try_push(Handover::Failed(r.conn))
                                .map_err(|_| Error::runtime("HandoffFull", "connection failure", 3))?;
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
    Ok((VenueTasks(tasks), gateway, sides))
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
