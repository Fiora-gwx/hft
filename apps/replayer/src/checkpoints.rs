//! Bounded, read-only checkpoint file assembly. Identity and Model semantics belong to
//! hft-sequencer; directory names are taken from recorded refdata.

use std::io::Read;
use std::path::{Path, PathBuf};

use hft_sequencer::advisor::checkpoint;
use hft_sequencer::driver::recompute::{ModelCheckpoint, model_identities};
use hft_sequencer::driver::replay::Log;
use hft_strategy_sdk::StrategySet;

use crate::Failure;

fn read(path: &Path, limit: usize) -> Result<Option<Vec<u8>>, Failure> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err((3, format!("{}: {e}", path.display()))),
    };
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err((2, format!("checkpoint is not a regular file: {}", path.display())));
    }
    if meta.len() > limit as u64 {
        return Err((3, "checkpoint files exceed --max-model-bytes".into()));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| (3, e.to_string()))?
        .take((limit as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| (3, e.to_string()))?;
    if bytes.len() > limit {
        return Err((3, "checkpoint grew beyond its read limit".into()));
    }
    Ok(Some(bytes))
}

pub(crate) fn load<S: StrategySet>(
    path: Option<&Path>,
    log: &Log,
    limit: usize,
) -> Result<(Vec<ModelCheckpoint>, Vec<PathBuf>), Failure> {
    let Some(path) = path else {
        return Ok((Vec::new(), Vec::new()));
    };
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Vec::new(), Vec::new())),
        Err(e) => return Err((3, e.to_string())),
    };
    if meta.file_type().is_symlink() {
        return Err((2, "symlink checkpoint input is not supported".into()));
    }
    let ids = model_identities::<S>(log).map_err(crate::model_error)?;
    let mut out = Vec::new();
    let mut paths = Vec::new();
    if meta.is_file() {
        let bytes = read(path, limit)?.ok_or((3, "checkpoint disappeared while loading".into()))?;
        let c = checkpoint::decode(&bytes, limit).map_err(|e| (3, format!("checkpoint: {e:?}")))?;
        let id = ids
            .iter()
            .find(|id| id.strategy == c.key.strategy)
            .ok_or((2, "checkpoint strategy is absent from the journal".into()))?;
        out.push(ModelCheckpoint {
            instance: id.instance.clone(),
            bytes,
        });
        paths.push(path.to_owned());
    } else if meta.is_dir() {
        let mut used = 0usize;
        let mut names = std::collections::BTreeSet::new();
        for id in ids {
            if !names.insert(id.instance.clone()) {
                continue;
            }
            let name = &id.instance;
            if name.is_empty() || matches!(name.as_str(), "." | "..") || name.contains(['/', '\\', ':']) {
                return Err((2, "recorded instance name is not a checkpoint filename".into()));
            }
            let p = path.join(format!("{name}.ckpt"));
            if let Some(bytes) = read(&p, limit.saturating_sub(used))? {
                checkpoint::decode(&bytes, limit).map_err(|e| (3, format!("{}: {e:?}", p.display())))?;
                used += bytes.len();
                out.push(ModelCheckpoint {
                    instance: id.instance,
                    bytes,
                });
                paths.push(p);
            }
        }
    } else {
        return Err((2, "checkpoint input is not a file or directory".into()));
    }
    Ok((out, paths))
}
