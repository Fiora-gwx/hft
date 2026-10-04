//! `DurableMeta`: per-instance durable key/value store (`<cold_root>/<instance>/meta/<key>`) —
//! README "持久元数据".

#![allow(clippy::disallowed_macros)] // allow-list: DurableMeta file I/O on cold threads

use std::fs::File;
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::config::InstanceName;
use crate::fs;

/// Durable `u64` values keyed by `[a-z0-9_]{1,64}` (cold path; may block).
#[derive(Debug)]
pub struct DurableMeta {
    dir: PathBuf,
}

fn check_key(key: &str) -> io::Result<()> {
    let ok = !key.is_empty()
        && key.len() <= 64
        && key
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_');
    if ok {
        Ok(())
    } else {
        Err(io::Error::new(ErrorKind::InvalidInput, "bad DurableMeta key"))
    }
}

fn bad(what: &str) -> io::Error {
    io::Error::new(
        ErrorKind::InvalidData,
        format!("corrupt DurableMeta file: {what}"),
    )
}

/// File content for value `v`: `"<v>\ncrc32c:<8 hex>\n"`.
fn render(v: u64) -> String {
    let s = v.to_string();
    format!("{s}\ncrc32c:{:08x}\n", crc32c::crc32c(s.as_bytes()))
}

fn parse(b: &[u8]) -> io::Result<u64> {
    let s = std::str::from_utf8(b).map_err(|_| bad("not UTF-8"))?;
    let rest = s.strip_suffix('\n').ok_or_else(|| bad("no final newline"))?;
    let (num, crc) = rest.split_once('\n').ok_or_else(|| bad("missing crc line"))?;
    let hex = crc
        .strip_prefix("crc32c:")
        .ok_or_else(|| bad("missing crc32c:"))?;
    if num.is_empty() || !num.bytes().all(|c| c.is_ascii_digit()) || (num.len() > 1 && num.starts_with('0')) {
        return Err(bad("value"));
    }
    if hex.len() != 8
        || !hex
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return Err(bad("crc format"));
    }
    let want = u32::from_str_radix(hex, 16).map_err(|_| bad("crc"))?;
    if crc32c::crc32c(num.as_bytes()) != want {
        return Err(bad("crc mismatch"));
    }
    num.parse::<u64>().map_err(|_| bad("value range"))
}

impl DurableMeta {
    /// Opens (creating `meta/` with mode 0770) the store of `instance` under `cold_root`.
    pub fn open(cold_root: &Path, instance: &InstanceName) -> Result<DurableMeta, io::Error> {
        let dir = cold_root.join(instance.as_str()).join("meta");
        fs::mkdir_0770(cold_root, &dir)?;
        Ok(DurableMeta { dir })
    }

    /// The value of `key`; `Ok(None)` when never written; corrupt file → error.
    pub fn read(&self, key: &str) -> Result<Option<u64>, io::Error> {
        check_key(key)?;
        match std::fs::read(self.dir.join(key)) {
            Ok(b) => parse(&b).map(Some),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Writes `key = v` durably: temp file → fsync → rename → fsync of the directory.
    pub fn write_sync(&self, key: &str, v: u64) -> Result<(), io::Error> {
        check_key(key)?;
        let tmp = self.dir.join(format!("{key}.tmp"));
        let dst = self.dir.join(key);
        {
            let mut f = File::create(&tmp)?;
            f.write_all(render(v).as_bytes())?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &dst)?;
        fs::sync_dir(&self.dir)
    }
}
