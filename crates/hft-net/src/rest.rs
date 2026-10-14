//! Warm HTTP/1.1 requests over the same verified TLS transport. One bounded request at a time.
//! No retries: a timeout after a write does not prove that a mutation failed.
use crate::{BuildEvent, BuildReq, ConnPhase, Connector, JobId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Busy,
    Request,
    Transport,
    Timeout,
    Framing,
    TooLarge,
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

pub struct Client {
    connector: Connector,
    pending: Option<(JobId, i64, bool)>,
    request: Vec<u8>,
    received: Vec<u8>,
    max_body: usize,
}
impl core::fmt::Debug for Client {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RestClient")
            .field("busy", &self.pending.is_some())
            .finish()
    }
}
impl Client {
    pub fn new(connector: Connector, max_body: usize) -> Self {
        Self {
            connector,
            pending: None,
            request: Vec::new(),
            received: Vec::new(),
            max_body,
        }
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn start(&mut self, req: BuildReq, request: &[u8]) -> Result<(), Error> {
        if self.busy() {
            return Err(Error::Busy);
        }
        if req.role != crate::ConnRole::Rest || request.len() > 16_384 {
            return Err(Error::Request);
        }
        let job = self.connector.submit(req).map_err(|_| Error::Transport)?;
        self.request.clear();
        self.request.extend_from_slice(request);
        self.received.clear();
        self.pending = Some((job, req.deadline_ns, false));
        Ok(())
    }
    /// One bounded nonblocking turn. All errors are deliberately free of URLs, headers and bodies.
    pub fn poll(&mut self, now_ns: i64) -> Option<Result<Response, Error>> {
        let (job, deadline, sent) = self.pending?;
        let result = if now_ns >= deadline {
            Some(Err(Error::Timeout))
        } else {
            match self.connector.drive(now_ns) {
                Some(BuildEvent::Failed { .. }) => Some(Err(Error::Transport)),
                _ => self.read(job, sent),
            }
        };
        if result.is_some() {
            self.connector.cancel(job);
            self.pending = None;
            self.request.fill(0);
            self.received.fill(0);
        }
        result
    }
    fn read(&mut self, job: JobId, sent: bool) -> Option<Result<Response, Error>> {
        let c = self.connector.conn_mut(job)?;
        if !sent {
            if c.send_plain(&self.request).is_err() {
                return Some(Err(Error::Transport));
            }
            if let Some(p) = &mut self.pending {
                p.2 = true;
            }
        }
        if c.flush().is_err() {
            return Some(Err(Error::Transport));
        }
        c.poll_rx(0);
        for _ in 0..64 {
            let bytes = c.plaintext();
            if self.received.len().saturating_add(bytes.len()) > self.max_body.saturating_add(65_536) {
                return Some(Err(Error::TooLarge));
            }
            self.received.extend_from_slice(bytes);
            let n = bytes.len();
            c.consume_plain(n);
            if c.open_one() != Some(true) {
                break;
            }
        }
        match decode(&self.received, self.max_body) {
            Ok(Some(r)) => Some(Ok(r)),
            Ok(None) if c.phase() == ConnPhase::Dead => Some(Err(Error::Transport)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        if let Some((job, _, _)) = self.pending {
            self.connector.cancel(job);
        }
        self.request.fill(0);
        self.received.fill(0);
    }
}

/// Strict framing: no close-delimited bodies, conflicting length headers, compression or redirects.
pub fn decode(bytes: &[u8], max_body: usize) -> Result<Option<Response>, Error> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut response = httparse::Response::new(&mut headers);
    let at = match response.parse(bytes).map_err(|_| Error::Framing)? {
        httparse::Status::Partial => {
            return if bytes.len() > 32_768 {
                Err(Error::TooLarge)
            } else {
                Ok(None)
            };
        }
        httparse::Status::Complete(n) => n,
    };
    let status = response.code.ok_or(Error::Framing)?;
    let mut length = None;
    let mut chunked = false;
    for h in response.headers {
        if h.name.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return Err(Error::Framing);
            }
            length = Some(
                core::str::from_utf8(h.value)
                    .ok()
                    .and_then(|s| s.parse::<usize>().ok())
                    .ok_or(Error::Framing)?,
            );
        } else if h.name.eq_ignore_ascii_case("transfer-encoding") {
            if chunked || !h.value.eq_ignore_ascii_case(b"chunked") {
                return Err(Error::Framing);
            }
            chunked = true;
        } else if h.name.eq_ignore_ascii_case("content-encoding")
            && !h.value.eq_ignore_ascii_case(b"identity")
        {
            return Err(Error::Framing);
        }
    }
    if chunked && length.is_some() {
        return Err(Error::Framing);
    }
    let data = &bytes[at..];
    if let Some(n) = length {
        if n > max_body {
            return Err(Error::TooLarge);
        }
        return Ok(data.get(..n).map(|b| Response {
            status,
            body: b.to_vec(),
        }));
    }
    if !chunked {
        return if status == 204 {
            Ok(Some(Response {
                status,
                body: Vec::new(),
            }))
        } else {
            Err(Error::Framing)
        };
    }
    let (mut offset, mut body) = (0usize, Vec::new());
    loop {
        let tail = &data[offset..];
        let Some(end) = tail.windows(2).position(|w| w == b"\r\n") else {
            return Ok(None);
        };
        let line = core::str::from_utf8(&tail[..end]).map_err(|_| Error::Framing)?;
        let size = usize::from_str_radix(line.split(';').next().ok_or(Error::Framing)?, 16)
            .map_err(|_| Error::Framing)?;
        if size > max_body.saturating_sub(body.len()) {
            return Err(Error::TooLarge);
        }
        offset += end + 2;
        if size == 0 {
            let trailer = &data[offset..];
            if trailer.starts_with(b"\r\n") || trailer.windows(4).any(|w| w == b"\r\n\r\n") {
                return Ok(Some(Response { status, body }));
            }
            return Ok(None);
        }
        let end = offset
            .checked_add(size)
            .and_then(|n| n.checked_add(2))
            .ok_or(Error::TooLarge)?;
        let Some(chunk) = data.get(offset..end) else {
            return Ok(None);
        };
        if &chunk[size..] != b"\r\n" {
            return Err(Error::Framing);
        }
        body.extend_from_slice(&chunk[..size]);
        offset += size + 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_261_warm_chunked_fragmentation() {
        let wire = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n4\r\ndefg\r\n0\r\n\r\n";
        for n in 0..wire.len() {
            assert!(decode(&wire[..n], 7).unwrap().is_none(), "prefix {n}");
        }
        assert_eq!(decode(wire, 7).unwrap().unwrap().body, b"abcdefg");
        assert_eq!(decode(wire, 6).unwrap_err(), Error::TooLarge);
    }

    #[test]
    fn net_262_warm_rejects_ambiguous_framing() {
        for wire in [
            &b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nTransfer-Encoding: chunked\r\n\r\n"[..],
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nContent-Length: 0\r\n\r\n",
            b"HTTP/1.1 200 OK\r\n\r\n",
        ] {
            assert_eq!(decode(wire, 128).unwrap_err(), Error::Framing);
        }
        assert_eq!(
            decode(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\n",
                usize::MAX
            )
            .unwrap_err(),
            Error::TooLarge
        );
        assert!(
            decode(b"HTTP/1.1 204 No Content\r\n\r\n", 0)
                .unwrap()
                .unwrap()
                .body
                .is_empty()
        );
    }
}
