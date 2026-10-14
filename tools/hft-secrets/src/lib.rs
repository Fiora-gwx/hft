//! Startup-only secret retrieval. Diagnostics never format manifests or service responses.
use serde::Deserialize;
#[cfg(unix)]
use std::path::PathBuf;
use std::{collections::BTreeSet, io::Write, path::Path};
use zeroize::Zeroizing;
pub mod aws;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub secret: Vec<Entry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub name: String,
    pub secret_id: String,
    pub json_key: Option<String>,
    pub kind: Kind,
}
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Plain,
    KmsCiphertext,
}

pub fn manifest(text: &str) -> Result<Manifest, &'static str> {
    // TOML errors contain source excerpts; deliberately discard their formatted text.
    let m: Manifest = toml::from_str(text).map_err(|_| "InvalidManifest")?;
    let mut names = BTreeSet::new();
    for e in &m.secret {
        let name = e.name.as_bytes();
        if name.is_empty()
            || name.len() > 255
            || e.name == "."
            || e.name == ".."
            || !name
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(b))
            || !names.insert(e.name.as_str())
        {
            return Err("InvalidName");
        }
        let parts: Vec<_> = e.secret_id.split(':').collect();
        if parts.len() < 7
            || parts[..3] != ["arn", "aws", "secretsmanager"]
            || parts[3].is_empty()
            || parts[4].len() != 12
            || !parts[4].bytes().all(|b| b.is_ascii_digit())
            || parts[5] != "secret"
            || parts[6].is_empty()
            || e.secret_id.bytes().any(|b| b.is_ascii_control())
        {
            return Err("InvalidSecretArn");
        }
        if e.json_key.as_ref().is_some_and(|k| k.is_empty()) {
            return Err("InvalidJsonKey");
        }
    }
    if m.secret.is_empty() {
        return Err("EmptyManifest");
    }
    Ok(m)
}

pub struct Secret {
    pub bytes: Zeroizing<Vec<u8>>,
    pub version: String,
}
pub trait Provider {
    fn get(&mut self, e: &Entry) -> Result<Secret, &'static str>;
}

#[derive(Clone, Copy, Debug)]
pub struct Owner {
    pub uid: u32,
    pub gid: u32,
}
#[cfg(unix)]
pub fn owner(name: &str) -> Result<Owner, &'static str> {
    if name.is_empty() || name.starts_with('-') {
        return Err("InvalidOwner");
    }
    let id = |flag| {
        let o = std::process::Command::new("id")
            .args([flag, "--", name])
            .output()
            .map_err(|_| "OwnerLookup")?;
        if !o.status.success() {
            return Err("OwnerLookup");
        }
        std::str::from_utf8(&o.stdout)
            .map_err(|_| "OwnerLookup")?
            .trim()
            .parse::<u32>()
            .map_err(|_| "OwnerLookup")
    };
    Ok(Owner {
        uid: id("-u")?,
        gid: id("-g")?,
    })
}
#[cfg(not(unix))]
pub fn owner(_: &str) -> Result<Owner, &'static str> {
    Err("UnixRequired")
}

/// All values are fetched before any output is created. Existing directories must be empty.
/// On a write failure only files created by this invocation are removed.
#[cfg(unix)]
pub fn fetch(
    m: &Manifest,
    p: &mut impl Provider,
    out: &Path,
    owner: Owner,
    stdout: &mut impl Write,
) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt, chown};
    let mut values = Vec::with_capacity(m.secret.len());
    for e in &m.secret {
        let s = p.get(e).map_err(|kind| format!("{} {kind}", e.name))?;
        if s.version.is_empty()
            || !s
                .version
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(format!("{} InvalidVersion", e.name));
        }
        values.push(s);
    }
    match std::fs::symlink_metadata(out) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {
            if std::fs::read_dir(out).map_err(|_| "OutputRead")?.next().is_some() {
                return Err("OutputNotEmpty".into());
            }
        }
        Ok(_) => return Err("UnsafeOutput".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(out)
                .map_err(|_| "OutputCreate")?;
        }
        Err(_) => return Err("OutputRead".into()),
    }
    std::fs::set_permissions(out, std::fs::Permissions::from_mode(0o700)).map_err(|_| "OutputMode")?;
    let mut written: Vec<PathBuf> = Vec::new();
    let result = (|| {
        for (e, s) in m.secret.iter().zip(&values) {
            let path = out.join(&e.name);
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o400)
                .open(&path)
                .map_err(|_| "OutputCreate")?;
            written.push(path.clone());
            // Enforce the contract even when ExecStartPre inherits a restrictive umask.
            f.set_permissions(std::fs::Permissions::from_mode(0o400))
                .map_err(|_| "OutputMode")?;
            f.write_all(&s.bytes).map_err(|_| "OutputWrite")?;
            f.sync_all().map_err(|_| "OutputWrite")?;
            chown(&path, Some(owner.uid), Some(owner.gid)).map_err(|_| "OutputOwner")?;
        }
        chown(out, Some(owner.uid), Some(owner.gid)).map_err(|_| "OutputOwner")?;
        Ok::<_, &str>(())
    })();
    if let Err(error) = result {
        for path in written {
            std::fs::remove_file(path).map_err(|_| "OutputCleanup")?;
        }
        return Err(error.into());
    }
    for (e, s) in m.secret.iter().zip(&values) {
        writeln!(stdout, "{} {}", e.name, s.version).map_err(|_| "OutputLog")?;
    }
    Ok(())
}
#[cfg(not(unix))]
pub fn fetch(
    _: &Manifest,
    _: &mut impl Provider,
    _: &Path,
    _: Owner,
    _: &mut impl Write,
) -> Result<(), String> {
    Err("UnixRequired".into())
}

