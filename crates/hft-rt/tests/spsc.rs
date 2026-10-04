//! In-process SPSC ring: capacity rules, full/empty, hints, batches (RT-001 – RT-006).
#![cfg(not(loom))]

use hft_rt::{Backing, RtErr, spsc};

#[test]
fn rt_001_capacity_rules() {
    let rows: [(&str, usize, Result<(), RtErr>); 8] = [
        ("RT-001.1", 0, Err(RtErr::Capacity)),
        ("RT-001.2", 1, Err(RtErr::Capacity)),
        ("RT-001.3", 2, Ok(())),
        ("RT-001.4", 3, Err(RtErr::Capacity)),
        ("RT-001.5", 6, Err(RtErr::Capacity)),
        ("RT-001.6", 1024, Ok(())),
        ("RT-001.7", 65536, Ok(())),
        ("RT-001.8", 1 << 62, Err(RtErr::NoMem)),
    ];
    for (row, cap, want) in rows {
        let got = spsc::<u64>(cap, Backing::Heap).map(|_| ());
        assert_eq!(got, want, "{row}");
    }
}

#[test]
fn rt_002_full_empty_fifo() {
    let (mut p, mut c) = spsc::<u64>(4, Backing::Heap).unwrap();
    for v in 1..=4 {
        assert_eq!(p.try_push(v), Ok(()));
    }
    assert_eq!(p.try_push(5), Err(5));
    assert_eq!(c.try_pop(), Some(1));
    assert_eq!(p.try_push(5), Ok(()));
    let got: Vec<Option<u64>> = (0..5).map(|_| c.try_pop()).collect();
    assert_eq!(got, vec![Some(2), Some(3), Some(4), Some(5), None]);
}

#[test]
fn rt_003_hints() {
    let (mut p, mut c) = spsc::<u64>(8, Backing::Heap).unwrap();
    for v in 0..3 {
        p.try_push(v).unwrap();
    }
    assert_eq!(p.free_hint(), 5);
    assert_eq!(c.len_hint(), 3);
    c.try_pop().unwrap();
    c.try_pop().unwrap();
    assert_eq!(c.len_hint(), 1);
    p.try_push(3).unwrap();
    let f = p.free_hint();
    assert!(f == 4 || f == 6, "free_hint {f}");
}

#[test]
fn rt_005_write_batch_commit() {
    let (mut p, mut c) = spsc::<u64>(8, Backing::Heap).unwrap();
    p.try_push(1).unwrap();
    p.try_push(2).unwrap();
    let mut b = p.write_batch(10);
    let pushed: Vec<bool> = (10..17).map(|v| b.push(v)).collect();
    assert_eq!(pushed, vec![true, true, true, true, true, true, false]);
    assert_eq!(c.len_hint(), 2);
    b.commit();
    let got: Vec<u64> = std::iter::from_fn(|| c.try_pop()).collect();
    assert_eq!(got, vec![1, 2, 10, 11, 12, 13, 14, 15]);
    assert_eq!(c.try_pop(), None);
    {
        let mut b = p.write_batch(3);
        for v in 0..3 {
            assert!(b.push(v));
        }
    } // dropped without commit
    assert_eq!(c.try_pop(), None);
}

#[test]
fn rt_006_read_batch_wrap() {
    let (mut p, mut c) = spsc::<u64>(8, Backing::Heap).unwrap();
    for v in 0..6 {
        p.try_push(v).unwrap();
    }
    for _ in 0..6 {
        c.try_pop().unwrap();
    }
    for v in 100..105 {
        p.try_push(v).unwrap();
    }
    {
        let b = c.read_batch(8);
        assert_eq!(b.slices(), (&[100u64, 101][..], &[102u64, 103, 104][..]));
    }
    let b = c.read_batch(3);
    assert_eq!(b.slices(), (&[100u64, 101][..], &[102u64][..]));
    b.release();
    assert_eq!(c.len_hint(), 2);
}
