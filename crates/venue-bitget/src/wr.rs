//! Bounded byte writer for frames and requests (no allocation; overflow is sticky).

use hft_types::write_fixed;

/// Writes into a caller buffer; any push past its end sets `full` and writes nothing more.
pub(crate) struct Wr<'a> {
    buf: &'a mut [u8],
    n: usize,
    full: bool,
}

impl<'a> Wr<'a> {
    pub fn new(buf: &'a mut [u8]) -> Wr<'a> {
        Wr {
            buf,
            n: 0,
            full: false,
        }
    }

    /// Bytes written; `None` when the buffer overflowed.
    pub fn finish(&self) -> Option<usize> {
        (!self.full).then_some(self.n)
    }

    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        if self.full {
            return self;
        }
        let end = self.n.wrapping_add(b.len());
        match self.buf.get_mut(self.n..end) {
            Some(d) if end >= self.n => {
                d.copy_from_slice(b);
                self.n = end;
            }
            _ => self.full = true,
        }
        self
    }

    pub fn byte(&mut self, b: u8) -> &mut Self {
        self.bytes(&[b])
    }

    /// Decimal of an unsigned integer.
    pub fn u64(&mut self, v: u64) -> &mut Self {
        let mut d = [0u8; 20];
        let mut i = d.len();
        let mut x = v;
        loop {
            i = i.wrapping_sub(1);
            if let Some(c) = d.get_mut(i) {
                *c = b'0'.wrapping_add((x % 10) as u8);
            }
            x /= 10;
            if x == 0 || i == 0 {
                break;
            }
        }
        self.bytes(d.get(i..).unwrap_or(&[]))
    }

    /// Decimal of a signed integer.
    pub fn i64(&mut self, v: i64) -> &mut Self {
        if v < 0 {
            self.byte(b'-');
        }
        self.u64(v.unsigned_abs())
    }

    /// `raw × 10^exp` as the shortest decimal (hft-types `write_fixed`).
    pub fn fixed(&mut self, raw: i64, exp: i8) -> &mut Self {
        let mut t = [0u8; 40];
        let n = write_fixed(raw, exp, &mut t);
        if n == 0 {
            self.full = true;
            return self;
        }
        self.bytes(t.get(..n).unwrap_or(&[]))
    }
}
