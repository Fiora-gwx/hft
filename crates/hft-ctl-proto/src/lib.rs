//! `hft-ctl-proto`: the control-plane protocol shared by ctl-gateway, the Rust and Python
//! clients, `hft-cfg` and the optional chain-core gateway: length-prefixed frames, the
//! request/response JSON schema, normalization of a request into `ControlCmd` bytes and its
//! `cmd_hash`, and the role × command × parameter × confirmation permission table.
//!
//! No I/O, no threads, no clock, no runtime state. Specification: `crates/hft-ctl-proto/README.md`.
#![forbid(unsafe_code)]
#![deny(
    clippy::float_arithmetic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented,
    clippy::indexing_slicing
)]

pub mod codes;
pub mod frame;
pub mod names;
pub mod normalize;
pub mod perms;
pub mod response;
pub mod schema;

pub use codes::{Detail, ResultCode, TransportStatus, render_detail};
pub use frame::{FrameErr, encode_frame, frame_len};
pub use normalize::{BlobPart, NormErr, Normalized, cmd_hash, normalize};
pub use perms::{
    CmdClass, ConfirmRule, PermErr, PermRow, Principal, Role, RoleSet, allowed, check_confirm_fields,
    class_of, effective_role, needs_confirm_static,
};
pub use response::{CmdResponse, EngineInfo, Hex32};
pub use schema::{AccountSel, ChatType, CmdBody, CmdRequest, SchemaErr, parse_request};
pub use uuid::Uuid;

/// Protocol version (`v`).
pub const PROTO_VERSION: u8 = 2;
/// Largest frame body.
pub const MAX_FRAME: usize = 1 << 20;
/// Largest decoded `blob_b64` (= `MAX_FRAME × 3/4`).
pub const MAX_BLOB: usize = 786_432;
/// Viewer emergency-stop confirmation window (semantics: control-plane; enforced by the sequencer).
pub const VIEWER_CONFIRM_TTL_MS: u32 = 60_000;
/// Largest `expires_at_ns − issued_at_ns`.
pub const MAX_EXPIRY_NS: i64 = 300_000_000_000;

const _: () = assert!(MAX_BLOB == MAX_FRAME * 3 / 4);
