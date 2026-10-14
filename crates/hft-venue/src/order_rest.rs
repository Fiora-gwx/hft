//! Fixed-size gateway ↔ warm REST worker messages. No secret crosses the gateway ring.
use hft_event::GwCmd;
use hft_types::{AccountId, ClientOrderId};
use hft_venue_proto::{RestOrigin, TargetRef};
#[derive(Clone, Copy, Debug)]
pub struct Request {
    pub cmd: GwCmd,
    pub target: TargetRef,
}
#[derive(Clone, Copy)]
pub struct Reply {
    pub account: AccountId,
    pub cmd: GwCmd,
    pub origin: RestOrigin,
    /// Captured when submitted; never reconstructed from a potentially reused slot.
    pub identity: Option<ClientOrderId>,
    pub status: u16,
    pub len: u16,
    pub body: [u8; 8192],
}
impl core::fmt::Debug for Reply {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RestReply")
            .field("account", &self.account)
            .field("status", &self.status)
            .field("len", &self.len)
            .finish()
    }
}
impl Reply {
    pub fn new(
        account: AccountId,
        cmd: GwCmd,
        origin: RestOrigin,
        identity: Option<ClientOrderId>,
        result: Result<hft_net::rest::Response, hft_net::rest::Error>,
    ) -> Self {
        let mut r = Self {
            account,
            cmd,
            origin,
            identity,
            status: 0,
            len: 0,
            body: [0; 8192],
        };
        if let Ok(response) = result
            && let Ok(n) = u16::try_from(response.body.len())
            && let Some(dst) = r.body.get_mut(..response.body.len())
        {
            dst.copy_from_slice(&response.body);
            r.status = response.status;
            r.len = n;
        }
        r
    }
}
