//! `cargo xtask deps`: dependency direction and forbidden dependencies (tools/README.md "cargo xtask deps").
//!
//! Reads `tools/xtask/deps.toml` (per-package tier and allowed internal dependencies, plus the rule lists) and
//! `cargo metadata` of the workspace at the repo root. Workspace members are identified by their directory
//! relative to the root (`crates/hft-oms`, `apps/hft-core`, `strategies/x`); everything else is external and
//! identified by its package name. The dependency closure of a package follows normal and build edges only
//! (dev-dependencies are not transitive); "any edge" means normal, build or dev.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

use crate::{EXIT_ENV, EXIT_FAIL, EXIT_OK, EnvError, rel_path, write_json};

/// Config file, relative to the repo root.
pub const CONFIG: &str = "tools/xtask/deps.toml";

/// One `[pkg."<path>"]` table.
#[derive(Debug, Clone, Default)]
pub struct PkgRule {
    pub tier: String,
    pub deps: Vec<String>,
    pub dev: Vec<String>,
    pub dev_tier_ok: bool,
}

/// Parsed `deps.toml`.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Key = package path or pattern (`strategies/*`).
    pub pkgs: BTreeMap<String, PkgRule>,
    pub rules: toml::Table,
}

fn str_list(t: &toml::Table, k: &str) -> Vec<String> {
    t.get(k)
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

impl Config {
    pub fn parse(text: &str) -> Result<Config, EnvError> {
        let t: toml::Table = text.parse().map_err(|e| EnvError(format!("{CONFIG}: {e}")))?;
        let mut pkgs = BTreeMap::new();
        if let Some(p) = t.get("pkg").and_then(|v| v.as_table()) {
            for (k, v) in p {
                let v = v
                    .as_table()
                    .ok_or_else(|| EnvError(format!("{CONFIG}: [pkg.\"{k}\"] is not a table")))?;
                let tier = v.get("tier").and_then(|x| x.as_str()).unwrap_or("").to_string();
                if !["pure", "hot", "warm", "cold", "dev"].contains(&tier.as_str()) {
                    return Err(EnvError(format!(
                        "{CONFIG}: [pkg.\"{k}\"] tier '{tier}' is not pure/hot/warm/cold/dev"
                    )));
                }
                pkgs.insert(
                    k.clone(),
                    PkgRule {
                        tier,
                        deps: str_list(v, "deps"),
                        dev: str_list(v, "dev"),
                        dev_tier_ok: v.get("dev_tier_ok").and_then(|x| x.as_bool()).unwrap_or(false),
                    },
                );
            }
        }
        let rules = t
            .get("rules")
            .and_then(|v| v.as_table())
            .cloned()
            .unwrap_or_default();
        Ok(Config { pkgs, rules })
    }

    pub fn load(root: &Path) -> Result<Config, EnvError> {
        let p = root.join(CONFIG);
        let text = std::fs::read_to_string(&p).map_err(|e| EnvError(format!("{CONFIG}: {e}")))?;
        Config::parse(&text)
    }

    /// The `[pkg]` key that governs `path`: an exact key, else a matching `dir/*` pattern.
    pub fn key_for(&self, path: &str) -> Option<&str> {
        if let Some((k, _)) = self.pkgs.get_key_value(path) {
            return Some(k);
        }
        self.pkgs
            .keys()
            .find(|k| pattern_match(k, path) && !self.pkgs.contains_key(path))
            .map(String::as_str)
    }

    pub fn rule_for(&self, path: &str) -> Option<&PkgRule> {
        self.key_for(path).and_then(|k| self.pkgs.get(k))
    }

    pub fn list(&self, k: &str) -> Vec<String> {
        str_list(&self.rules, k)
    }

    fn one(&self, k: &str) -> String {
        self.rules
            .get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    }
}

/// `pat` matches `name`: exact, or `prefix*` / `dir/*` (a single trailing `*`).
pub fn pattern_match(pat: &str, name: &str) -> bool {
    match pat.strip_suffix('*') {
        Some(prefix) => {
            name.starts_with(prefix) && name.len() > prefix.len() && !name[prefix.len()..].contains('/')
        }
        None => pat == name,
    }
}

fn any_match(pats: &[String], name: &str) -> bool {
    pats.iter().any(|p| pattern_match(p, name))
}

/// Edge kind of a resolved dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Normal,
    Build,
    Dev,
}

