//! Rust client (cold processes): one connection per request, frames per hft-ctl-proto.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use hft_ctl_proto::{
    AccountSel, ChatType, CmdBody, CmdRequest, CmdResponse, PROTO_VERSION, ResultCode, TransportStatus, Uuid,
    encode_frame, frame_len,
};
use hft_event::WARMUP_MAX_PAYLOAD;

/// Default request lifetime.
pub const DEFAULT_EXPIRY_NS: i64 = 30_000_000_000;
/// Default wait for a response.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Client errors.
#[derive(Debug)]
pub enum ClientError {
    Io(std::io::Error),
    Timeout,
    /// The response frame is not a valid response.
    Protocol(String),
    /// `result(cmd_id)` for a cmd_id this process never sent.
    UnknownCmdId,
    /// A warmup chunk is longer than `WARMUP_MAX_PAYLOAD`.
    ChunkTooLong,
    /// The gateway or sequencer rejected the request.
    Rejected(Box<CmdResponse>),
}

/// The client.
#[derive(Debug)]
pub struct CtlClient {
    socket: PathBuf,
    issuer: String,
    chat_type: Option<ChatType>,
    sent: Mutex<Vec<(Uuid, Vec<u8>)>>,
    retry_backoff: Duration,
}

static SEQ: AtomicU64 = AtomicU64::new(0);

fn now_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as i64)
}

/// A fresh cmd_id (time, process and counter; RFC 4122 version 4 variant bits).
pub fn new_cmd_id() -> Uuid {
    let hi = (now_ns() as u64) ^ (u64::from(std::process::id()) << 40);
    let lo = SEQ.fetch_add(1, Ordering::Relaxed) ^ ((now_ns() as u64).rotate_left(17));
    let mut b = [0u8; 16];
    b[..8].copy_from_slice(&hi.to_be_bytes());
    b[8..].copy_from_slice(&lo.to_be_bytes());
    b[6] = (b[6] & 0x0F) | 0x40;
    b[8] = (b[8] & 0x3F) | 0x80;
    Uuid::from_bytes(b)
}

impl CtlClient {
    pub fn new(socket: impl Into<PathBuf>, issuer: impl Into<String>) -> CtlClient {
        CtlClient {
            socket: socket.into(),
            issuer: issuer.into(),
            chat_type: None,
            sent: Mutex::new(Vec::new()),
            retry_backoff: Duration::from_millis(20),
        }
    }

    /// Chat type sent with `tg:*` issuers.
    pub fn with_chat_type(mut self, c: ChatType) -> CtlClient {
        self.chat_type = Some(c);
        self
    }

    /// A request skeleton of this client: new cmd_id, now, +30 s.
    pub fn request(&self, account: AccountSel, cmd: CmdBody) -> CmdRequest {
        let t = now_ns();
        CmdRequest {
            v: PROTO_VERSION,
            cmd_id: new_cmd_id(),
            issuer: self.issuer.clone(),
            issued_at_ns: t,
            expires_at_ns: t + DEFAULT_EXPIRY_NS,
            account,
            preview: false,
            confirmed: false,
            expect_state_version: None,
            view_account: None,
            chat_type: self.chat_type,
            cmd,
        }
    }

