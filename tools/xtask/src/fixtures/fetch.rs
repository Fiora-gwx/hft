//! S3 fixture acquisition. Only verified, decompressed bytes become a reusable cache entry.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use crate::exec::{Cmd, Exec};
use crate::{EXIT_ENV, EXIT_OK, EnvError};

use super::pointer::{self, Content, Object, Pointer};

pub(crate) fn error(path: &Path, e: impl std::fmt::Display) -> EnvError {
    EnvError(format!("{}: {e}", path.display()))
}

pub(crate) fn regular(path: &Path) -> Result<Option<fs::Metadata>, EnvError> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_file() => Ok(Some(m)),
        Ok(_) => Err(error(path, "expected a regular file, not a link or directory")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(error(path, e)),
    }
}

pub(crate) fn directory(path: &Path) -> Result<(), EnvError> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_dir() => Ok(()),
        Ok(_) => Err(error(path, "expected a directory, not a link or file")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|e| error(path, e))
        }
        Err(e) => Err(error(path, e)),
    }
}

fn pointers(root: &Path) -> Result<Vec<Pointer>, EnvError> {
    let dir = root.join("fixtures/replay");
    match fs::symlink_metadata(&dir) {
        Ok(m) if m.file_type().is_dir() => {}
        Ok(_) => return Err(error(&dir, "expected a directory, not a link or file")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(error(&dir, e)),
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| error(&dir, e))? {
        let p = entry.map_err(|e| error(&dir, e))?.path();
        if p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".pointer.toml"))
        {
            paths.push(p);
        }
    }
    paths.sort();
    // Validate the entire manifest before a download, deletion or cache directory creation.
    let mut names = std::collections::BTreeSet::new();
    paths
        .iter()
        .map(|p| {
            let pointer = pointer::read(p)?;
            if !names.insert(pointer.name.to_ascii_lowercase()) {
                return Err(error(p, "case-alias pointer names"));
            }
            Ok(pointer)
        })
        .collect()
}

pub(crate) fn matches(path: &Path, p: &Object) -> Result<bool, EnvError> {
    let Some(meta) = regular(path)? else {
        return Ok(false);
    };
    if meta.len() != p.bytes {
        return Ok(false);
    }
    let mut file = File::open(path).map_err(|e| error(path, e))?;
    let mut hash = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut read = 0u64;
    loop {
        let n = file.read(&mut buf).map_err(|e| error(path, e))?;
        if n == 0 {
            break;
        }
        read = read
            .checked_add(n as u64)
            .ok_or_else(|| error(path, "file too large"))?;
        if read > p.bytes {
            return Ok(false);
        }
        hash.update(&buf[..n]);
    }
    Ok(read == p.bytes && format!("{:x}", hash.finalize()) == p.sha256)
}

pub(super) struct Staging(pub(super) PathBuf);
impl Staging {
    pub(super) fn create(cache: &Path) -> Result<Self, EnvError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..128 {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = cache.join(format!(".fetch-{}-{n}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(error(&path, e)),
            }
        }
        Err(error(cache, "unable to reserve a staging directory"))
    }
}
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn command(exec: &mut dyn Exec, cmd: Cmd) -> Result<(), EnvError> {
    let out = exec.run(&cmd)?;
    if !out.ok() {
        return Err(EnvError(format!(
            "{} failed ({}): {}",
            cmd.program,
            out.code,
            out.stderr.trim()
        )));
    }
    Ok(())
}

/// Read all local pointers, fetch missing or invalid objects and publish verified cache files.
/// AWS credentials come only from the caller's CLI environment (CI uses OIDC). No upload occurs.
pub fn run_with(root: &Path, exec: &mut dyn Exec) -> i32 {
    match fetch(root, exec) {
        Ok(n) => {
            eprintln!("fixtures fetch: {n} verified object(s)");
            EXIT_OK
        }
        Err(e) => {
            eprintln!("fixtures fetch: {e}");
            EXIT_ENV
        }
    }
}

fn fetch(root: &Path, exec: &mut dyn Exec) -> Result<usize, EnvError> {
    let pointers = pointers(root)?;
    if pointers.is_empty() {
        return Ok(0);
    }
    let target = root.join("target");
    directory(&target)?;
    let cache = target.join("fixtures-cache");
    directory(&cache)?;
    for pointer in &pointers {
        let dest = cache.join(&pointer.name);
        let p = match &pointer.content {
            Content::Object(p) => p,
            Content::Files(b) => {
                super::bundle::fetch(&cache, &pointer.name, b, exec)?;
                continue;
            }
        };
        if matches(&dest, p)? {
            continue;
        }
        if regular(&dest)?.is_some() {
            fs::remove_file(&dest).map_err(|e| error(&dest, e))?;
        }
        let staging = Staging::create(&cache)?;
        let decoded = download(&staging.0, p, exec)?;
        // Another fetch may have completed concurrently. Never overwrite an unverified path.
        if regular(&dest)?.is_some() {
            if matches(&dest, p)? {
                continue;
            }
            return Err(error(&dest, "cache changed during fetch"));
        }
        // Same-filesystem hard-link publication is atomic and refuses an existing destination
        // on both Unix and Windows. Dropping staging then removes only the temporary name.
        if let Err(e) = fs::hard_link(&decoded, &dest) {
            if matches(&dest, p)? {
                continue;
            }
            return Err(error(&dest, e));
        }
    }
    Ok(pointers.len())
}

pub(super) fn download(stage: &Path, p: &Object, exec: &mut dyn Exec) -> Result<PathBuf, EnvError> {
    let compressed = stage.join("object.zst");
    let decoded = stage.join("object");
    command(
        exec,
        Cmd::capture("aws", &["s3", "cp"])
            .arg(&p.uri)
            .arg(compressed.display().to_string())
            .arg("--only-show-errors")
            .arg("--no-progress"),
    )?;
    regular(&compressed)?.ok_or_else(|| error(&compressed, "download produced no file"))?;
    command(
        exec,
        Cmd::capture("zstd", &["--decompress", "--quiet", "--keep", "--memory=128MB"])
            .arg(compressed.display().to_string())
            .arg("-o")
            .arg(decoded.display().to_string()),
    )?;
    if !matches(&decoded, p)? {
        return Err(error(
            &decoded,
            format!("decompressed SHA-256 or bytes differ from pointer {}", p.uri),
        ));
    }
    Ok(decoded)
}
