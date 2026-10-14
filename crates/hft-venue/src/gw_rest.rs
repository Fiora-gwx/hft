//! Gateway-owned persistent REST lanes. Responses re-enter the same account decoder as WS.
use super::*;
use hft_net::{RestErr, RestPool};
use hft_venue_proto::{ReqRef, RestOrigin, RestReqBuf, RestWhat, RoKind, RoQuery};
pub(super) enum HttpError {
    Net(RestErr),
    Rate,
}
impl From<RestErr> for HttpError {
    fn from(e: RestErr) -> Self {
        Self::Net(e)
    }
}
#[derive(Clone, Copy)]
pub(super) struct Pending {
    tag: u64,
    cmd: GwCmd,
    origin: RestOrigin,
    identity: Option<ClientOrderId>,
    conn: ConnId,
}
pub(super) struct Pools {
    pub endpoints: Vec<(EndpointId, RestPool<2>)>,
    pending: Vec<Pending>,
    tag: u64,
    last_idle: u64,
}
impl<P: VenueProtocol, const CAP: usize> GwRuntime<P, CAP> {
    /// Startup only: endpoint order must match the protocol's RestReqBuf.endpoint table.
    pub fn set_rest_pools(
        &mut self,
        account: AccountId,
        endpoints: Vec<(EndpointId, RestPool<2>)>,
    ) -> Result<(), InitErr> {
        let a = self.acct_ix(account).ok_or(InitErr::DepsMismatch)?;
        if endpoints.is_empty() || endpoints.len() > crate::MAX_REST_ENDPOINTS || self.accts[a].http.is_some()
        {
            return Err(InitErr::DepsMismatch);
        }
        self.accts[a].http = Some(Pools {
            pending: Vec::with_capacity(endpoints.len() * 2),
            endpoints,
            tag: 0,
            last_idle: 0,
        });
        Ok(())
    }
    pub(super) fn submit_http(
        &mut self,
        a: usize,
        c: GwCmd,
        origin: RestOrigin,
        wire: &RestReqBuf,
        now: u64,
    ) -> Result<ConnId, HttpError> {
        let weight = self
            .rd
            .rate_limits(P::VENUE)
            .iter()
            .filter(|l| l.bucket == hft_refdata::LimitBucket::Weight)
            .map(|l| u32::from(l.default_cost.max(1)))
            .max()
            .unwrap_or(1);
        if !self.rest_available(a, weight, true, now) {
            return Err(HttpError::Rate);
        }
        let identity = origin.req.map(|r| {
            let epoch = if c.op == GwOp::Place {
                self.epoch
            } else {
                self.mirror.get(r.slot as usize).map_or(self.epoch, |m| m.epoch)
            };
            self.cid(c.account, r.slot, r.r#gen, epoch)
        });
        let x = &mut self.accts[a];
        let Some(http) = &mut x.http else {
            return Err(RestErr::NoConn.into());
        };
        if http.pending.len() == http.pending.capacity() {
            return Err(RestErr::Busy.into());
        }
        let tag = http.tag.wrapping_add(1);
        let pool = http
            .endpoints
            .get_mut(usize::from(wire.endpoint))
            .ok_or(RestErr::Proto)?;
        let conn = crate::rest::submit(&mut pool.1, wire, tag, now)?;
        http.tag = tag;
        http.pending.push(Pending {
            tag,
            cmd: c,
            origin,
            identity,
            conn,
        });
        self.rest_charge(a, weight);
        // Signed content is omitted, including the query string and authentication headers.
        let mut meta = [0; 1025];
        meta[0] = wire.method as u8;
        let path = wire
            .path_query
            .as_slice()
            .split(|b| *b == b'?')
            .next()
            .unwrap_or(&[]);
        meta[1..1 + path.len()].copy_from_slice(path);
        self.raw.tx(
            conn,
            what::REST_RESPONSE,
            RawHdr::REDACTED,
            &meta[..1 + path.len()],
            now,
        );
        Ok(conn)
    }
    pub(super) fn order_http(&mut self, a: usize, c: GwCmd, target: TargetRef, now: u64) {
        let mut wire = RestReqBuf::default();
        let clk = self.wall(now);
        let tg = self.target_gen(&c);
        if P::encode_rest(self.rd, &self.accts[a].tpl, &c, &target, &clk, &mut wire).is_err() {
            self.report(
                &c,
                tg,
                NO_CONN,
                if c.op == GwOp::Query {
                    SendStatus::QueryUnsupported
                } else {
                    SendStatus::EncodeError
                },
                0,
                0,
            );
            return;
        }
        let origin = RestOrigin {
            what: RestWhat::Order,
            req: Some(ReqRef {
                slot: target.id.slot,
                r#gen: target.id.r#gen,
                op: c.op,
            }),
            inst: c.inst,
        };
        match self.submit_http(a, c, origin, &wire, now) {
            Ok(conn) => {
                if c.op == GwOp::Place {
                    self.write_mirror(&c);
                }
                self.report(&c, tg, conn, SendStatus::Written, 0, now);
            }
            Err(HttpError::Net(RestErr::Send(SendErr::Io(e)))) => {
                if c.op == GwOp::Place {
                    self.write_mirror(&c);
                }
                self.report(&c, tg, NO_CONN, SendStatus::WriteError, e, now);
            }
            Err(HttpError::Rate) => self.report(&c, tg, NO_CONN, SendStatus::RateLimited, 0, 0),
            Err(_) => self.report(&c, tg, NO_CONN, SendStatus::NoConn, 0, 0),
        }
    }
    pub(super) fn poll_http(&mut self, now: u64) -> bool {
        let idle_ticks = self.t.ms(u64::from(self.cfg.rest_idle_check_ms));
        let mut did = false;
        for a in 0..self.accts.len() {
            let n = self.accts[a].http.as_ref().map_or(0, |h| h.endpoints.len());
            for endpoint in 0..n {
                if self.out.free_hint() <= SINK_CAP || !self.pend_rep.is_empty() {
                    return did;
                }
                let mut reply = None;
                if let Some(h) = &mut self.accts[a].http {
                    if now.saturating_sub(h.last_idle) >= idle_ticks {
                        h.endpoints[endpoint].1.idle_check(now);
                    }
                    if let Some(result) = h.endpoints[endpoint].1.poll(now) {
                        let tag = match &result {
                            Ok(r) => r.tag,
                            Err(r) => r.tag,
                        };
                        if let Some(i) = h.pending.iter().position(|p| p.tag == tag) {
                            let p = h.pending.remove(i);
                            let mut r = crate::order_rest::Reply {
                                account: p.cmd.account,
                                cmd: p.cmd,
                                origin: p.origin,
                                identity: p.identity,
                                status: 0,
                                len: 0,
                                body: [0; 8192],
                            };
                            let mut ts = hft_net::Timestamps {
                                rx_tsc: now,
                                ..Default::default()
                            };
                            if let Ok(resp) = result {
                                ts = resp.ts;
                                self.raw.rx(
                                    p.conn,
                                    what::REST_RESPONSE,
                                    0,
                                    0,
                                    ts.best_rx().0,
                                    ts.flags,
                                    resp.body,
                                    ts.rx_tsc,
                                );
                                if resp.body.len() <= r.body.len() {
                                    r.status = resp.status;
                                    r.len = resp.body.len() as u16;
                                    r.body[..resp.body.len()].copy_from_slice(resp.body);
                                }
                            }
                            reply = Some((r, p.conn, ts));
                        }
                        did = true;
                    }
                    while let Some(m) = h.endpoints[endpoint].1.take_retired() {
                        self.ids.free(m.id());
                        if self.retire.try_push(m).is_err() {
                            self.retire_pend.push_back(m);
                        }
                    }
                }
                if let Some((r, conn, ts)) = reply {
                    let failed = r.status == 0 || !(200..300).contains(&r.status);
                    if failed && r.origin.what != RestWhat::Order {
                        self.conn_state(
                            r.account,
                            if r.origin.what == RestWhat::Keepalive {
                                ConnRole::UserStream
                            } else {
                                ConnRole::Order
                            },
                            conn,
                            ConnStateKind::Degraded,
                            codes::DEADMAN,
                            now,
                        );
                    }
                    self.decode_rest_reply(r, conn, ts, now);
                }
                self.drain_out(now);
            }
            if let Some(h) = &mut self.accts[a].http
                && now.saturating_sub(h.last_idle) >= idle_ticks
            {
                h.last_idle = now;
            }
        }
        did
    }
    pub(super) fn build_http(&mut self, now: u64) -> bool {
        if self.building.iter().any(Option::is_some) {
            return false;
        }
        for a in 0..self.accts.len() {
            let x = &mut self.accts[a];
            let Some(h) = &mut x.http else { continue };
            for (ep, (endpoint, pool)) in h.endpoints.iter_mut().enumerate() {
                if pool.next_build(now).is_none() {
                    continue;
                }
                if let Some(id) = self.ids.alloc() {
                    let req = BuildReq {
                        venue: P::VENUE,
                        conn: id,
                        role: ConnRole::Rest,
                        endpoint: *endpoint,
                        src_ip_idx: x.src_ip_idx,
                        rx_queue: 0,
                        deadline_ns: i64::MAX,
                    };
                    if self.build.try_push(req).is_ok() {
                        self.building[id.0 as usize] = Some((a, ConnRole::Rest, ep));
                        return true;
                    }
                    self.ids.free(id);
                }
                pool.build_failed();
                return false;
            }
        }
        false
    }
    pub(super) fn keepalive(&mut self, c: GwCmd, now: u64) {
        let Some(a) = self.acct_ix(c.account) else {
            self.n.cmd_bad_account += 1;
            return;
        };
        let mut wire = RestReqBuf::default();
        let clk = self.wall(now);
        let result = P::session_keepalive_request(self.rd, &self.accts[a].tpl, &clk, &mut wire);
        if result == Ok(false) {
            return;
        }
        let origin = RestOrigin {
            what: RestWhat::Keepalive,
            req: None,
            inst: InstId(0),
        };
        if result.is_err() || self.submit_http(a, c, origin, &wire, now).is_err() {
            self.conn_state(
                c.account,
                ConnRole::UserStream,
                NO_CONN,
                ConnStateKind::Degraded,
                codes::DEADMAN,
                now,
            );
        }
    }
    pub(super) fn sweep(&mut self, a: usize, c: GwCmd, now: u64) {
        let mut wire = RestReqBuf::default();
        let clk = self.wall(now);
        let query = RoQuery {
            kind: RoKind::OpenOrders,
            inst: c.inst,
            cursor: 0,
            since_ms: 0,
            limit: 0,
        };
        let result = P::ro_request(self.rd, &self.accts[a].tpl, c.account, &query, &clk, &mut wire);
        let origin = RestOrigin {
            what: RestWhat::Sweep,
            req: None,
            inst: c.inst,
        };
        if result.is_err() || self.submit_http(a, c, origin, &wire, now).is_err() {
            self.conn_state(
                c.account,
                ConnRole::Rest,
                NO_CONN,
                ConnStateKind::Degraded,
                codes::ACCT_QUERY,
                now,
            );
        }
    }
}
