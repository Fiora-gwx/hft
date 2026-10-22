//! CORE-429: bounded shutdown joins (`runtime::join_all_within`).
use hft_core::Error;
use hft_core::runtime::join_all_within;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

fn named(
    name: &str,
    f: impl FnOnce() -> Result<(), Error> + Send + 'static,
) -> std::thread::JoinHandle<Result<(), Error>> {
    std::thread::Builder::new().name(name.into()).spawn(f).unwrap()
}

#[test]
fn core_429_bounded_shutdown_join() {
    let limit = Some(Duration::from_millis(200));
    // .1 every thread ends: Ok.
    let r = join_all_within(vec![named("a", || Ok(())), named("b", || Ok(()))], limit);
    assert!(r.is_ok(), "CORE-429.1");
    // .2 a failed thread: its error.
    let r = join_all_within(
        vec![
            named("a", || Ok(())),
            named("feed", || Err(Error::runtime("Boom", "x", 3))),
        ],
        limit,
    );
    assert_eq!(r.map_err(|e| e.code), Err("Boom"), "CORE-429.2");
    // .3 a thread that never exits: ShutdownStuck naming it after the limit, not a hang.
    let release = Arc::new(AtomicBool::new(false));
    let r2 = release.clone();
    let start = Instant::now();
    let r = join_all_within(
        vec![
            named("a", || Ok(())),
            named("persist", move || {
                while !r2.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(())
            }),
        ],
        limit,
    );
    let took = start.elapsed();
    release.store(true, Ordering::Release);
    let e = r.unwrap_err();
    assert_eq!((e.code, e.exit), ("ShutdownStuck", 134), "CORE-429.3");
    assert!(e.detail.contains("persist"), "CORE-429.3 {}", e.detail);
    assert!(took < Duration::from_secs(2), "CORE-429.3 {took:?}");
    // .4 an error and a stuck thread: the error is returned (first cause).
    let release = Arc::new(AtomicBool::new(false));
    let r2 = release.clone();
    let r = join_all_within(
        vec![
            named("feed", || Err(Error::runtime("Boom", "x", 3))),
            named("persist", move || {
                while !r2.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(())
            }),
        ],
        limit,
    );
    release.store(true, Ordering::Release);
    assert_eq!(r.map_err(|e| e.code), Err("Boom"), "CORE-429.4");
}