impl Kind {
    fn section(self) -> &'static str {
        match self {
            Kind::Normal => "[dependencies]",
            Kind::Build => "[build-dependencies]",
            Kind::Dev => "[dev-dependencies]",
        }
    }
}

/// A dependency declaration of one package (from its manifest).
#[derive(Debug, Clone)]
pub struct Decl {
    /// Package name of the dependency.
    pub name: String,
    pub kind: Kind,
    pub features: Vec<String>,
}

/// One package of the graph.
#[derive(Debug, Clone)]
pub struct Node {
    /// Repo-relative directory for workspace members, package name otherwise.
    pub label: String,
    pub member: bool,
    pub name: String,
    /// Resolved edges: (target node index, kinds).
    pub edges: Vec<(usize, BTreeSet<Kind>)>,
    pub decls: Vec<Decl>,
    /// `[features]` table of the package.
    pub features: BTreeMap<String, Vec<String>>,
}

/// The resolved dependency graph.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
}

impl Graph {
    /// Build from `cargo metadata --format-version 1` JSON.
    /// `test_cfgs`: dependency targets (e.g. `cfg(loom)`) whose edges count as dev edges (test-only
    /// `--cfg` builds, never in a shipped build).
    pub fn from_metadata(root: &Path, meta: &Value, test_cfgs: &[String]) -> Result<Graph, EnvError> {
        let bad = |w: &str| EnvError(format!("cargo metadata: missing {w}"));
        let members: BTreeSet<&str> = meta["workspace_members"]
            .as_array()
            .ok_or_else(|| bad("workspace_members"))?
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        let root_c = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        let mut idx: HashMap<String, usize> = HashMap::new();
        let mut nodes = Vec::new();
        for p in meta["packages"].as_array().ok_or_else(|| bad("packages"))? {
            let id = p["id"].as_str().unwrap_or("").to_string();
            let name = p["name"].as_str().unwrap_or("").to_string();
            let member = members.contains(id.as_str());
            let label = if member {
                let mp = Path::new(p["manifest_path"].as_str().unwrap_or(""));
                let dir = mp.parent().unwrap_or(mp);
                let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
                rel_path(&root_c, &dir)
            } else {
                name.clone()
            };
            let decls = p["dependencies"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|d| Decl {
                            name: d["name"].as_str().unwrap_or("").to_string(),
                            kind: match d["kind"].as_str() {
                                Some("dev") => Kind::Dev,
                                Some("build") => Kind::Build,
                                _ => Kind::Normal,
                            },
                            features: d["features"]
                                .as_array()
                                .map(|f| f.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                                .unwrap_or_default(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let features = p["features"]
                .as_object()
                .map(|o| {
                    o.iter()
                        .map(|(k, v)| {
                            let l = v
                                .as_array()
                                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                                .unwrap_or_default();
                            (k.clone(), l)
                        })
                        .collect()
                })
                .unwrap_or_default();
            idx.insert(id, nodes.len());
            nodes.push(Node {
                label,
                member,
                name,
                edges: Vec::new(),
                decls,
                features,
            });
        }
        for n in meta["resolve"]["nodes"]
            .as_array()
            .ok_or_else(|| bad("resolve.nodes"))?
        {
            let Some(&from) = n["id"].as_str().and_then(|i| idx.get(i)) else {
                continue;
            };
            let mut edges = Vec::new();
            for d in n["deps"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
                let Some(&to) = d["pkg"].as_str().and_then(|i| idx.get(i)) else {
                    continue;
                };
                let kinds: BTreeSet<Kind> = d["dep_kinds"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|k| match k["kind"].as_str() {
                                _ if k["target"]
                                    .as_str()
                                    .is_some_and(|t| test_cfgs.iter().any(|c| c == t)) =>
                                {
                                    Kind::Dev
                                }
                                Some("dev") => Kind::Dev,
                                Some("build") => Kind::Build,
                                _ => Kind::Normal,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                if to != from {
                    edges.push((to, kinds));
                }
            }
            nodes[from].edges = edges;
        }
        Ok(Graph { nodes })
    }

    fn find(&self, label: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.label == label)
    }

    /// Shortest path (labels) from `from` along normal/build edges to the first node satisfying `hit`;
    /// `first_any` also follows the root's dev edges.
    fn path_to(&self, from: usize, first_any: bool, hit: &dyn Fn(&Node) -> bool) -> Option<Vec<String>> {
        let mut prev: HashMap<usize, usize> = HashMap::new();
        let mut q = VecDeque::from([from]);
        let mut seen = BTreeSet::from([from]);
        while let Some(u) = q.pop_front() {
            for (v, kinds) in &self.nodes[u].edges {
                let ok = kinds.iter().any(|k| *k != Kind::Dev) || (first_any && u == from);
                if !ok || !seen.insert(*v) {
                    continue;
                }
                prev.insert(*v, u);
                if hit(&self.nodes[*v]) {
                    let mut path = vec![self.nodes[*v].label.clone()];
                    let mut c = *v;
                    while let Some(&p) = prev.get(&c) {
                        path.push(self.nodes[p].label.clone());
                        c = p;
                    }
                    path.reverse();
                    return Some(path);
                }
                q.push_back(*v);
            }
        }
        None
    }
}

/// One rule violation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Violation {
    /// Rule number of tools/README.md "cargo xtask deps" (0 = unregistered package).
    pub rule: u8,
    pub package: String,
    pub detail: String,
}

fn is_strategy(label: &str, cfg: &Config) -> bool {
    label.starts_with("strategies/") && label != cfg.one("registry")
}

fn enables(features: &[String], dep: &str, feat: &str) -> bool {
    features
        .iter()
        .any(|f| f == &format!("{dep}/{feat}") || f == &format!("{dep}?/{feat}"))
}

/// Check the graph against the config; returns the violations (sorted, deduplicated).
pub fn check(g: &Graph, cfg: &Config) -> Vec<Violation> {
    let mut out = BTreeSet::new();
    let mut v = |rule: u8, package: &str, detail: String| {
        out.insert(Violation {
            rule,
            package: package.to_string(),
            detail,
        });
    };
    let tier_of = |label: &str| cfg.rule_for(label).map(|r| r.tier.clone());
    let lib = |label: &str| label.starts_with("crates/") || label.starts_with("strategies/");
    let journal = cfg.one("journal");
    let writer = cfg.one("journal_writer_feature");
    let testing = cfg.one("testing_feature");
    let registry = cfg.one("registry");
    let rt = cfg.one("rt");

    for (i, n) in g.nodes.iter().enumerate() {
        if !n.member {
            continue;
        }
        let l = n.label.as_str();
        let Some(rule) = cfg.rule_for(l) else {
            v(
                0,
                l,
                format!(
                    "workspace member `{l}` ({}) has no [pkg] entry in {CONFIG}",
                    n.name
                ),
            );
            continue;
        };
        // Rules 1, 5, 6, 9 (direct edges).
        for (to, kinds) in &n.edges {
            let t = &g.nodes[*to];
            for &k in kinds {
                if t.member {
                    let allowed_normal = any_match(&rule.deps, &t.label);
                    let t_dev = tier_of(&t.label).as_deref() == Some("dev");
                    let allowed = match k {
                        Kind::Dev => allowed_normal || any_match(&rule.dev, &t.label) || t_dev,
                        _ => allowed_normal,
                    };
                    if !allowed {
                        v(
                            1,
                            l,
                            format!(
                                "`{l} → {}` ({}) is not in the allowed set of {CONFIG}",
                                t.label,
                                k.section()
                            ),
                        );
                    }
                    if k != Kind::Dev && t_dev && !rule.dev_tier_ok {
                        v(
                            5,
                            l,
                            format!("`{l} → {}`: dev-tier package outside [dev-dependencies]", t.label),
                        );
                    }
                    if k != Kind::Dev
                        && matches!(rule.tier.as_str(), "pure" | "hot")
                        && tier_of(&t.label).as_deref() == Some("cold")
                    {
                        v(
                            1,
                            l,
                            format!(
                                "`{l} → {}`: {} package depends on a cold package",
                                t.label, rule.tier
                            ),
                        );
                    }
                    if l == "crates/hft-venue" && t.label.starts_with("crates/venue-") {
                        v(
                            6,
                            l,
                            format!("`{l} → {}`: hft-venue depends on a venue crate", t.label),
                        );
                    }
                    if is_strategy(&t.label, cfg) && l != registry {
                        v(
                            9,
                            l,
                            format!(
                                "`{l} → {}` ({}): only {registry} depends on strategy crates",
                                t.label,
                                k.section()
                            ),
                        );
                    }
                    if t.label == registry && !any_match(&cfg.list("registry_users"), l) {
                        v(
                            9,
                            l,
                            format!(
                                "`{l} → {registry}` ({}): only {} depend on the registry",
                                k.section(),
                                cfg.list("registry_users").join(", ")
                            ),
                        );
                    }
                }
            }
        }
        // Rule 3: venue-* direct normal/build dependencies.
        if l.starts_with("crates/venue-") {
            for (to, kinds) in &n.edges {
                let t = &g.nodes[*to];
                if kinds.iter().all(|k| *k == Kind::Dev) {
                    continue;
                }
                let ok = if t.member {
                    any_match(&cfg.list("venue_internal"), &t.label)
                } else {
                    any_match(&cfg.list("venue_codecs"), &t.label)
                };
                if !ok {
                    v(
                        3,
                        l,
                        format!(
                            "`{l} → {}`: venue crates only depend on {} and the codec whitelist",
                            t.label,
                            cfg.list("venue_internal").join(", ")
                        ),
                    );
                }
            }
        }
        // Rule 2: forbidden crates in the closure of hot/pure library crates.
        if lib(l) && matches!(rule.tier.as_str(), "pure" | "hot") {
            let bad = cfg.list("hot_pure_forbidden");
            for f in &bad {
                if let Some(p) = g.path_to(i, false, &|x: &Node| !x.member && x.label == *f) {
                    v(
                        2,
                        l,
                        format!("{} package closure contains `{f}`: {}", rule.tier, p.join(" → ")),
                    );
                }
            }
        }
        // Rule 4: chain crates only in the closure of the allowed packages.
        if !any_match(&cfg.list("chain_allowed"), l) {
            let chain = cfg.list("chain_crates");
            if let Some(p) = g.path_to(i, false, &|x: &Node| !x.member && any_match(&chain, &x.label)) {
                v(4, l, format!("closure contains a chain crate: {}", p.join(" → ")));
            }
        }
        // Rule 7: no hft-rt in the closure of L1 and strategy crates.
        if (any_match(&cfg.list("no_rt"), l) || is_strategy(l, cfg))
            && let Some(p) = g.path_to(i, false, &|x: &Node| x.label == rt)
        {
            v(
                7,
                l,
                format!("closure contains {rt} (rule T4): {}", p.join(" → ")),
            );
        }
        // Rule 8: hft-journal `writer` only in the closure of the allowed apps; checked for the
        // packages that only read journals (crates/AGENTS.md: the other apps and hft-py).
        if any_match(&cfg.list("journal_writer_roots"), l)
            && !any_match(&cfg.list("journal_writer_allowed"), l)
        {
            let jname = journal_name(g, &journal);
            let enabling = |x: &Node| {
                x.decls.iter().any(|d| {
                    d.kind != Kind::Dev
                        && (d.name == jname && d.features.contains(&writer)
                            || g.nodes.iter().any(|t| {
                                t.member
                                    && t.name == d.name
                                    && d.features.iter().any(|f| {
                                        t.features.get(f).is_some_and(|e| enables(e, &jname, &writer))
                                    })
                            }))
                })
            };
            if enabling(n) {
                v(8, l, format!("enables `{jname}/{writer}`"));
            } else if let Some(p) = g.path_to(i, false, &|x: &Node| enabling(x)) {
                v(
                    8,
                    l,
                    format!("closure enables `{jname}/{writer}` via {}", p.join(" → ")),
                );
            }
        }
        // Rule 9: closures (incl. own dev-dependencies) without registry or strategy crates.
        if any_match(&cfg.list("no_strategy_closure"), l)
            && let Some(p) = g.path_to(i, true, &|x: &Node| {
                x.label == registry || is_strategy(&x.label, cfg)
            })
        {
            v(
                9,
                l,
                format!(
                    "closure (incl. dev-dependencies) contains a strategy crate: {}",
                    p.join(" → ")
                ),
            );
        }
        // Rule 10: hft-verify closure without hft crates and network libraries.
        if l == cfg.one("verify") {
            let net = cfg.list("verify_forbidden");
            if let Some(p) = g.path_to(i, false, &|x: &Node| x.member || any_match(&net, &x.label)) {
                v(
                    10,
                    l,
                    format!(
                        "closure contains an hft crate or network library: {}",
                        p.join(" → ")
                    ),
                );
            }
        }
        // Rule 11: AWS secret SDKs only in hft-secrets' closure.
        if l != cfg.one("secrets") {
            let aws = cfg.list("secrets_crates");
            if let Some(p) = g.path_to(i, false, &|x: &Node| !x.member && any_match(&aws, &x.label)) {
                v(
                    11,
                    l,
                    format!("closure contains an AWS secrets SDK: {}", p.join(" → ")),
                );
            }
        }
        // Rule 12: the `testing` feature only in [dev-dependencies].
        for d in &n.decls {
            if d.kind != Kind::Dev && d.features.contains(&testing) {
                v(
                    12,
                    l,
                    format!(
                        "`{l} → {}` enables the `{testing}` feature in {}",
                        d.name,
                        d.kind.section()
                    ),
                );
            }
        }
        if let Some(def) = n.features.get("default")
            && def
                .iter()
                .any(|f| f == &testing || f.ends_with(&format!("/{testing}")))
        {
            v(12, l, format!("default features enable `{testing}`"));
        }
    }
    out.into_iter().collect()
}

/// Package name of the member at `label` (falls back to the directory name).
fn journal_name(g: &Graph, label: &str) -> String {
    g.find(label)
        .map(|i| g.nodes[i].name.clone())
        .unwrap_or_else(|| label.rsplit('/').next().unwrap_or(label).to_string())
}

/// `cargo metadata` of the workspace at `root`.
pub fn metadata(root: &Path) -> Result<Value, EnvError> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let out = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .output()
        .map_err(|e| EnvError(format!("run cargo metadata: {e}")))?;
    if !out.status.success() {
        return Err(EnvError(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| EnvError(format!("cargo metadata JSON: {e}")))
}

/// Entry point: returns the exit code.
pub fn run(root: &Path) -> i32 {
    let res = Config::load(root).and_then(|cfg| {
        let meta = metadata(root)?;
        Ok((Graph::from_metadata(root, &meta, &cfg.list("test_cfgs"))?, cfg))
    });
    let (g, cfg) = match res {
        Ok(x) => x,
        Err(e) => {
            eprintln!("deps: error: {e}");
            return EXIT_ENV;
        }
    };
    let vs = check(&g, &cfg);
    for x in &vs {
        eprintln!("[rule {}] {}: {}", x.rule, x.package, x.detail);
    }
    let members = g.nodes.iter().filter(|n| n.member).count();
    let j = json!({
        "members": members,
        "violations": vs.iter().map(|x| json!({"rule": x.rule, "package": x.package, "detail": x.detail})).collect::<Vec<_>>(),
    });
    if let Err(e) = write_json(root, "deps", &j) {
        eprintln!("deps: error: {e}");
        return EXIT_ENV;
    }
    eprintln!(
        "deps: members={members} violations={} -> {}",
        vs.len(),
        if vs.is_empty() { "PASS" } else { "FAIL" }
    );
    if vs.is_empty() { EXIT_OK } else { EXIT_FAIL }
}
