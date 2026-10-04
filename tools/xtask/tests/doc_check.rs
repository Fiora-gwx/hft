//! `cargo xtask doc-check` helpers (tools/TESTS.md TOOL-067, TOOL-068).

use xtask::doc_check::{doc_length, slugs};

#[test]
fn tool_067_anchor_slug() {
    let rows: [(&str, &[&str], &str); 6] = [
        ("TOOL-067.1", &["1. 金样"], "1-金样"),
        ("TOOL-067.2", &["延迟与资源契约"], "延迟与资源契约"),
        ("TOOL-067.3", &["分段(`StageSet`)"], "分段stageset"),
        (
            "TOOL-067.4",
            &["cargo xtask decoupling-gate"],
            "cargo-xtask-decoupling-gate",
        ),
        (
            "TOOL-067.5",
            &["质量门(`quality-gates.yml`)"],
            "质量门quality-gatesyml",
        ),
        ("TOOL-067.6", &["示例", "其他", "示例"], "示例-1"),
    ];
    for (row, headings, want) in rows {
        let got = slugs(headings.iter().copied());
        assert_eq!(got.last().map(String::as_str), Some(want), "{row}: {got:?}");
    }
}

#[test]
fn tool_068_length_metric() {
    let fenced = format!("```\n{}\n```\n是", "汉".repeat(100));
    let rows: [(&str, &str, usize); 5] = [
        ("TOOL-068.1", "单写者规则", 5),
        ("TOOL-068.2", "use hft-oms now", 3),
        ("TOOL-068.3", "[链接](https://example.com/a/b)", 2),
        ("TOOL-068.4", &fenced, 1),
        ("TOOL-068.5", ",。——", 0),
    ];
    for (row, text, want) in rows {
        assert_eq!(doc_length(text), want, "{row}");
    }
}
