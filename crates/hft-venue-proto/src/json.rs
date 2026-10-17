//! Allocation-free projection of ordinary wire JSON onto borrowed field slices.
//!
//! This is a conservative fast path, not a replacement JSON grammar. Escapes, non-ASCII
//! strings, exponents and deep nesting return `None`; callers must use their checked decoder.
//! Unknown values are syntax-scanned without constructing tokens or converting numbers.

/// Project an object in one pass. `field` maps a key to an output slot; unknown keys return None.
/// Duplicate selected keys, unsupported syntax, trailing bytes or invalid slot numbers decline
/// the fast path. Missing fields are empty slices. Work is bounded by input length, depth by 16.
#[inline]
pub fn project<const N: usize>(input: &[u8], field: impl Fn(&[u8]) -> Option<usize>) -> Option<[&[u8]; N]> {
    let mut c = Cursor {
        input,
        at: 0,
        visited: 0,
    };
    let mut fields = [&[][..]; N];
    c.byte(b'{')?;
    if c.peek() != Some(b'}') {
        loop {
            let key = c.string()?;
            c.byte(b':')?;
            c.space();
            let start = c.at;
            c.skip(0)?;
            if let Some(i) = field(key) {
                let slot = fields.get_mut(i)?;
                if !slot.is_empty() {
                    return None;
                }
                *slot = input.get(start..c.at)?;
            }
            match c.peek()? {
                b',' => c.at += 1,
                b'}' => break,
                _ => return None,
            }
        }
    }
    c.byte(b'}')?;
    c.space();
    (c.at == input.len()).then_some(fields)
}

/// A schema's action for one object key. States are 0..16; leaf slots are 0..32.
#[derive(Clone, Copy, Debug)]
pub enum Select {
    Skip,
    Raw(usize),
    Object(u8),
    First(u8),
    /// Exactly one object in the array; otherwise decline to the reference decoder.
    One(u8),
}

/// Project selected nested objects in one traversal. `First` selects only the first object in
/// an array and syntax-scans remaining entries. Duplicate selected keys/aliases decline the
/// fast path, including duplicate container keys. Same fallback contract as `project`.
#[inline]
pub fn nested<const N: usize>(
    input: &[u8],
    required_states: u16,
    select: impl Fn(u8, &[u8]) -> Select,
) -> Option<[&[u8]; N]> {
    let mut c = Cursor {
        input,
        at: 0,
        visited: 0,
    };
    let mut fields = [&[][..]; N];
    c.object(0, &select, &mut fields, 0)?;
    c.space();
    (c.at == input.len() && c.visited & required_states == required_states).then_some(fields)
}

/// Borrow a plain string from a projected field. No unescaping, copying or allocation.
#[inline]
pub fn string(raw: &[u8]) -> Option<&str> {
    let inner = raw.strip_prefix(b"\"")?.strip_suffix(b"\"")?;
    core::str::from_utf8(inner).ok()
}

/// JSON unsigned integer, with overflow checked directly on the wire bytes.
#[inline]
pub fn uint(raw: &[u8]) -> Option<u64> {
    if raw.is_empty() {
        return None;
    }
    let mut n = 0u64;
    if raw.len() <= 19 {
        // At most 19 decimal digits fit in u64. The length proof removes a carry dependency
        // from every digit while retaining syntax validation for direct callers.
        for &b in raw {
            if !b.is_ascii_digit() {
                return None;
            }
            n = n.wrapping_mul(10).wrapping_add(u64::from(b - b'0'));
        }
        return Some(n);
    }
    for &b in raw {
        if !b.is_ascii_digit() {
            return None;
        }
        n = n.checked_mul(10)?.checked_add(u64::from(b - b'0'))?;
    }
    Some(n)
}

/// Compact objects in the exchange's usual field order. Keys include their quotes and colon.
/// Any order/whitespace/extra-field variation declines to `project` or the checked decoder.
#[inline]
pub fn ordered<'a, const N: usize>(input: &'a [u8], keys: &[&[u8]; N]) -> Option<[&'a [u8]; N]> {
    let mut c = Cursor {
        input,
        at: 1,
        visited: 0,
    };
    if input.first() != Some(&b'{') {
        return None;
    }
    let mut fields = [&[][..]; N];
    for (slot, key) in fields.iter_mut().zip(keys) {
        if !input.get(c.at..)?.starts_with(key) {
            return None;
        }
        c.at += key.len();
        let start = c.at;
        c.skip(0)?;
        *slot = input.get(start..c.at)?;
        let separator = input.get(c.at)?;
        c.at += 1;
        if *separator != b',' && *separator != b'}' {
            return None;
        }
    }
    if input.get(c.at.checked_sub(1)?) != Some(&b'}') {
        return None;
    }
    c.space();
    (c.at == input.len()).then_some(fields)
}

/// JSON signed integer. Values outside i64 decline the fast path.
#[inline]
pub fn int(raw: &[u8]) -> Option<i64> {
    if let Some(rest) = raw.strip_prefix(b"-") {
        let n = uint(rest)?;
        if n == (1u64 << 63) {
            Some(i64::MIN)
        } else {
            i64::try_from(n).ok()?.checked_neg()
        }
    } else {
        i64::try_from(uint(raw)?).ok()
    }
}