/// Extract a selected JSON string without ever returning a JSON parser's diagnostic.
pub fn select(bytes: Zeroizing<Vec<u8>>, key: Option<&str>) -> Result<Zeroizing<Vec<u8>>, &'static str> {
    let Some(key) = key else { return Ok(bytes) };
    let mut v: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| "SecretJson")?;
    let mut value = v.get_mut(key).map(serde_json::Value::take).unwrap_or_default();
    // Zeroize every string remaining in the temporary parse tree as well as the selected one.
    fn erase(v: &mut serde_json::Value) {
        use zeroize::Zeroize;
        match v {
            serde_json::Value::String(s) => s.zeroize(),
            serde_json::Value::Array(a) => a.iter_mut().for_each(erase),
            serde_json::Value::Object(m) => m.values_mut().for_each(erase),
            _ => {}
        }
    }
    erase(&mut v);
    match value {
        serde_json::Value::String(s) => Ok(Zeroizing::new(s.into_bytes())),
        _ => {
            erase(&mut value);
            Err("SecretJsonKey")
        }
    }
}

/// Parse/check before initializing a provider (including its runtime and credential chain).
pub fn run<P: Provider>(
    args: &[String],
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    init: impl FnOnce() -> Result<P, &'static str>,
) -> i32 {
    let result = (|| {
        let Some(mode) = args.first() else {
            return Err((2, "Usage".into()));
        };
        if !["fetch", "--check"].contains(&mode.as_str()) || args.len() % 2 != 1 {
            return Err((2, "Usage".into()));
        }
        let mut opts = std::collections::BTreeMap::new();
        for pair in args[1..].chunks_exact(2) {
            if !["--manifest", "--out", "--owner", "--mode"].contains(&pair[0].as_str())
                || opts.insert(pair[0].as_str(), pair[1].as_str()).is_some()
            {
                return Err((2, "Usage".into()));
            }
        }
        let path = opts.get("--manifest").ok_or((2, "ManifestRequired".into()))?;
        let text = Zeroizing::new(std::fs::read_to_string(path).map_err(|_| (2, "ManifestRead".into()))?);
        let m = manifest(&text).map_err(|e| (2, e.into()))?;
        if mode == "--check" {
            if opts.len() != 1 {
                return Err((2, "CheckOptions".into()));
            }
            return Ok(());
        }
        if opts.get("--mode") != Some(&"0400") {
            return Err((2, "ModeMustBe0400".into()));
        }
        let out = opts.get("--out").ok_or((2, "OutputRequired".into()))?;
        let own =
            owner(opts.get("--owner").ok_or((2, "OwnerRequired".into()))?).map_err(|e| (2, e.into()))?;
        let mut p = init().map_err(|e| (2, e.into()))?;
        fetch(&m, &mut p, Path::new(out), own, stdout).map_err(|e| (1, e))
    })();
    match result {
        Ok(()) => 0,
        Err((code, error)) => {
            let _ = writeln!(stderr, "{error}");
            code
        }
    }
}

impl std::fmt::Debug for Manifest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Manifest { [redacted] }")
    }
}

impl std::fmt::Debug for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Entry { [redacted] }")
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret { [redacted] }")
    }
}
