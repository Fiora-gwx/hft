//! Cold assembly and draining of optional per-worker measurement rings.
use crate::{Error, config::Config};
use hft_rt::{
    TscClock,
    latency_trace::{self, Reader, Writer},
};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
};

#[derive(Debug)]
struct Stream {
    name: String,
    reader: Reader,
    file: BufWriter<File>,
    records: u64,
}
#[derive(Debug, Default)]
pub(crate) struct Drain {
    streams: Vec<Stream>,
    manifest: Option<PathBuf>,
}

fn error(e: impl std::fmt::Display) -> Error {
    Error::runtime("LatencyTrace", e.to_string(), 3)
}

pub(crate) fn prepare(c: &Config, clock: TscClock) -> Result<(BTreeMap<String, Writer>, Drain), Error> {
    let Some(capacity) = c.journal.latency_trace_capacity else {
        return Ok((BTreeMap::new(), Drain::default()));
    };
    if !(256..=65536).contains(&capacity) || !capacity.is_power_of_two() {
        return Err(error(
            "latency_trace_capacity must be a power of two in 256..=65536",
        ));
    }
    let root = c
        .paths
        .journal_cold
        .join(crate::storage::instance(c).as_str())
        .join("latency");
    fs::create_dir_all(&root).map_err(error)?;
    let dir = root.join(format!("trace-{}", hft_rt::tsc_ordered().0));
    fs::create_dir(&dir).map_err(error)?;
    let mut writers = BTreeMap::new();
    let mut drain = Drain {
        streams: Vec::new(),
        manifest: Some(dir.join("completion.json")),
    };
    for thread in c.threads.iter().filter(|t| t.name != "persist") {
        let (writer, reader) = latency_trace::channel(capacity).map_err(|e| error(format!("ring: {e:?}")))?;
        if !thread
            .name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
        {
            return Err(error("unsafe trace thread name"));
        }
        let filename = format!("{}.bin", thread.name.replace(':', "_"));
        if filename.contains(['/', '\\']) || filename.contains("..") {
            return Err(error("unsafe trace thread name"));
        }
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(&filename))
            .map_err(error)?;
        drain.streams.push(Stream {
            name: thread.name.clone(),
            reader,
            file: BufWriter::with_capacity(65536, file),
            records: 0,
        });
        writers.insert(thread.name.clone(), writer);
    }
    let threads = drain
        .streams
        .iter()
        .map(|s| format!("\"{}\"", s.name))
        .collect::<Vec<_>>()
        .join(",");
    let schema = format!(
        r#"{{"version":1,"encoding":"little-endian u64","record_bytes":120,"columns":["tsc","serial","dropped","stage","input_seq","decision_seq","order","frame","event_hash","venue_conn","kind_inst","v0","v1","v2","v3"],"clock":{{"counter":"TSC/CNTVCT_EL0","hz_milli":{},"anchor_tsc":{},"realtime_ns":{},"phc_ns":{},"err_ns":{}}},"capacity_per_thread":{},"threads":[{}],"calibration_updates":"ClockStatus in the associated seq journal","complete":false}}"#,
        clock.hz_milli,
        clock.base.tsc.0,
        clock.base.realtime_ns,
        clock.base.phc_ns,
        clock.base.err_ns,
        capacity,
        threads
    );
    fs::write(dir.join("schema.json"), schema).map_err(error)?;
    Ok((writers, drain))
}

impl Drain {
    /// A bounded batch per producer; called only by the existing cold persister.
    pub(crate) fn poll(&mut self) -> Result<(), Error> {
        for stream in &mut self.streams {
            for _ in 0..4096 {
                let Some(record) = stream.reader.rx.try_pop() else {
                    break;
                };
                for word in record.words() {
                    stream.file.write_all(&word.to_le_bytes()).map_err(error)?;
                }
                stream.records += 1;
            }
        }
        Ok(())
    }
    pub(crate) fn finish(&mut self) -> Result<(), Error> {
        while self.streams.iter().any(|s| s.reader.rx.len_hint() != 0) {
            self.poll()?;
        }
        for stream in &mut self.streams {
            stream.file.flush().map_err(error)?;
            stream.file.get_ref().sync_all().map_err(error)?;
        }
        if let Some(path) = &self.manifest {
            let rows = self
                .streams
                .iter()
                .map(|s| {
                    format!(
                        r#"{{"thread":"{}","records":{},"dropped":{}}}"#,
                        s.name,
                        s.records,
                        s.reader.dropped()
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            fs::write(path, format!(r#"{{"closed":true,"streams":[{}]}}"#, rows)).map_err(error)?;
        }
        Ok(())
    }
}
