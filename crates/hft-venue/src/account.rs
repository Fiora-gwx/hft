//! Warm account bootstrap and periodic observations. No ledger ownership, no background thread.
#![allow(clippy::disallowed_macros)] // allow-list: warm HTTP encoding, never called by a hot task.
use hft_net::{BuildReq, ConnId, ConnRole, EndpointId};
use hft_refdata::RefData;
use hft_types::{AccountId, MarketKind};
use hft_venue_proto::{
    Credential, Method, RestReqBuf, VenueCfg,
    account::{AccountProtocol, Query, State},
};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub timeout_ms: u32,
    pub interval_ms: u32,
    pub request_gap_ms: u32,
    pub max_cancel_rounds: u32,
    pub listen_keepalive_ms: u32,
    pub funding_window_ms: u64,
    pub funding_lag_ms: u32,
}
#[derive(Debug)]
pub enum Error {
    Transport(hft_net::rest::Error),
    Protocol(Query),
    CancelNotQuiescent,
    Busy,
    Funding(crate::FundingPollErr),
}
struct Work {
    query: Query,
    session: bool,
}
/// Deterministic mock-exchange seam; production uses the verified TLS client.
pub trait Transport {
    fn start(&mut self, req: BuildReq, wire: &[u8]) -> Result<(), hft_net::rest::Error>;
    fn poll(&mut self, now: i64) -> Option<Result<hft_net::rest::Response, hft_net::rest::Error>>;
}
impl Transport for hft_net::rest::Client {
    fn start(&mut self, r: BuildReq, w: &[u8]) -> Result<(), hft_net::rest::Error> {
        self.start(r, w)
    }
    fn poll(&mut self, n: i64) -> Option<Result<hft_net::rest::Response, hft_net::rest::Error>> {
        self.poll(n)
    }
}
pub struct Client<P: AccountProtocol, T: Transport = hft_net::rest::Client> {
    transport: T,
    host: String,
    cfg: Config,
    venue: VenueCfg,
    rd: &'static RefData,
    account: AccountId,
    rest: Credential,
    session: Credential,
    queue: VecDeque<Work>,
    active: Option<Work>,
    state: State,
    next: i64,
    last_poll: i64,
    last_keepalive: i64,
    cancel_rounds: u32,
    boot: bool,
    listen: Vec<u8>,
    ready: bool,
    _p: core::marker::PhantomData<P>,
    order: Option<(hft_event::GwCmd, hft_venue_proto::RestOrigin)>,
    order_reply: Option<crate::order_rest::Reply>,
    started_ms: i64,
    funding: Option<crate::FundingPoll<P::Funding>>,
    funding_active: bool,
}
impl<P: AccountProtocol, T: Transport> core::fmt::Debug for Client<P, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AccountClient")
            .field("account", &self.account)
            .finish()
    }
}
impl<P: AccountProtocol, T: Transport> Client<P, T> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        transport: T,
        host: String,
        cfg: Config,
        venue: VenueCfg,
        rd: &'static RefData,
        account: AccountId,
        rest: Credential,
        session: Credential,
    ) -> Self {
        Self {
            transport,
            host,
            cfg,
            venue,
            rd,
            account,
            rest,
            session,
            queue: VecDeque::new(),
            active: None,
            state: State::default(),
            next: 0,
            last_poll: 0,
            last_keepalive: 0,
            cancel_rounds: 0,
            boot: true,
            listen: Vec::new(),
            ready: false,
            _p: core::marker::PhantomData,
            order: None,
            order_reply: None,
            started_ms: 0,
            funding: None,
            funding_active: false,
        }
    }
    fn add(&mut self, q: Query) {
        self.queue.push_back(Work {
            query: q,
            session: false,
        });
    }
    pub fn start(&mut self) {
        self.add(Query::Instruments);
        self.settings();
        self.add(Query::OpenOrders {
            cursor: String::new(),
        });
        self.add(Query::Balances);
        if P::MARKET == MarketKind::LinearPerp {
            self.add(Query::Positions);
        }
        if P::USER_STREAM {
            self.add(Query::ListenKey);
        }
        let caps = self.rd.caps(P::VENUE);
        if caps.dms != hft_refdata::DeadManCap::None {
            self.deadman(caps.dms_timeout_ms);
        }
    }
    fn deadman(&mut self, timeout_ms: u32) {
        for i in self.rd.insts_of_venue(P::VENUE) {
            self.queue.push_back(Work {
                query: Query::DeadMan {
                    symbol: self.rd.inst_meta(*i).symbol.to_string(),
                    timeout_ms,
                },
                session: false,
            });
        }
    }
    fn settings(&mut self) {
        self.add(Query::Settings);
        if P::MARKET == MarketKind::LinearPerp {
            self.add(Query::SymbolSettings);
        }
        self.add(Query::Permissions);
        self.queue.push_back(Work {
            query: Query::Settings,
            session: true,
        });
    }
    /// Reauthentication is requested before a replacement private session can be handed over.
    pub fn reauthenticate(&mut self) -> Result<(), Error> {
        if self.active.is_some() || !self.queue.is_empty() || self.ready {
            return Err(Error::Busy);
        }
        self.settings();
        if P::USER_STREAM {
            self.add(Query::ListenKey);
        }
        Ok(())
    }
    pub fn idle(&self) -> bool {
        !self.funding_active
            && self.active.is_none()
            && self.queue.is_empty()
            && !self.ready
            && self.order.is_none()
            && self.order_reply.is_none()
    }
    pub fn take_order(&mut self) -> Option<crate::order_rest::Reply> {
        self.order_reply.take()
    }
    pub fn order(&mut self, r: crate::order_rest::Request, now: i64) -> Result<(), Error> {
        if !self.idle() {
            return Err(Error::Busy);
        }
        if r.cmd.op == hft_event::GwOp::DeadManRefresh {
            self.deadman(r.cmd.arg);
            return Ok(());
        }
        let origin = hft_venue_proto::RestOrigin {
            what: hft_venue_proto::RestWhat::Order,
            req: Some(hft_venue_proto::ReqRef {
                slot: r.target.id.slot,
                r#gen: r.target.id.r#gen,
                op: r.cmd.op,
            }),
            inst: r.cmd.inst,
        };
        let mut req = RestReqBuf::default();
        let tpl =
            P::build_templates(&self.venue, self.rd, self.account, &self.rest).map_err(|_| Error::Busy)?;
        if P::encode_rest(
            self.rd,
            &tpl,
            &r.cmd,
            &r.target,
            &hft_venue_proto::WallClock {
                unix_ms: now / 1_000_000,
                unix_us: now / 1000,
            },
            &mut req,
        )
        .is_err()
        {
            self.order_reply = Some(crate::order_rest::Reply::new(
                self.account,
                r.cmd,
                origin,
                Err(hft_net::rest::Error::Request),
            ));
            return Ok(());
        }
        if self.send(&req, now).is_err() {
            self.order_reply = Some(crate::order_rest::Reply::new(
                self.account,
                r.cmd,
                origin,
                Err(hft_net::rest::Error::Transport),
            ));
            return Ok(());
        }
        self.order = Some((r.cmd, origin));
        Ok(())
    }
    pub fn listen_body(&self) -> Option<&[u8]> {
        (!self.listen.is_empty()).then_some(self.listen.as_slice())
    }
    pub fn take(&mut self) -> Option<State> {
        if !self.ready {
            return None;
        }
        self.ready = false;
        if !self.boot
            && let Some(h) = &mut self.state.config
        {
            h.source = 3;
        }
        self.boot = false;
        Some(std::mem::take(&mut self.state))
    }
    pub fn poll(&mut self, now: i64) -> Result<bool, Error> {
        if self.started_ms == 0 {
            self.started_ms = now / 1_000_000;
            if self.rd.caps(P::VENUE).funding_settle == hft_refdata::FundingSettleKind::RateOnly {
                self.funding = Some(
                    crate::FundingPoll::new(
                        self.account,
                        self.started_ms,
                        crate::FundingPollCfg {
                            poll_ms: i64::from(self.cfg.interval_ms),
                            lookback_ms: i64::try_from(self.cfg.funding_window_ms)
                                .map_err(|_| Error::Funding(crate::FundingPollErr::Config))?,
                            settle_lag_ms: i64::from(self.cfg.funding_lag_ms),
                            max_settlement_skew_ms: i64::from(self.cfg.funding_lag_ms),
                            max_pages: 64,
                            max_bills: 4096,
                        },
                    )
                    .map_err(Error::Funding)?,
                );
            }
        }
        if let Some(result) = self.transport.poll(now) {
            if self.funding_active {
                self.funding_active = false;
                let r = result.map_err(Error::Transport)?;
                let poll = self.funding.as_mut().ok_or(Error::Busy)?;
                poll.response(self.rd, r.status, &r.body, now / 1_000_000, hft_types::Tsc(0))
                    .map_err(Error::Funding)?;
                self.state.funding = poll.take_ready();
                self.ready = !self.state.funding.is_empty();
                self.next = now + i64::from(self.cfg.request_gap_ms) * 1_000_000;
                return Ok(true);
            }
            if let Some((cmd, origin)) = self.order.take() {
                self.order_reply = Some(crate::order_rest::Reply::new(self.account, cmd, origin, result));
                self.next = now + i64::from(self.cfg.request_gap_ms) * 1_000_000;
                return Ok(true);
            }
            let work = self.active.take().ok_or(Error::Busy)?;
            let r = result.map_err(Error::Transport)?;
            let q = &work.query;
            if matches!(q, Query::ListenKey | Query::Keepalive) {
                if !(200..300).contains(&r.status) {
                    return Err(Error::Protocol(q.clone()));
                }
                if matches!(q, Query::ListenKey) {
                    // Let the protocol validate the complete listen-key response before caching it.
                    let mut target = hft_venue_proto::EndpointSpec::default();
                    P::endpoint(
                        ConnRole::UserStream,
                        hft_venue_proto::RouteId(0),
                        &self.venue,
                        Some(&self.rest),
                        Some(&r.body),
                        now / 1_000_000,
                        &mut target,
                    )
                    .map_err(|_| Error::Protocol(q.clone()))?;
                    self.listen.fill(0);
                    self.listen = r.body;
                }
                self.last_keepalive = now;
            } else {
                let state = if work.session {
                    &mut State::default()
                } else {
                    &mut self.state
                };
                P::account_decode(
                    self.rd,
                    self.account,
                    q,
                    r.status,
                    &r.body,
                    now / 1_000_000,
                    state,
                )
                .map_err(|_| Error::Protocol(q.clone()))?;
                let session_uid = work.session.then(|| state.config.as_ref().map(|c| c.venue_uid));
                if let Some(uid) = session_uid
                    && uid != self.state.config.as_ref().map(|c| c.venue_uid)
                {
                    let unavailable = uid == Some([0; 24])
                        || self.state.config.as_ref().is_some_and(|c| c.venue_uid == [0; 24]);
                    if !unavailable
                        || self.rd.account(self.account).uid_policy != hft_refdata::UidPolicy::Unverifiable
                    {
                        return Err(Error::Protocol(q.clone()));
                    }
                    if let Some(h) = &mut self.state.config {
                        h.venue_uid.fill(0);
                        h.uid_keys = 0;
                    }
                }
                if self.boot && matches!(q, Query::OpenOrders { .. }) {
                    if !self.state.orders.is_empty() {
                        self.cancel_rounds += 1;
                        if self.cancel_rounds > self.cfg.max_cancel_rounds {
                            return Err(Error::CancelNotQuiescent);
                        }
                        // Re-query from page one after cancelling this page: pagination must not
                        // skip entries when deletion changes the exchange's cursor window.
                        self.queue.push_front(Work {
                            query: Query::OpenOrders {
                                cursor: String::new(),
                            },
                            session: false,
                        });
                        for o in self.state.orders.iter().rev() {
                            self.queue.push_front(Work {
                                query: Query::Cancel {
                                    symbol: o.symbol.clone(),
                                    order_id: o.order_id.clone(),
                                },
                                session: false,
                            });
                        }
                    } else if !self.state.cursor.is_empty() {
                        return Err(Error::Protocol(q.clone()));
                    }
                }
            }
            self.next = now + i64::from(self.cfg.request_gap_ms) * 1_000_000;
            if self.queue.is_empty() {
                self.ready = true;
                self.last_poll = now;
            }
            return Ok(true);
        }
        if self.ready
            || self.funding_active
            || self.active.is_some()
            || self.order.is_some()
            || self.order_reply.is_some()
            || now < self.next
        {
            return Ok(false);
        }
        if self.queue.is_empty()
            && !self.boot
            && now - self.last_poll >= i64::from(self.cfg.interval_ms) * 1_000_000
        {
            self.settings();
            self.add(Query::Balances);
            if P::MARKET == MarketKind::LinearPerp {
                self.add(Query::Positions);
            }
        }

        if self.queue.is_empty()
            && P::USER_STREAM
            && !self.listen.is_empty()
            && now - self.last_keepalive >= i64::from(self.cfg.listen_keepalive_ms) * 1_000_000
        {
            self.add(Query::Keepalive);
        }
        if self.queue.is_empty()
            && !self.boot
            && let Some(poll) = &mut self.funding
        {
            let clock = hft_venue_proto::WallClock {
                unix_ms: now / 1_000_000,
                unix_us: now / 1000,
            };
            if let Some(req) = poll
                .request(&self.venue, &self.rest, &clock)
                .map_err(|_| Error::Funding(crate::FundingPollErr::Config))?
            {
                self.send(&req, now)?;
                self.funding_active = true;
                return Ok(true);
            }
        }
        let Some(w) = self.queue.pop_front() else {
            return Ok(false);
        };
        let key = if w.session { &self.session } else { &self.rest };
        let mut req = RestReqBuf::default();
        P::account_request(&self.venue, key, &w.query, now / 1_000_000, &mut req)
            .map_err(|_| Error::Protocol(w.query.clone()))?;
        self.send(&req, now)?;
        self.active = Some(w);
        Ok(true)
    }
    fn send(&mut self, req: &RestReqBuf, now: i64) -> Result<(), Error> {
        let method = match req.method {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
        };
        let mut wire=format!("{method} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept-Encoding: identity\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",std::str::from_utf8(req.path_query.as_slice()).map_err(|_|Error::Transport(hft_net::rest::Error::Request))?,self.host,req.body.len()).into_bytes();
        wire.extend_from_slice(req.headers.as_slice());
        wire.extend_from_slice(b"\r\n");
        wire.extend_from_slice(req.body.as_slice());
        let result = self.transport.start(
            BuildReq {
                venue: P::VENUE,
                conn: ConnId(240),
                role: ConnRole::Rest,
                endpoint: EndpointId(0),
                src_ip_idx: 0,
                rx_queue: 0,
                deadline_ns: now + i64::from(self.cfg.timeout_ms) * 1_000_000,
            },
            &wire,
        );
        wire.fill(0);
        result.map_err(Error::Transport)?;
        Ok(())
    }
    /// After a runtime observation failure, discard the partial batch and retry after the
    /// configured interval. The caller publishes an unreadable configuration gate meanwhile.
    pub fn retry(&mut self, now: i64) {
        self.funding_active = false;
        if let Some(poll) = &mut self.funding {
            poll.retry(now / 1_000_000);
        }
        self.queue.clear();
        self.active = None;
        self.state = State::default();
        self.ready = false;
        self.last_poll = 0;
        self.next = now + i64::from(self.cfg.interval_ms) * 1_000_000;
    }
}
impl<P: AccountProtocol, T: Transport> Drop for Client<P, T> {
    fn drop(&mut self) {
        self.listen.fill(0);
        self.rest.secret.0.clear();
        self.session.secret.0.clear();
    }
}
