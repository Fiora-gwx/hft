//! Call trace of the processing order (feature `testing`): tests record which component entry
//! the sequencer called, in order. Without the feature nothing is recorded.

/// One traced call: a static name plus two arguments.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct TraceEv {
    pub what: &'static str,
    pub a: u64,
    pub b: u64,
}

/// The trace buffer.
#[derive(Clone, Debug, Default)]
pub struct Trace {
    /// Recording is on (tests switch it on).
    pub on: bool,
    pub ev: Vec<TraceEv>,
}

impl Trace {
    #[inline]
    #[allow(unused_variables)]
    pub fn rec(&mut self, what: &'static str, a: u64, b: u64) {
        #[cfg(feature = "testing")]
        if self.on {
            self.ev.push(TraceEv { what, a, b });
        }
    }
    /// Names in order.
    pub fn names(&self) -> Vec<&'static str> {
        self.ev.iter().map(|e| e.what).collect()
    }
    /// Position of the first event with this name.
    pub fn pos(&self, what: &str) -> Option<usize> {
        self.ev.iter().position(|e| e.what == what)
    }
    /// Count of events with this name.
    pub fn count(&self, what: &str) -> usize {
        self.ev.iter().filter(|e| e.what == what).count()
    }
    pub fn clear(&mut self) {
        self.ev.clear();
    }
}
