//! Shared helpers of the hft-refdata integration tests.
#![allow(dead_code)]

pub mod f0;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

pub use f0::*;
use hft_refdata::blob::{ENTRY_LEN, HEADER_LEN, RawBlob, SectionEntry};
use hft_refdata::{LoadErr, LoadErrCode, LoadReport, RefData};
use zerocopy::{FromBytes, Immutable, IntoBytes};

// ---- per-thread counting allocator (stands in for hft-testkit::CountingAlloc) ----

pub struct CountingAlloc;

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static COUNT: Cell<u64> = const { Cell::new(0) };
    static LIVE_BYTES: Cell<i64> = const { Cell::new(0) };
}

fn note(delta: i64, is_alloc: bool) {
    let _ = ENABLED.try_with(|e| {
        if e.get() {
            if is_alloc {
                let _ = COUNT.try_with(|c| c.set(c.get() + 1));
            }
            let _ = LIVE_BYTES.try_with(|b| b.set(b.get() + delta));
        }
    });
}

// SAFETY: every method forwards to `System` with the caller's arguments unchanged; the extra
// work only touches const-initialized thread-locals, which never allocate.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note(layout.size() as i64, true);
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        note(layout.size() as i64, true);
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note(new_size as i64 - layout.size() as i64, true);
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        note(-(layout.size() as i64), false);
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

/// Starts counting on this thread (resets the counters).
pub fn count_start() {
    COUNT.with(|c| c.set(0));
    LIVE_BYTES.with(|b| b.set(0));
    ENABLED.with(|e| e.set(true));
}

/// Stops counting; returns (allocation count, net live bytes).
pub fn count_stop() -> (u64, i64) {
    ENABLED.with(|e| e.set(false));
    (COUNT.with(|c| c.get()), LIVE_BYTES.with(|b| b.get()))
}

// ---- load helpers ----

pub fn load(b: &[u8]) -> Result<RefData, LoadReport> {
    RefData::load(b, None)
}

pub fn ok(b: &[u8]) -> RefData {
    match load(b) {
        Ok(r) => r,
        Err(r) => panic!("expected Ok, got {:?}", r.errors),
    }
}

pub fn e(code: LoadErrCode, section: u16, row: u32) -> LoadErr {
    LoadErr { code, section, row }
}

/// Errors of a failing load (panics on success).
pub fn errs(b: &[u8]) -> Vec<LoadErr> {
    match load(b) {
        Ok(_) => panic!("expected Err, got Ok"),
        Err(r) => r.errors,
    }
}

/// `Ok` → empty vector; `Err` → errors.
pub fn outcome(r: Result<RefData, LoadReport>) -> Vec<LoadErr> {
    r.err().map(|r| r.errors).unwrap_or_default()
}

/// Asserts a load fails with exactly one error of `code`.
pub fn assert_single(b: &[u8], code: LoadErrCode, case: &str) {
    let es = errs(b);
    assert_eq!(es.len(), 1, "{case}: {es:?}");
    assert_eq!(es[0].code, code, "{case}: {es:?}");
}

// ---- raw editing helpers ----

pub fn raw(b: &[u8]) -> RawBlob {
    RawBlob::parse(b).expect("well-formed container")
}

/// Reads row `i` of section `id`.
pub fn get_row<T: FromBytes>(r: &RawBlob, id: u16, i: usize) -> T {
    r.section(id).and_then(|s| s.read_row(i)).expect("row exists")
}

/// Edits row `i` of section `id` in place.
pub fn edit_row<T, F>(r: &mut RawBlob, id: u16, i: usize, f: F)
where
    T: FromBytes + IntoBytes + Immutable,
    F: FnOnce(&mut T),
{
    let mut row: T = get_row(r, id, i);
    f(&mut row);
    assert!(r.section_mut(id).expect("section").write_row(i, &row));
}

/// Directory entry `i` of an encoded blob.
pub fn dir_entry(b: &[u8], i: usize) -> SectionEntry {
    let at = HEADER_LEN + ENTRY_LEN * i;
    SectionEntry::read_from_bytes(&b[at..at + ENTRY_LEN]).expect("entry")
}

pub fn set_dir_entry(b: &mut [u8], i: usize, e: &SectionEntry) {
    let at = HEADER_LEN + ENTRY_LEN * i;
    b[at..at + ENTRY_LEN].copy_from_slice(e.as_bytes());
}

/// Index of section `id` in the directory of an encoded blob.
pub fn dir_index(b: &[u8], id: u16) -> usize {
    let n = u16::from_le_bytes([b[10], b[11]]) as usize;
    (0..n)
        .find(|&i| dir_entry(b, i).id == id)
        .expect("section in directory")
}

/// Appends `bytes` to the Strings section and returns the StrRef pointing at them.
pub fn push_string(r: &mut RawBlob, bytes: &[u8]) -> hft_refdata::blob::StrRef {
    let s = r.section_mut(hft_refdata::blob::sec::STRINGS).expect("strings");
    let off = s.data.len() as u32;
    s.data.extend_from_slice(bytes);
    s.count = s.data.len() as u32;
    hft_refdata::blob::StrRef {
        off,
        len: bytes.len() as u32,
    }
}

/// Renumbers the assets of `s` to `ids` (row k gets `ids[k]`), keeping references on the same rows.
pub fn renumber_assets(s: &mut Spec, ids: &[u16]) {
    use hft_types::AssetId;
    let map = |a: AssetId| AssetId(ids[usize::from(a.0) - 1]);
    for i in &mut s.insts {
        i.asset = map(i.asset);
        i.quote = map(i.quote);
    }
    for b in &mut s.sim_balances {
        b.asset = map(b.asset);
    }
    for (a, id) in s.assets.iter_mut().zip(ids) {
        a.id = AssetId(*id);
    }
}
