//! Whole-set staging and exact cache membership. No extraction of archive-supplied paths.

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};

use crate::EnvError;
use crate::exec::Exec;

use super::fetch::{Staging, download, error, matches};
use super::pointer::Bundle;

fn existing_directory(path: &Path) -> Result<bool, EnvError> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_dir() => Ok(true),
        Ok(_) => Err(error(path, "expected a directory, not a link or file")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(error(path, e)),
    }
}

/// Validate both parent directories before following cache-relative names.
pub(crate) fn cache_root(root: &Path) -> Result<PathBuf, EnvError> {
    let target = root.join("target");
    let cache = target.join("fixtures-cache");
    if !existing_directory(&target)? || !existing_directory(&cache)? {
        return Err(error(
            &cache,
            "cache missing; run `cargo xtask fixtures fetch` first",
        ));
    }
    Ok(cache)
}

pub(crate) fn verified(path: &Path, bundle: &Bundle) -> Result<bool, EnvError> {
    if !existing_directory(path)? {
        return Ok(false);
    }
    let mut files = BTreeSet::new();
    let mut dirs = BTreeSet::new();
    for entry in walkdir::WalkDir::new(path).min_depth(1).follow_links(false) {
        let entry = entry.map_err(|e| error(path, e))?;
        let relative = entry.path().strip_prefix(path).map_err(|e| error(path, e))?;
        let name = relative
            .to_str()
            .ok_or_else(|| error(path, "non-UTF8 cache path"))?
            .replace('\\', "/");
        let ty = entry.file_type();
        if ty.is_symlink() || !(ty.is_dir() || ty.is_file()) {
            return Err(error(entry.path(), "unexpected link or special file in bundle"));
        }
        if ty.is_dir() {
            dirs.insert(name);
        } else {
            files.insert(name);
        }
    }
    if dirs != bundle.dirs || files != bundle.files.keys().cloned().collect() {
        return Ok(false);
    }
    for (name, object) in &bundle.files {
        if !matches(&path.join(name), object)? {
            return Ok(false);
        }
    }
    Ok(true)
}

struct Lock(PathBuf);
impl Lock {
    fn acquire(cache: &Path, name: &str) -> Result<Self, EnvError> {
        let path = cache.join(format!(".bundle-{}.lock", name.to_ascii_lowercase()));
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| error(&path, format!("exclusive bundle fetch lock: {e}")))?;
        Ok(Self(path))
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub(super) fn fetch(cache: &Path, name: &str, bundle: &Bundle, exec: &mut dyn Exec) -> Result<(), EnvError> {
    let _lock = Lock::acquire(cache, name)?;
    let dest = cache.join(name);
    if verified(&dest, bundle)? {
        return Ok(());
    }
    let staging = Staging::create(cache)?;
    let tree = staging.0.join("tree");
    fs::create_dir(&tree).map_err(|e| error(&tree, e))?;
    for (name, object) in &bundle.files {
        let member = Staging::create(&staging.0)?;
        let decoded = download(&member.0, object, exec)?;
        let path = tree.join(name);
        let parent = path
            .parent()
            .ok_or_else(|| error(&path, "member has no parent"))?;
        fs::create_dir_all(parent).map_err(|e| error(parent, e))?;
        fs::rename(decoded, &path).map_err(|e| error(&path, e))?;
    }
    if !verified(&tree, bundle)? {
        return Err(error(&tree, "staged bundle failed verification"));
    }
    // Keep an invalid old tree until the full replacement is verified. Move it only inside the
    // owned staging directory, whose destructor never follows links. A failed replacement is
    // restored so failure cannot leave a partial new cache entry.
    let stale = staging.0.join("stale");
    if existing_directory(&dest)? {
        let absolute = fs::canonicalize(&dest).map_err(|e| error(&dest, e))?;
        let parent = fs::canonicalize(cache).map_err(|e| error(cache, e))?;
        if absolute.parent() != Some(parent.as_path()) {
            return Err(error(&dest, "cache entry resolves outside the cache root"));
        }
        fs::rename(&dest, &stale).map_err(|e| error(&dest, e))?;
    }
    if let Err(e) = fs::rename(&tree, &dest) {
        if stale.exists() {
            fs::rename(&stale, &dest)
                .map_err(|restore| error(&dest, format!("publish: {e}; restore: {restore}")))?;
        }
        return Err(error(&dest, e));
    }
    Ok(())
}
