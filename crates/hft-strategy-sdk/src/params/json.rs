//! Minimal JSON reader and writer helpers for the canonical schema JSON (no serde, README
//! "依赖"). Reader: objects (key order kept), arrays, strings, integers, booleans, null;
//! fractional or exponent numbers are rejected (the canonical JSON has integers only).

/// Parsed JSON value (cold path only).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Int(i128),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub(crate) fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(kv) => kv.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    pub(crate) fn as_int(&self) -> Option<i128> {
        match self {
            Json::Int(v) => Some(*v),
            _ => None,
        }
    }
    pub(crate) fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub(crate) fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(a) => Some(a),
            _ => None,
        }
    }
}

/// Nesting limit (the canonical schema JSON nests 4 levels).
const MAX_DEPTH: u32 = 32;

struct Parser<'a> {
    b: &'a [u8],
    pos: usize,
}

/// Parses one JSON document (surrounding whitespace allowed, nothing else after it).
pub(crate) fn parse(s: &str) -> Option<Json> {
    let mut p = Parser {
        b: s.as_bytes(),
        pos: 0,
    };
    let v = p.value(0)?;
    p.ws();
    if p.pos == p.b.len() { Some(v) } else { None }
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }
    fn bump(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }
    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }
    fn lit(&mut self, word: &[u8]) -> Option<()> {
        let end = self.pos.checked_add(word.len())?;
        if self.b.get(self.pos..end)? == word {
            self.pos = end;
            Some(())
        } else {
            None
        }
    }
    fn value(&mut self, depth: u32) -> Option<Json> {
        if depth > MAX_DEPTH {
            return None;
        }
        self.ws();
        match self.peek()? {
            b'{' => self.object(depth),
            b'[' => self.array(depth),
            b'"' => self.string().map(Json::Str),
            b't' => self.lit(b"true").map(|_| Json::Bool(true)),
            b'f' => self.lit(b"false").map(|_| Json::Bool(false)),
            b'n' => self.lit(b"null").map(|_| Json::Null),
            b'-' | b'0'..=b'9' => self.int(),
            _ => None,
        }
    }
    fn object(&mut self, depth: u32) -> Option<Json> {
        self.bump()?;
        let mut kv = Vec::new();
        self.ws();
        if self.peek()? == b'}' {
            self.bump()?;
            return Some(Json::Obj(kv));
        }
        loop {
            self.ws();
            let k = self.string()?;
            self.ws();
            if self.bump()? != b':' {
                return None;
            }
            let v = self.value(depth + 1)?;
            kv.push((k, v));
            self.ws();
            match self.bump()? {
                b',' => {}
                b'}' => return Some(Json::Obj(kv)),
                _ => return None,
            }
        }
    }
    fn array(&mut self, depth: u32) -> Option<Json> {
        self.bump()?;
        let mut a = Vec::new();
        self.ws();
        if self.peek()? == b']' {
            self.bump()?;
            return Some(Json::Arr(a));
        }
        loop {
            a.push(self.value(depth + 1)?);
            self.ws();
            match self.bump()? {
                b',' => {}
                b']' => return Some(Json::Arr(a)),
                _ => return None,
            }
        }
    }
    fn int(&mut self) -> Option<Json> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        let digits = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        let n = self.pos - digits;
        if n == 0 || n > 39 || (n > 1 && self.b.get(digits) == Some(&b'0')) {
            return None;
        }
        if matches!(self.peek(), Some(b'.' | b'e' | b'E')) {
            return None;
        }
        let s = core::str::from_utf8(self.b.get(start..self.pos)?).ok()?;
        s.parse::<i128>().ok().map(Json::Int)
    }
    fn hex4(&mut self) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..4 {
            let d = char::from(self.bump()?).to_digit(16)?;
            v = v * 16 + d;
        }
        Some(v)
    }
    fn string(&mut self) -> Option<String> {
        if self.bump()? != b'"' {
            return None;
        }
        let mut out = Vec::new();
        loop {
            match self.bump()? {
                b'"' => return String::from_utf8(out).ok(),
                b'\\' => {
                    let c = match self.bump()? {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hi = self.hex4()?;
                            let cp = if (0xD800..0xDC00).contains(&hi) {
                                self.lit(b"\\u")?;
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return None;
                                }
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            } else {
                                hi
                            };
                            char::from_u32(cp)?
                        }
                        _ => return None,
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                c if c < 0x20 => return None,
                c => out.push(c),
            }
        }
    }
}

/// Appends `s` as a JSON string literal (`"`, `\` and control characters escaped; other
/// characters, including non-ASCII, written as UTF-8).
pub(crate) fn push_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
