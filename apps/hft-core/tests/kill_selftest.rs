use hft_rt::{CachePadded, KillWord};
use std::sync::atomic::{AtomicU32, Ordering};

#[test]
fn core_268_selftest_matrix() {
    for ix in [0, 1, 11] {
        let word = CachePadded(AtomicU32::new(u32::MAX));
        hft_core::venues::selftest_word(&word, ix).unwrap();
        assert_eq!(word.load(Ordering::Acquire), 0);
    }
    let word = CachePadded(AtomicU32::new(0));
    assert!(hft_core::venues::selftest_word(&word, 12).is_err());
}

#[test]
fn core_271_selftest_uses_only_atomic_words_and_leaves_them_clear() {
    let words: [KillWord; 6] = std::array::from_fn(|_| CachePadded(AtomicU32::new(0)));
    for (i, word) in words.iter().enumerate() {
        assert_eq!(word as *const KillWord as usize % 128, 0);
        if i != 0 {
            assert_eq!(
                word as *const KillWord as usize - &words[i - 1] as *const KillWord as usize,
                128
            );
        }
        hft_core::venues::selftest_word(word, 0).unwrap();
        assert_eq!(word.load(Ordering::Acquire), 0);
    }
}
