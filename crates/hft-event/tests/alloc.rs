//! Zero allocation and zero system calls on every codec entry (EVT-196).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

#[path = "common/cmds.rs"]
mod cmds;
mod common;
#[path = "common/spb.rs"]
mod spb;

use common::*;
use hft_event::*;
use hft_types::*;
use std::alloc::{GlobalAlloc, Layout as AllocLayout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, Ordering};

struct Counting;
thread_local! {
    // No destructor or lazy heap storage. The harness's allocations on other threads do
    // not belong to the measured synchronous codec calls.
    static ON: Cell<bool> = const { Cell::new(false) };
    static COUNTS: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
}

fn note(alloc: bool) {
    let _ = ON.try_with(|on| {
        if on.get() {
            let _ = COUNTS.try_with(|counts| {
                let (a, f) = counts.get();
                counts.set((a + u64::from(alloc), f + u64::from(!alloc)));
            });
        }
    });
}

fn measured(work: impl FnOnce()) -> (u64, u64) {
    COUNTS.with(|c| c.set((0, 0)));
    ON.with(|on| on.set(true));
    work();
    ON.with(|on| on.set(false));
    COUNTS.with(Cell::get)
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: AllocLayout) -> *mut u8 {
        note(true);
        // SAFETY: forwarded unchanged to the system allocator.
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: AllocLayout) {
        note(false);
        // SAFETY: forwarded unchanged to the system allocator.
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Caller-provided buffers for every entry.
struct Ctx {
    out: Vec<u64>,
    rec_bbo: Vec<u64>,
    rec_adv: Vec<u64>,
    cmd_body: Vec<u8>,
    spb: (Vec<u64>, usize),
    snap: Vec<u8>,
    parts: Vec<Vec<u8>>,
    reasm: Vec<u8>,
    payload: Vec<u8>,
    bbo: Bbo,
}

fn aligned_vec(b: &[u8]) -> Vec<u64> {
    let mut v = vec![0u64; b.len().div_ceil(8)];
    zerocopy::IntoBytes::as_mut_bytes(v.as_mut_slice())[..b.len()].copy_from_slice(b);
    v
}

fn ctx() -> Ctx {
    let payload = vec![7u8; 1024];
    let adv = AdviceHdr {
        strategy: StrategyId(3),
        account: AccountId(0),
        local_kind: 1,
        group: GROUP_NONE,
        len: 1024,
        _pad: [0; 6],
        basis_seq: 1,
        basis_tsc: Tsc(1),
    };
    let mut tmp = vec![0u8; 2048];
    let n = encode_var(&mut tmp, 1, 1, RecordFlags(0), &adv, &payload).unwrap() as usize;
    let w = [1u8; 10];
    let cmd_body = cmds::encode(&cmds::all_samples(&w)[3]);
    let mut snap = vec![0u8; 96];
    snap.extend([1u8; 100_000]);
    let parts = PartIter::new(&snap)
        .unwrap()
        .enumerate()
        .map(|(i, (ph, c, f))| {
            let mut o = vec![0u8; 65_536];
            let n = encode_part(&mut o, 10 + i as u64, 1, f, EventKind::STATE_SNAPSHOT, &ph, c).unwrap();
            o.truncate(n as usize);
            o
        })
        .collect();
    Ctx {
        out: vec![0u64; 8192],
        rec_bbo: aligned_vec(&enc(&bbo(), 1, 1, RecordFlags(0))),
        rec_adv: aligned_vec(&tmp[..n]),
        cmd_body,
        spb: spb::aligned(&spb::blob_b()),
        snap,
        parts,
        reasm: vec![0u8; 1 << 20],
        payload,
        bbo: bbo(),
    }
}

fn work(c: &mut Ctx) {
    use zerocopy::IntoBytes;
    let adv = AdviceHdr {
        strategy: StrategyId(3),
        account: AccountId(0),
        local_kind: 1,
        group: GROUP_NONE,
        len: 1024,
        _pad: [0; 6],
        basis_seq: 1,
        basis_tsc: Tsc(1),
    };
    let b = c.bbo;
    let h = cmds::hdr();
    let kill = ControlCmd::Kill {
        scope: cmds::scope(),
        level: KillLevel::Account,
        flatten: false,
        cancel_resting: true,
        hard: false,
    };
    let mut r = Reassembler::new(&mut c.reasm);
    for i in 0..1000u64 {
        let out = c.out.as_mut_bytes();
        black_box(record_len(black_box(80), false));
        black_box(encode_fixed(out, i, i, RecordFlags(0), &b));
        black_box(encode_var(out, i, i, RecordFlags(0), &adv, &c.payload)).ok();
        let rv = decode_record(c.rec_bbo.as_bytes()).unwrap();
        black_box(rv.read::<Bbo>()).ok();
        black_box(rv.view::<Bbo>()).ok();
        let ra = decode_record(c.rec_adv.as_bytes()).unwrap();
        black_box(ra.var::<AdviceHdr>()).ok();
        black_box(classify(EventKind::NOTE, rv.body)).ok();
        black_box(kind_info(EventKind(black_box(0x0203))));
        let t = TimerCode::Oms(OmsTimer::UnknownQuery { table_ix: 3, slot: 9 });
        let (o, id, g) = t.encode().unwrap();
        black_box(TimerCode::decode(o, id, g)).ok();
        let cr = CtlRecord {
            kind: EventKind::CONTROL_CMD,
            flags: RecordFlags(0),
            body: &c.cmd_body,
        };
        let n = cr.encode_into(out).unwrap();
        black_box(CtlRecord::read(&out[..n])).ok();
        let ar = AdviceRecord {
            hdr: adv,
            payload: &c.payload,
        };
        let n = ar.encode_into(out).unwrap();
        black_box(AdviceRecord::read(&out[..n])).ok();
        let n = kill.encode(&h, out).unwrap();
        black_box(ControlCmd::decode(&out[..n])).ok();
        black_box(ControlCmd::decode(&c.cmd_body)).ok();
        black_box(parse_spb1(&c.spb.0.as_bytes()[..c.spb.1])).ok();
        black_box(multi_view::<SnapshotHdr>(&c.snap)).ok();
        for p in PartIter::new(&c.snap).unwrap() {
            black_box(p);
        }
        for p in &c.parts {
            let v = decode_record(p).unwrap();
            black_box(r.push(&v).map(|x| x.map(<[u8]>::len))).ok();
        }
    }
}

#[test]
fn evt_196_zero_alloc_zero_syscall() {
    // Positive control: neither side of an allocation/free may disappear from the counter.
    assert_eq!(measured(|| drop(black_box(Box::new(1u64)))), (1, 1));
    // Negative control: overlap an unrelated thread's real allocation with the measuring
    // interval, without allocating synchronization objects inside that interval.
    static START: AtomicBool = AtomicBool::new(false);
    static DONE: AtomicBool = AtomicBool::new(false);
    let other = std::thread::spawn(|| {
        while !START.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        drop(black_box(Box::new(2u64)));
        DONE.store(true, Ordering::Release);
    });
    let foreign = measured(|| {
        START.store(true, Ordering::Release);
        while !DONE.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
    });
    other.join().unwrap();
    assert_eq!(foreign, (0, 0));
    let mut c = ctx();
    work(&mut c); // warm-up
    assert_eq!(measured(|| work(&mut c)), (0, 0), "step 1");
    #[cfg(target_os = "linux")]
    seccomp_step(&mut c);
}

/// Step 2: a forked child runs the same calls under seccomp strict mode (only read/write/exit/
/// sigreturn); any other system call kills it with SIGKILL.
#[cfg(target_os = "linux")]
fn seccomp_step(c: &mut Ctx) {
    // SAFETY: fork in a test; the child only runs pure code and exits via the raw exit syscall.
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0);
    if pid == 0 {
        // SAFETY: prctl/syscall with constant arguments in the child.
        unsafe {
            if libc::prctl(libc::PR_SET_SECCOMP, libc::SECCOMP_MODE_STRICT as libc::c_ulong) != 0 {
                libc::syscall(libc::SYS_exit, 2);
            }
        }
        work(c);
        // SAFETY: raw exit (exit_group is not allowed in strict mode).
        unsafe {
            libc::syscall(libc::SYS_exit, 0);
        }
    }
    let mut status = 0;
    // SAFETY: waiting for our own child.
    unsafe { libc::waitpid(pid, &mut status, 0) };
    assert!(
        libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
        "child status {status:#x}"
    );
}
