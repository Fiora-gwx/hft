//! Canonical JSON (keys sorted at every level, independent of serde_json's map order), the content
//! hash and dotted-key lookup used by `verify`.

use serde_json::Value;
use sha2::{Digest, Sha256};

fn esc(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

fn write(v: &Value, pretty: bool, indent: usize, out: &mut String) {
    let pad = |n: usize| "  ".repeat(n);
    match v {
        Value::Object(m) => {
            if m.is_empty() {
                out.push_str("{}");
                return;
            }
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                if pretty {
                    out.push('\n');
                    out.push_str(&pad(indent + 1));
                }
                out.push_str(&esc(k));
                out.push(':');
                if pretty {
                    out.push(' ');
                }
                write(&m[*k], pretty, indent + 1, out);
            }
            if pretty {
                out.push('\n');
                out.push_str(&pad(indent));
            }
            out.push('}');
        }
        Value::Array(a) => {
            if a.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                if pretty {
                    out.push('\n');
                    out.push_str(&pad(indent + 1));
                }
                write(x, pretty, indent + 1, out);
            }
            if pretty {
                out.push('\n');
                out.push_str(&pad(indent));
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// Pretty canonical form (two-space indent, sorted keys, trailing newline): the file format.
pub fn pretty(v: &Value) -> String {
    let mut s = String::new();
    write(v, true, 0, &mut s);
    s.push('\n');
    s
}

/// Compact canonical form: the hashed form.
pub fn compact(v: &Value) -> String {
    let mut s = String::new();
    write(v, false, 0, &mut s);
    s
}

/// `host_profile_hash`: SHA-256 (lower-case hex) of the compact canonical form of the profile
/// without `host_profile_hash` and `verify`.
pub fn profile_hash(v: &Value) -> String {
    let mut c = v.clone();
    if let Value::Object(m) = &mut c {
        m.remove("host_profile_hash");
        m.remove("verify");
    }
    let d = Sha256::digest(compact(&c).as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// One value found for a dotted key; `path` is the concrete key (`irq.eni_b[2].effective_affinity_list`).
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub path: String,
    pub value: Option<Value>,
}

/// Looks up a dotted key. Object keys may themselves contain dots (`hftbatch.slice`); a segment
/// ending in `[*]` expands an array. A missing key yields one [`Found`] with `value = None`.
pub fn lookup(v: &Value, key: &str) -> Vec<Found> {
    let segs: Vec<&str> = key.split('.').collect();
    let mut out = Vec::new();
    walk(v, &segs, String::new(), &mut out);
    if out.is_empty() {
        out.push(Found {
            path: key.to_string(),
            value: None,
        });
    }
    out
}

fn join(prefix: &str, k: &str) -> String {
    if prefix.is_empty() {
        k.to_string()
    } else {
        format!("{prefix}.{k}")
    }
}

fn walk(v: &Value, segs: &[&str], prefix: String, out: &mut Vec<Found>) {
    if segs.is_empty() {
        out.push(Found {
            path: prefix,
            value: Some(v.clone()),
        });
        return;
    }
    let Value::Object(m) = v else {
        out.push(Found {
            path: join(&prefix, &segs.join(".")),
            value: None,
        });
        return;
    };
    // Longest object key equal to a dot-joined prefix of the remaining segments.
    for n in (1..=segs.len()).rev() {
        let cand = segs[..n].join(".");
        let (name, wild) = match cand.strip_suffix("[*]") {
            Some(b) => (b.to_string(), true),
            None => (cand.clone(), false),
        };
        if let Some(x) = m.get(&name) {
            let p = join(&prefix, &name);
            if wild {
                match x {
                    Value::Array(a) => {
                        for (i, e) in a.iter().enumerate() {
                            walk(e, &segs[n..], format!("{p}[{i}]"), out);
                        }
                    }
                    other => walk(other, &segs[n..], p, out),
                }
            } else {
                walk(x, &segs[n..], p, out);
            }
            return;
        }
    }
    out.push(Found {
        path: join(&prefix, &segs.join(".")),
        value: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_and_lookup() {
        let v = json!({"b": 1, "a": {"y.z": [{"q": 1}, {"q": 2}], "x": null}});
        assert_eq!(compact(&v), r#"{"a":{"x":null,"y.z":[{"q":1},{"q":2}]},"b":1}"#);
        let f = lookup(&v, "a.y.z[*].q");
        assert_eq!(f.len(), 2);
        assert_eq!(f[1].path, "a.y.z[1].q");
        assert_eq!(f[1].value, Some(json!(2)));
        assert_eq!(lookup(&v, "a.nope")[0].value, None);
    }
}