    fn exchange(&self, json: &[u8], timeout: Duration) -> Result<CmdResponse, ClientError> {
        let deadline = Instant::now() + timeout;
        let mut s = UnixStream::connect(&self.socket).map_err(ClientError::Io)?;
        let mut f = Vec::with_capacity(json.len() + 4);
        encode_frame(json, &mut f);
        s.set_write_timeout(Some(timeout)).map_err(ClientError::Io)?;
        s.write_all(&f).map_err(ClientError::Io)?;
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            if let Some(h) = buf.get(..4)
                && let Ok(n) = frame_len(&[h[0], h[1], h[2], h[3]])
                && buf.len() >= 4 + n
            {
                return CmdResponse::from_json(&buf[4..4 + n])
                    .map_err(|e| ClientError::Protocol(e.to_string()));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(ClientError::Timeout);
            }
            s.set_read_timeout(Some(left)).map_err(ClientError::Io)?;
            match s.read(&mut chunk) {
                Ok(0) => {
                    return Err(ClientError::Protocol(
                        "connection closed before a response".into(),
                    ));
                }
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Err(ClientError::Timeout);
                }
                Err(e) => return Err(ClientError::Io(e)),
            }
        }
    }

    /// Sends one request and waits up to `timeout` for its response.
    pub fn send(&self, cmd: &CmdRequest, timeout: Duration) -> Result<CmdResponse, ClientError> {
        let json = cmd.to_json();
        if let Ok(mut s) = self.sent.lock() {
            s.retain(|(id, _)| *id != cmd.cmd_id);
            s.push((cmd.cmd_id, json.clone()));
        }
        self.exchange(&json, timeout)
    }

    /// Sends `cmd` as a preview (`preview = true`, not confirmed).
    pub fn preview(&self, cmd: &CmdRequest) -> Result<CmdResponse, ClientError> {
        let mut c = cmd.clone();
        c.preview = true;
        c.confirmed = false;
        c.expect_state_version = None;
        self.send(&c, DEFAULT_TIMEOUT)
    }

    /// The confirmation: new cmd_id, same command, `confirmed = true`; `state_version` = the
    /// preview's `ctl_state_version` (`None` for the Viewer emergency stop).
    pub fn confirm(&self, cmd: &CmdRequest, state_version: Option<u64>) -> Result<CmdResponse, ClientError> {
        let mut c = cmd.clone();
        c.cmd_id = new_cmd_id();
        c.preview = false;
        c.confirmed = true;
        c.expect_state_version = state_version;
        self.send(&c, DEFAULT_TIMEOUT)
    }

    /// Re-sends the original request of `cmd_id` byte for byte (the duplicate-cmd_id rule).
    pub fn result(&self, cmd_id: Uuid) -> Result<CmdResponse, ClientError> {
        let json = self
            .sent
            .lock()
            .ok()
            .and_then(|s| s.iter().find(|(id, _)| *id == cmd_id).map(|(_, j)| j.clone()))
            .ok_or(ClientError::UnknownCmdId)?;
        self.exchange(&json, DEFAULT_TIMEOUT)
    }

    /// The schema JSON of a strategy kind.
    pub fn get_schema(&self, kind: &str) -> Result<serde_json::Value, ClientError> {
        let req = self.request(
            AccountSel::Missing,
            CmdBody::GetSchema {
                strategy_kind: Some(kind.to_owned()),
                kind_id: None,
            },
        );
        let r = self.send(&req, DEFAULT_TIMEOUT)?;
        if r.code != Some(ResultCode::Applied) {
            return Err(ClientError::Rejected(Box::new(r)));
        }
        serde_json::from_str(r.body.get()).map_err(|e| ClientError::Protocol(e.to_string()))
    }

    /// Sends the producer's chunks in order (`chunk_no` 0.., `last` on the final one); retries
    /// `busy`/`RateLimited` with the same cmd_id, stops at the first rejected chunk. Returns the
    /// number of applied chunks.
    pub fn inject_warmup(
        &self,
        instance: &str,
        local_kind: u16,
        chunks: &[&[u8]],
    ) -> Result<u32, ClientError> {
        if chunks.iter().any(|c| c.len() > WARMUP_MAX_PAYLOAD) {
            return Err(ClientError::ChunkTooLong);
        }
        let mut done = 0u32;
        for (k, c) in chunks.iter().enumerate() {
            let req = self.request(
                AccountSel::Missing,
                CmdBody::InjectWarmup {
                    strategy: instance.to_owned(),
                    local_kind: local_kind.to_string(),
                    chunk_no: k.to_string(),
                    last: Some(k + 1 == chunks.len()),
                    payload_b64: base64::engine::general_purpose::STANDARD.encode(c),
                    account: None,
                },
            );
            let mut r = self.send(&req, DEFAULT_TIMEOUT)?;
            loop {
                let retry = r.status == TransportStatus::Busy
                    || r.status == TransportStatus::Accepted
                    || r.code == Some(ResultCode::RateLimited);
                if !retry {
                    break;
                }
                std::thread::sleep(self.retry_backoff);
                r = self.result(req.cmd_id)?;
            }
            if r.code != Some(ResultCode::Applied) {
                return Ok(done);
            }
            done += 1;
        }
        Ok(done)
    }
}
