//! CPU lists (`2-15,17`) and hex CPU masks (`ff,00000001`) as sets of CPU numbers.

use std::collections::BTreeSet;

/// Parses a kernel CPU list (`0-3,8,10-11`); flags such as `managed_irq,domain,` in front of an
/// `isolcpus=` value are skipped. `None` on a malformed range.
pub fn parse_list(s: &str) -> Option<BTreeSet<u32>> {
    let mut out = BTreeSet::new();
    for part in s.trim().split(',') {
        let part = part.trim();
        if part.is_empty() || part.chars().all(|c| c.is_ascii_alphabetic() || c == '_') {
            continue;
        }
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b): (u32, u32) = (a.parse().ok()?, b.parse().ok()?);
                if a > b {
                    return None;
                }
                out.extend(a..=b);
            }
            None => {
                out.insert(part.parse().ok()?);
            }
        }
    }
    Some(out)
}

/// Formats a set as a kernel CPU list (`0-3,8`); empty set → `""`.
pub fn format_list(set: &BTreeSet<u32>) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut it = set.iter().copied().peekable();
    while let Some(start) = it.next() {
        let mut end = start;
        while it.peek() == Some(&(end + 1)) {
            end += 1;
            it.next();
        }
        out.push(if start == end {
            start.to_string()
        } else {
            format!("{start}-{end}")
        });
    }
    out.join(",")
}

/// Parses a hex CPU mask as written by the kernel (`00000000,00000004`, groups of 32 bits, most
/// significant first). `None` on non-hex input.
pub fn parse_mask(s: &str) -> Option<BTreeSet<u32>> {
    let mut out = BTreeSet::new();
    let hex: String = s.trim().chars().filter(|c| *c != ',').collect();
    if hex.is_empty() {
        return Some(out);
    }
    for (i, c) in hex.chars().rev().enumerate() {
        let d = c.to_digit(16)?;
        for b in 0..4 {
            if d & (1 << b) != 0 {
                out.insert(i as u32 * 4 + b);
            }
        }
    }
    Some(out)
}

/// Normalises a CPU list string (`"2-3,4"` → `"2-4"`); malformed input is returned unchanged.
pub fn normalize(s: &str) -> String {
    parse_list(s)
        .map(|x| format_list(&x))
        .unwrap_or_else(|| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_and_masks() {
        let s = parse_list("managed_irq,domain,2-4,7").unwrap();
        assert_eq!(format_list(&s), "2-4,7");
        assert_eq!(format_list(&parse_mask("00000000,00000104").unwrap()), "2,8");
        assert_eq!(format_list(&parse_mask("0001").unwrap()), "0");
        assert!(parse_list("3-1").is_none());
    }
}
