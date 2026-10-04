//! `tools/xtask/doc-check.toml`: name lists, legal frontmatter values, scanned paths, external scripts
//! (tools/README.md "实现说明").

use regex::Regex;

use crate::EnvError;

/// Config file, relative to the repo root.
pub const CONFIG: &str = "tools/xtask/doc-check.toml";

/// One pattern rule (`[[rule]]`): a regex that must not appear in the selected files.
#[derive(Debug, Clone)]
pub struct Rule {
    /// Check category: `strategy-name`, `history`, `residue`, `schedule`.
    pub check: String,
    pub id: String,
    pub pattern: Regex,
    /// File globs (repo-relative); default `**/*.md`.
    pub files: Vec<String>,
    /// Path prefixes exempt from the rule.
    pub skip: Vec<String>,
    /// Only files whose frontmatter `scope` (a README's own, else the component README's) equals this.
    pub scope: Option<String>,
    /// Lines under a heading (any level) matching this are exempt.
    pub skip_sections: Option<Regex>,
    /// Lines matching this are exempt.
    pub exempt_line: Option<Regex>,
    /// Removed from each line before matching (link targets, path pointers, citations).
    pub strip: Vec<Regex>,
    /// Lines inside fenced code blocks are exempt.
    pub skip_code: bool,
}

/// Parsed config.
#[derive(Debug, Clone)]
pub struct Config {
    pub scripts: Vec<String>,
    pub exclude: Vec<String>,
    pub evidence: Vec<String>,
    pub fm_keys: Vec<String>,
    pub fm_scope: Vec<String>,
    pub fm_status: Vec<String>,
    pub components: Vec<String>,
    pub notes_root: String,
    pub notes_lifecycles: Vec<String>,
    pub notes_classes: Vec<String>,
    pub deleted_components: Vec<String>,
    pub deleted_skip: Vec<String>,
    pub rules: Vec<Rule>,
}

fn list(t: &toml::Table, k: &str) -> Vec<String> {
    t.get(k)
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

fn table<'a>(t: &'a toml::Table, k: &str) -> Result<&'a toml::Table, EnvError> {
    t.get(k)
        .and_then(|v| v.as_table())
        .ok_or_else(|| EnvError(format!("{CONFIG}: missing [{k}]")))
}

fn re(s: &str, what: &str) -> Result<Regex, EnvError> {
    Regex::new(s).map_err(|e| EnvError(format!("{CONFIG}: {what}: bad regex: {e}")))
}

impl Config {
    pub fn parse(text: &str) -> Result<Config, EnvError> {
        let t: toml::Table = text.parse().map_err(|e| EnvError(format!("{CONFIG}: {e}")))?;
        let fm = table(&t, "frontmatter")?;
        let notes = table(&t, "agent_notes")?;
        let deleted = table(&t, "deleted")?;
        let mut rules = Vec::new();
        for (n, r) in t
            .get("rule")
            .and_then(|v| v.as_array())
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .enumerate()
        {
            let r = r
                .as_table()
                .ok_or_else(|| EnvError(format!("{CONFIG}: [[rule]] #{n} is not a table")))?;
            let s = |k: &str| r.get(k).and_then(|v| v.as_str()).map(str::to_string);
            let id = s("id").unwrap_or_else(|| format!("rule#{n}"));
            let pattern = re(
                &s("pattern").ok_or_else(|| EnvError(format!("{CONFIG}: rule {id}: no pattern")))?,
                &id,
            )?;
            let mut files = list(r, "files");
            if files.is_empty() {
                files.push("**/*.md".to_string());
            }
            rules.push(Rule {
                check: s("check").unwrap_or_else(|| "residue".to_string()),
                pattern,
                files,
                skip: list(r, "skip"),
                scope: s("scope"),
                skip_sections: s("skip_sections").map(|x| re(&x, &id)).transpose()?,
                exempt_line: s("exempt_line").map(|x| re(&x, &id)).transpose()?,
                strip: list(r, "strip")
                    .iter()
                    .map(|x| re(x, &id))
                    .collect::<Result<_, _>>()?,
                skip_code: r.get("skip_code").and_then(|v| v.as_bool()).unwrap_or(false),
                id,
            });
        }
        Ok(Config {
            scripts: list(&t, "scripts"),
            exclude: list(&t, "exclude"),
            evidence: list(&t, "evidence"),
            fm_keys: list(fm, "keys"),
            fm_scope: list(fm, "scope"),
            fm_status: list(fm, "status"),
            components: list(fm, "components"),
            notes_root: notes
                .get("root")
                .and_then(|v| v.as_str())
                .unwrap_or(".agents/notes")
                .to_string(),
            notes_lifecycles: list(notes, "lifecycles"),
            notes_classes: list(notes, "classes"),
            deleted_components: list(deleted, "components"),
            deleted_skip: list(deleted, "skip"),
            rules,
        })
    }

    pub fn load(root: &std::path::Path) -> Result<Config, EnvError> {
        let text =
            std::fs::read_to_string(root.join(CONFIG)).map_err(|e| EnvError(format!("{CONFIG}: {e}")))?;
        Config::parse(&text)
    }
}
