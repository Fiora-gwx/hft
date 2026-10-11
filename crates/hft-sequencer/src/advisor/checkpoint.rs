//! Cold, allocation-free decoding of the Model checkpoint format owned by hft-core.
//! File discovery, read limits before allocation, fsync and rename belong to the application.

use hft_types::{AccountId, StrategyId};

use super::{CkptHdr, ckpt_checksum};

pub const HEADER_LEN: usize = 64;
pub const MAGIC: [u8; 8] = *b"HFTCKPT1";

/// Persisted identity. Parameter versions, refdata versions and binary hashes are not keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub kind_id: u16,
    pub layout_version: u32,
    pub schema_id: u32,
    pub strategy: StrategyId,
    pub account: AccountId,
    pub last_seq: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileError {
    HeaderTruncated,
    Magic,
    Version,
    Length,
    SizeLimit,
    Reserved,
    Checksum,
    Path,
}

/// Borrowed validated bytes. Loading this object does not restore or create a Model.
#[derive(Clone, Copy, Debug)]
pub struct Decoded<'a> {
    pub key: Key,
    pub path_version: u32,
    pub path: &'a [u8],
    pub samples: &'a [u8],
    pub body: &'a [u8],
}

impl Decoded<'_> {
    /// Adapter for the advisor store. The caller obtains `last_ens` from the journal at
    /// `key.last_seq`; engine time is not an extra field in the checkpoint file.
    pub fn header(&self, last_ens: i64) -> CkptHdr {
        CkptHdr {
            strategy: self.key.strategy,
            account: self.key.account,
            kind_id: self.key.kind_id,
            schema_id: self.key.schema_id,
            layout_version: self.key.layout_version,
            path_version: self.path_version,
            last_seq: self.key.last_seq,
            last_ens,
            len: self.body.len() as u64,
            checksum: ckpt_checksum(self.body),
        }
    }
}

fn u16_at(b: &[u8], i: usize) -> Result<u16, FileError> {
    let a = b.get(i..i + 2).ok_or(FileError::Length)?;
    Ok(u16::from_le_bytes([a[0], a[1]]))
}
fn u32_at(b: &[u8], i: usize) -> Result<u32, FileError> {
    let a = b.get(i..i + 4).ok_or(FileError::Length)?;
    Ok(u32::from_le_bytes([a[0], a[1], a[2], a[3]]))
}
fn u64_at(b: &[u8], i: usize) -> Result<u64, FileError> {
    let a = b.get(i..i + 8).ok_or(FileError::Length)?;
    Ok(u64::from_le_bytes([
        a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7],
    ]))
}

/// Header CRC field is treated as zero. The caller has checked the 64 B header exists.
fn checksum(file: &[u8]) -> u32 {
    let c = crc32c::crc32c(&file[..40]);
    let c = crc32c::crc32c_append(c, &[0; 4]);
    crc32c::crc32c_append(c, &file[44..])
}

/// `PathSecHdr` is `{path_version: u32, path_len: u32}`, in that order.
pub(super) fn path_sections(body: &[u8]) -> Result<(u32, &[u8], &[u8]), FileError> {
    if body.len() < 8 {
        return Err(FileError::Path);
    }
    let version = u32_at(body, 0)?;
    let len = usize::try_from(u32_at(body, 4)?).map_err(|_| FileError::Path)?;
    let end = 8usize.checked_add(len).ok_or(FileError::Path)?;
    let path = body.get(8..end).ok_or(FileError::Path)?;
    if version == 0 && !path.is_empty() {
        return Err(FileError::Path);
    }
    Ok((version, path, &body[end..]))
}

/// Validate the entire file before exposing any body bytes. O(file size), no allocation.
/// `max_body_bytes` is supplied by the caller's resource limit, not by the file header.
pub fn decode(file: &[u8], max_body_bytes: usize) -> Result<Decoded<'_>, FileError> {
    if file.len() < HEADER_LEN {
        return Err(FileError::HeaderTruncated);
    }
    if file[..8] != MAGIC {
        return Err(FileError::Magic);
    }
    if u16_at(file, 8)? != 1 {
        return Err(FileError::Version);
    }
    let len = usize::try_from(u64_at(file, 32)?).map_err(|_| FileError::SizeLimit)?;
    if len > max_body_bytes {
        return Err(FileError::SizeLimit);
    }
    if len != file.len() - HEADER_LEN {
        return Err(FileError::Length);
    }
    if file[22..24].iter().chain(&file[44..64]).any(|b| *b != 0) {
        return Err(FileError::Reserved);
    }
    if u32_at(file, 40)? != checksum(file) {
        return Err(FileError::Checksum);
    }
    let body = &file[HEADER_LEN..];
    let (path_version, path, samples) = path_sections(body)?;
    Ok(Decoded {
        key: Key {
            kind_id: u16_at(file, 10)?,
            layout_version: u32_at(file, 12)?,
            schema_id: u32_at(file, 16)?,
            strategy: StrategyId(file[20]),
            account: AccountId(file[21]),
            last_seq: u64_at(file, 24)?,
        },
        path_version,
        path,
        samples,
        body,
    })
}

/// Serialize a completed advisor body. Its length and the file CRC are calculated from the
/// bytes, not trusted from `h.len`/`h.checksum` (which are not yet filled at store `begin`).
/// Cold path: allocates the 64 B header and one body; never reads or writes a file.
pub fn encode(h: &CkptHdr, body: &[u8], max_body_bytes: usize) -> Result<Vec<u8>, FileError> {
    if body.len() > max_body_bytes {
        return Err(FileError::SizeLimit);
    }
    let (version, _, _) = path_sections(body)?;
    if version != h.path_version {
        return Err(FileError::Path);
    }
    let len = HEADER_LEN.checked_add(body.len()).ok_or(FileError::SizeLimit)?;
    let mut file = vec![0; len];
    file[..8].copy_from_slice(&MAGIC);
    file[8..10].copy_from_slice(&1u16.to_le_bytes());
    file[10..12].copy_from_slice(&h.kind_id.to_le_bytes());
    file[12..16].copy_from_slice(&h.layout_version.to_le_bytes());
    file[16..20].copy_from_slice(&h.schema_id.to_le_bytes());
    file[20] = h.strategy.0;
    file[21] = h.account.0;
    file[24..32].copy_from_slice(&h.last_seq.to_le_bytes());
    file[32..40].copy_from_slice(&(body.len() as u64).to_le_bytes());
    file[HEADER_LEN..].copy_from_slice(body);
    let crc = checksum(&file);
    file[40..44].copy_from_slice(&crc.to_le_bytes());
    Ok(file)
}
