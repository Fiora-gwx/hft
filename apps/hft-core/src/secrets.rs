//! Startup-only secret transfer. Raw buffers are wiped when assembly releases them.
use crate::{Error, config::Config};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

#[derive(Default)]
pub struct SecretFiles {
    aliases: BTreeMap<PathBuf, PathBuf>,
    bytes: BTreeMap<PathBuf, Zeroizing<Vec<u8>>>,
}
impl std::fmt::Debug for SecretFiles {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretFiles")
            .field("files", &self.bytes.len())
            .finish()
    }
}
impl SecretFiles {
    /// Read each distinct regular file once, then unlink it before configuration validation.
    /// Resolve all aliases first so a shared REST/session key or passphrase is read only once.
    pub fn read_and_unlink(config: &Config) -> Result<Self, Vec<Error>> {
        let mut result = Self::default();
        let mut errors = Vec::new();
        for keys in config.venues.values().flat_map(|v| &v.accounts) {
            for path in [
                &keys.api_key_path,
                &keys.session_seed_path,
                &keys.rest_api_key_path,
                &keys.rest_secret_path,
            ]
            .into_iter()
            .chain(keys.passphrase_path.iter())
            {
                if result.aliases.contains_key(path) {
                    continue;
                }
                let resolved = std::fs::symlink_metadata(path).and_then(|meta| {
                    if !meta.is_file() || meta.file_type().is_symlink() {
                        return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
                    }
                    std::fs::canonicalize(path)
                });
                match resolved {
                    Ok(real) => {
                        result.aliases.insert(path.clone(), real);
                    }
                    Err(_) => errors.push(Error::config(
                        "CredentialRead",
                        "credential must be a readable regular file",
                    )),
                }
            }
        }
        let unique: std::collections::BTreeSet<_> = result.aliases.values().cloned().collect();
        for real in &unique {
            // Fixed allocation: no intermediate growth leaves key bytes in freed allocations.
            let mut bytes = Zeroizing::new(vec![0; 4097]);
            let read = (|| {
                let mut file = std::fs::File::open(real)?;
                let mut n = 0;
                while n < bytes.len() {
                    let got = file.read(&mut bytes[n..])?;
                    if got == 0 {
                        break;
                    }
                    n += got;
                }
                Ok::<_, std::io::Error>(n)
            })();
            // Also remove a successfully opened malformed/oversize secret. No values in errors.
            let removed = std::fs::remove_file(real);
            match (read, removed) {
                (Ok(n), Ok(())) if n > 0 && n <= 4096 => {
                    bytes.truncate(n);
                    result.bytes.insert(real.clone(), bytes);
                }
                _ => errors.push(Error::config(
                    "CredentialRead",
                    "cannot transfer and unlink credential",
                )),
            }
        }
        if errors.is_empty() {
            Ok(result)
        } else {
            Err(errors)
        }
    }

    pub fn get(&self, path: &Path) -> Result<&[u8], Error> {
        self.aliases
            .get(path)
            .and_then(|p| self.bytes.get(p))
            .map(|b| b.as_slice())
            .ok_or_else(|| Error::config("CredentialRead", "credential was not transferred at startup"))
    }
}