struct Cursor<'a> {
    input: &'a [u8],
    at: usize,
    visited: u16,
}

impl<'a> Cursor<'a> {
    fn object<const N: usize>(
        &mut self,
        state: u8,
        select: &impl Fn(u8, &[u8]) -> Select,
        fields: &mut [&'a [u8]; N],
        depth: u8,
    ) -> Option<()> {
        if depth >= 16 {
            return None;
        }
        self.byte(b'{')?;
        self.visited |= 1u16 << state;
        let mut seen = 0u64;
        if self.peek()? != b'}' {
            loop {
                let key = self.string()?;
                self.byte(b':')?;
                self.space();
                let action = select(state, key);
                let tag = match action {
                    Select::Skip => None,
                    Select::Raw(i) if i < 32 => Some(i),
                    Select::Object(s) if s < 16 => Some(32 + usize::from(s)),
                    Select::First(s) | Select::One(s) if s < 16 => Some(48 + usize::from(s)),
                    _ => return None,
                };
                if let Some(tag) = tag {
                    let bit = 1u64 << tag;
                    if seen & bit != 0 {
                        return None;
                    }
                    seen |= bit;
                }
                match action {
                    Select::Skip => self.skip(depth)?,
                    Select::Raw(i) => {
                        let start = self.at;
                        self.skip(depth)?;
                        *fields.get_mut(i)? = self.input.get(start..self.at)?;
                    }
                    Select::Object(s) => self.object(s, select, fields, depth + 1)?,
                    Select::First(s) | Select::One(s) => {
                        self.byte(b'[')?;
                        self.object(s, select, fields, depth + 1)?;
                        while self.peek()? == b',' {
                            if matches!(action, Select::One(_)) {
                                return None;
                            }
                            self.at += 1;
                            self.skip(depth + 1)?;
                        }
                        self.byte(b']')?;
                    }
                }
                match self.peek()? {
                    b',' => self.at += 1,
                    b'}' => break,
                    _ => return None,
                }
            }
        }
        self.byte(b'}')
    }

    #[inline]
    fn space(&mut self) {
        while matches!(self.input.get(self.at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    #[inline]
    fn peek(&mut self) -> Option<u8> {
        self.space();
        self.input.get(self.at).copied()
    }

    #[inline]
    fn byte(&mut self, b: u8) -> Option<()> {
        if self.peek()? != b {
            return None;
        }
        self.at += 1;
        Some(())
    }

    #[inline]
    fn string(&mut self) -> Option<&'a [u8]> {
        self.byte(b'"')?;
        let rest = self.input.get(self.at..)?;
        let n = memchr::memchr(b'"', rest)?;
        let value = rest.get(..n)?;
        // Escaped keys/values must go through the reference decoder. ASCII checks also reject
        // unescaped control characters and keep unknown-field UTF-8 validation identical.
        if value.iter().any(|&b| !(0x20..0x80).contains(&b) || b == b'\\') {
            return None;
        }
        self.at += n + 1;
        Some(value)
    }

    #[inline]
    fn skip(&mut self, depth: u8) -> Option<()> {
        match self.peek()? {
            b'"' => {
                self.string()?;
            }
            b'{' | b'[' if depth < 16 => {
                let object = self.input.get(self.at) == Some(&b'{');
                let end = if object { b'}' } else { b']' };
                self.at += 1;
                if self.peek()? != end {
                    loop {
                        if object {
                            self.string()?;
                            self.byte(b':')?;
                        }
                        self.skip(depth + 1)?;
                        match self.peek()? {
                            b',' => self.at += 1,
                            b if b == end => break,
                            _ => return None,
                        }
                    }
                }
                self.byte(end)?;
            }
            b't' => self.literal(b"true")?,
            b'f' => self.literal(b"false")?,
            b'n' => self.literal(b"null")?,
            b'-' | b'0'..=b'9' => {
                if self.input.get(self.at) == Some(&b'-') {
                    self.at += 1;
                }
                match self.input.get(self.at)? {
                    b'0' => self.at += 1,
                    b'1'..=b'9' => self.digits()?,
                    _ => return None,
                }
                if self.input.get(self.at) == Some(&b'.') {
                    self.at += 1;
                    self.digits()?;
                }
                // Exponents (including overflowing ignored numbers) use the checked fallback.
            }
            _ => return None,
        }
        Some(())
    }

    #[inline]
    fn literal(&mut self, word: &[u8]) -> Option<()> {
        if !self.input.get(self.at..)?.starts_with(word) {
            return None;
        }
        self.at += word.len();
        Some(())
    }

    #[inline]
    fn digits(&mut self) -> Option<()> {
        let start = self.at;
        while self.input.get(self.at).is_some_and(u8::is_ascii_digit) {
            self.at += 1;
        }
        (self.at > start).then_some(())
    }
}
