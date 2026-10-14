//! Lossless numeric error codes and the contract-8 opaque string / HTTP namespaces.
pub const RAW_CODE_NONE: u32 = 0;
pub const fn raw_code_num(code: i32) -> u32 {
    code as u32
}
pub const fn raw_code_http(status: u16) -> u32 {
    0x2000_0000 | status as u32
}
pub fn raw_code_str(code: &[u8]) -> u32 {
    let digits = code.strip_prefix(b"-").unwrap_or(code);
    if !digits.is_empty()
        && digits.iter().all(u8::is_ascii_digit)
        && let Some(n) = core::str::from_utf8(code)
            .ok()
            .and_then(|s| s.parse::<i32>().ok())
    {
        return raw_code_num(n);
    }
    0x4000_0000 | (xxhash_rust::xxh3::xxh3_64(code) as u32 & 0x00ff_ffff)
}
