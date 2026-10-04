//! `BookSet`: one `Top` per refdata inst and L2 books only for the insts subscribed to L2,
//! carved from one caller-provided region.

use hft_refdata::RefData;
use hft_types::InstId;

use crate::l2::{L2Book, carve};
use crate::top::Top;
use crate::{BookErr, BookParams};

/// Dense per-inst book set (index = `InstId`). Slot 0 and ids outside refdata map to a scratch
/// `Top` that is never snapshotted.
pub struct BookSet {
    /// `n_inst + 1` entries; entry 0 unused.
    tops: Vec<Top>,
    /// `n_inst + 1` entries; 0 = no L2 book, otherwise index + 1 into `books`.
    l2_slot: Vec<u16>,
    books: Vec<L2Book>,
    sink: Top,
}

impl core::fmt::Debug for BookSet {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BookSet")
            .field("n_inst", &self.tops.len().saturating_sub(1))
            .field("n_l2", &self.books.len())
            .finish_non_exhaustive()
    }
}

impl BookSet {
    /// Bytes of caller memory `new_in` needs for these L2 books (each book 64 B aligned).
    pub fn bytes_needed(refdata: &RefData, l2_insts: &[InstId]) -> usize {
        l2_insts
            .iter()
            .map(|i| L2Book::bytes_needed(&BookParams::from_inst(refdata.inst(*i))))
            .sum()
    }

    /// Allocates the `Top` array and one L2 book per `l2_insts` entry in `mem` (64 B aligned).
    pub fn new_in(
        refdata: &RefData,
        l2_insts: &[InstId],
        mem: &'static mut [u8],
    ) -> Result<BookSet, BookErr> {
        let n = refdata.inst_count();
        if l2_insts.len() >= usize::from(u16::MAX) {
            return Err(BookErr::BadParams);
        }
        let mut l2_slot = vec![0u16; n + 1];
        let mut total = 0usize;
        for (k, i) in l2_insts.iter().enumerate() {
            let ix = i.0 as usize;
            if ix == 0 || ix > n {
                return Err(BookErr::UnknownInst);
            }
            let slot = l2_slot.get_mut(ix).ok_or(BookErr::UnknownInst)?;
            if *slot != 0 {
                return Err(BookErr::DupInst);
            }
            *slot = (k + 1) as u16;
            let p = BookParams::from_inst(refdata.inst(*i));
            if !p.is_valid() {
                return Err(BookErr::BadParams);
            }
            total += L2Book::bytes_needed(&p);
        }
        let mut words = carve(mem, total)?;
        let mut books = Vec::with_capacity(l2_insts.len());
        for i in l2_insts {
            let p = BookParams::from_inst(refdata.inst(*i));
            let g = L2Book::geo_of(&p);
            let (head, rest) = core::mem::take(&mut words).split_at_mut(g.bytes() / 8);
            words = rest;
            books.push(L2Book::from_words(g, head));
        }
        Ok(BookSet {
            tops: vec![Top::ZERO; n + 1],
            l2_slot,
            books,
            sink: Top::ZERO,
        })
    }

    pub(crate) fn tops_len(&self) -> usize {
        self.tops.len()
    }

    #[inline]
    pub fn top(&self, i: InstId) -> &Top {
        self.tops.get(i.0 as usize).unwrap_or(&self.sink)
    }

    #[inline]
    pub fn top_mut(&mut self, i: InstId) -> &mut Top {
        match self.tops.get_mut(i.0 as usize) {
            Some(t) => t,
            None => &mut self.sink,
        }
    }

    #[inline]
    fn slot(&self, i: InstId) -> Option<usize> {
        let s = *self.l2_slot.get(i.0 as usize)?;
        (s != 0).then(|| usize::from(s) - 1)
    }

    #[inline]
    pub fn l2(&self, i: InstId) -> Option<&L2Book> {
        self.books.get(self.slot(i)?)
    }

    #[inline]
    pub fn l2_mut(&mut self, i: InstId) -> Option<&mut L2Book> {
        let s = self.slot(i)?;
        self.books.get_mut(s)
    }
}
