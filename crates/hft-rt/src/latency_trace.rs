//! Optional, single-writer measurement transport. Never consulted by business logic.
use crate::{Backing, Consumer, Producer, RtErr, spsc, tsc_ordered};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

/// Stable numeric stages; the offline schema owns their display names.
#[derive(Clone, Copy, Debug)]
#[repr(u64)]
pub enum Stage {
    Read = 1,
    DecryptStart,
    DecryptEnd,
    Frame,
    DecodeStart,
    DecodeEnd,
    DecodedEvent,
    FeedPush,
    RepPush,
    InputPop,
    ProcessStart,
    JournalInput,
    StrategyStart,
    StrategyEnd,
    RiskStart,
    RiskEnd,
    OmsStart,
    OmsEnd,
    Decision,
    OutPush,
    OutPop,
    EncodeStart,
    EncodeEnd,
    EncryptStart,
    EncryptEnd,
    Send,
    ProcessEnd,
    TxStamp,
    InputHanded,
    ExecStart,
    ExecEnd,
    TxQueued,
    TxWrite,
}

/// Correlation only. Zero means unavailable; order packs generation above slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Context {
    pub input_seq: u64,
    pub decision_seq: u64,
    pub order: u64,
    pub frame: u64,
    pub event_hash: u64,
    pub venue_conn: u64,
    pub kind_inst: u64,
}

/// Fifteen little-endian u64 words on disk, independent of Rust struct layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Record {
    pub tsc: u64,
    pub serial: u64,
    pub dropped: u64,
    pub stage: u64,
    pub context: Context,
    pub values: [u64; 4],
}
impl Record {
    pub const WORDS: usize = 15;
    pub fn words(self) -> [u64; Self::WORDS] {
        let c = self.context;
        [
            self.tsc,
            self.serial,
            self.dropped,
            self.stage,
            c.input_seq,
            c.decision_seq,
            c.order,
            c.frame,
            c.event_hash,
            c.venue_conn,
            c.kind_inst,
            self.values[0],
            self.values[1],
            self.values[2],
            self.values[3],
        ]
    }
}

#[derive(Debug)]
pub struct Writer {
    tx: Producer<Record>,
    dropped: &'static AtomicU64,
    serial: u64,
    context: Context,
}
#[derive(Debug)]
pub struct Reader {
    pub rx: Consumer<Record>,
    dropped: &'static AtomicU64,
}
impl Reader {
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

/// Startup only; bounded storage is allocated and touched by the SPSC constructor.
pub fn channel(capacity: usize) -> Result<(Writer, Reader), RtErr> {
    let (tx, rx) = spsc(capacity, Backing::Heap)?;
    let dropped: &'static AtomicU64 = Box::leak(Box::new(AtomicU64::new(0)));
    Ok((
        Writer {
            tx,
            dropped,
            serial: 0,
            context: Context::default(),
        },
        Reader { rx, dropped },
    ))
}

// D3 exception: measurement-only single-writer state; never read by decisions.
fn with_writer<R>(f: impl FnOnce(&RefCell<Option<Writer>>) -> R) -> R {
    thread_local! {
        static WRITER: RefCell<Option<Writer>> = const { RefCell::new(None) };
    }
    WRITER.with(f)
}

#[derive(Debug)]
pub struct AlreadyInstalled;

/// Call once on the owning thread before its hot loop; replacement is rejected.
pub fn install(writer: Writer) -> Result<(), AlreadyInstalled> {
    with_writer(|w| match w.try_borrow_mut() {
        Ok(mut w) if w.is_none() => {
            *w = Some(writer);
            Ok(())
        }
        _ => Err(AlreadyInstalled),
    })
}

#[inline]
pub fn enabled() -> bool {
    with_writer(|w| w.try_borrow().is_ok_and(|w| w.is_some()))
}

#[inline]
pub fn context() -> Context {
    with_writer(|w| {
        w.try_borrow()
            .ok()
            .and_then(|w| w.as_ref().map(|w| w.context))
            .unwrap_or_default()
    })
}

#[inline]
pub fn set_context(context: Context) {
    with_writer(|w| {
        if let Ok(mut w) = w.try_borrow_mut()
            && let Some(w) = w.as_mut()
        {
            w.context = context;
        }
    });
}

#[derive(Debug)]
pub struct Scope(Context);
impl Drop for Scope {
    fn drop(&mut self) {
        set_context(self.0);
    }
}
pub fn scope(context: Context) -> Scope {
    let previous = self::context();
    set_context(context);
    Scope(previous)
}

/// Bounded, nonblocking, no allocation/OS clock. Overflow affects diagnostics only.
#[inline]
pub fn at(stage: Stage, tsc: u64, values: [u64; 4]) {
    with_writer(|w| {
        if let Ok(mut w) = w.try_borrow_mut()
            && let Some(w) = w.as_mut()
        {
            w.serial = w.serial.saturating_add(1);
            let r = Record {
                tsc,
                serial: w.serial,
                dropped: w.dropped.load(Ordering::Relaxed),
                stage: stage as u64,
                context: w.context,
                values,
            };
            if w.tx.try_push(r).is_err() {
                w.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
}

#[inline]
pub fn stamp(stage: Stage, values: [u64; 4]) {
    if enabled() {
        at(stage, tsc_ordered().0, values);
    }
}

/// New per-thread frame identity; its value is the Frame record's serial.
pub fn frame(venue: u8, conn: u8, values: [u64; 4]) {
    with_writer(|w| {
        if let Ok(mut w) = w.try_borrow_mut()
            && let Some(w) = w.as_mut()
        {
            w.context = Context {
                frame: w.serial.saturating_add(1),
                venue_conn: u64::from(venue) | (u64::from(conn) << 8),
                ..Context::default()
            };
        }
    });
    stamp(Stage::Frame, values);
}

/// Stable FNV-1a over normalized, nonsecret event bytes. Cold joins reject ambiguity.
#[inline]
pub fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}

/// Associate an emitted normalized event with the current frame; no payload is retained.
#[inline]
pub fn event(stage: Stage, kind: u16, bytes: &[u8]) {
    event_values(stage, kind, bytes, [bytes.len() as u64, 0, 0, 0]);
}

pub fn event_values(stage: Stage, kind: u16, bytes: &[u8], values: [u64; 4]) {
    if enabled() {
        let mut c = context();
        c.event_hash = fingerprint(bytes);
        c.kind_inst = u64::from(kind);
        let _scope = scope(c);
        stamp(stage, values);
    }
}
