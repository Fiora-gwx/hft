//! Observe the owned credential allocation immediately before System frees it.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Inspect;
static WATCH: AtomicUsize = AtomicUsize::new(0);
static WIPED: AtomicBool = AtomicBool::new(false);
// SAFETY: allocation is delegated unchanged to System; only the watched, still-live allocation
// is read immediately before its deallocation, with no allocation or panic in this hook.
unsafe impl GlobalAlloc for Inspect {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if WATCH
            .compare_exchange(ptr as usize, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let bytes = unsafe { std::slice::from_raw_parts(ptr, layout.size()) };
            WIPED.store(bytes.iter().all(|b| *b == 0), Ordering::SeqCst);
        }
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Inspect = Inspect;

#[test]
fn core_224_key_buffers_zeroized() {
    let root = tempfile::tempdir().unwrap();
    let mut cfg: hft_core::config::Config =
        toml::from_str(include_str!("../config/binance-testnet.toml")).unwrap();
    for length in [64, 4096] {
        let path = root.path().join("test-key");
        std::fs::write(&path, vec![0xab; length]).unwrap();
        let keys = &mut cfg.venues.values_mut().next().unwrap().accounts[0];
        keys.api_key_path = path.clone();
        keys.session_seed_path = path.clone();
        keys.rest_api_key_path = path.clone();
        keys.rest_secret_path = path.clone();
        keys.passphrase_path = None;
        let secrets = hft_core::secrets::SecretFiles::read_and_unlink(&cfg).unwrap();
        let raw = secrets.get(&path).unwrap();
        assert!(raw.iter().all(|b| *b == 0xab));
        WIPED.store(false, Ordering::SeqCst);
        WATCH.store(raw.as_ptr() as usize, Ordering::SeqCst);
        drop(secrets);
        assert_eq!(
            WATCH.load(Ordering::SeqCst),
            0,
            "watched allocation was not freed"
        );
        assert!(
            WIPED.load(Ordering::SeqCst),
            "credential allocation retained bytes"
        );
    }
}
