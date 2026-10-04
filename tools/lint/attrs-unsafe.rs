// Replaces `#![forbid(unsafe_code)]` of attrs-hot.rs in the four crates allowed to use `unsafe`
// (docs/hot-path-rules.md B5: hft-rt, hft-journal, hft-net, hft-book): every unsafe block carries a
// `// SAFETY:` comment. A crate of the four that uses no `unsafe` may keep `forbid(unsafe_code)` instead.
#![deny(clippy::undocumented_unsafe_blocks)]
