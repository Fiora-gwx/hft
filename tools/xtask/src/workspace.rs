//! Workspace layout helpers shared by `ci`, `hot-check` and others: members of the root `Cargo.toml`,
//! package names and README frontmatter.

use std::path::Path;

use crate::EnvError;
use crate::doc_check::md::Doc;

/// One workspace member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// Directory relative to the root (`crates/hft-oms`).
    pub dir: String,
    /// `[package] name`.
    pub name: String,
}

fn read_toml(p: &Path) -> Result<toml::Table, EnvError> {
    let text = std::fs::read_to_string(p).map_err(|e| EnvError(format!("read {}: {e}", p.display())))?;
    text.parse::<toml::Table>()
        .map_err(|e| EnvError(format!("parse {}: {e}", p.display())))
}

/// `[package] name` of the crate in `dir` (relative to `root`).
pub fn package_name(root: &Path, dir: &str) -> Option<String> {
    let t = read_toml(&root.join(dir).join("Cargo.toml")).ok()?;
    t.get("package")?.get("name")?.as_str().map(str::to_string)
}

/// Members of the root `Cargo.toml` (`[workspace] members`; a trailing `/*` expands to every
/// subdirectory with a `Cargo.toml`), sorted by directory.
pub fn members(root: &Path) -> Result<Vec<Member>, EnvError> {
    let t = read_toml(&root.join("Cargo.toml"))?;
    let pats: Vec<String> = t
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let mut dirs = Vec::new();
    for p in pats {
        if let Some(base) = p.strip_suffix("/*") {
            if let Ok(rd) = std::fs::read_dir(root.join(base)) {
                for e in rd.flatten() {
                    if e.path().join("Cargo.toml").is_file() {
                        dirs.push(format!("{base}/{}", e.file_name().to_string_lossy()));
                    }
                }
            }
        } else if root.join(&p).join("Cargo.toml").is_file() {
            dirs.push(p);
        }
    }
    dirs.sort();
    dirs.dedup();
    Ok(dirs
        .into_iter()
        .map(|d| Member {
            name: package_name(root, &d).unwrap_or_else(|| d.rsplit('/').next().unwrap_or(&d).to_string()),
            dir: d,
        })
        .collect())
}

/// Frontmatter value `key` of `<dir>/README.md`.
pub fn readme_fm(root: &Path, dir: &str, key: &str) -> Option<String> {
    let rel = format!("{dir}/README.md");
    let text = std::fs::read_to_string(root.join(&rel)).ok()?;
    Doc::new(&rel, text).fm_get(key).map(str::to_string)
}
