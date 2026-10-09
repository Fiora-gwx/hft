//! Permission table (single source; README "权限表" is rendered from `PERM_ROWS` +
//! `OPTIONAL_ROWS` and compared by `cargo xtask doc-check`), principal table and role codes.
//! Pure functions of the command: no runtime state is read.

use hft_types::{KillLevel, RunMode};

use crate::normalize::NormErr;
use crate::schema::{AnnotateKind, CmdBody, CmdRequest, NoticeKind};

/// Human role. Codes: Viewer 1, Trader 2, Admin 4 (3 retired, never reused; 0 = process principal).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum Role {
    Viewer = 1,
    Trader = 2,
    Admin = 4,
}

impl Role {
    /// Every role, lowest first.
    pub const ALL: [Role; 3] = [Role::Viewer, Role::Trader, Role::Admin];

    /// Bit in a `RoleSet` (`1 << (code − 1)`).
    pub const fn bit(self) -> u8 {
        1 << (self as u8 - 1)
    }

    pub fn name(self) -> &'static str {
        match self {
            Role::Viewer => "Viewer",
            Role::Trader => "Trader",
            Role::Admin => "Admin",
        }
    }

    /// Role of a name (`"Viewer"`, `"Trader"`, `"Admin"`).
    pub fn from_name(s: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|r| r.name() == s)
    }
}

/// Invalid role code or role-set bits.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct RoleErr;

impl TryFrom<u8> for Role {
    type Error = RoleErr;
    fn try_from(v: u8) -> Result<Role, RoleErr> {
        match v {
            1 => Ok(Role::Viewer),
            2 => Ok(Role::Trader),
            4 => Ok(Role::Admin),
            _ => Err(RoleErr),
        }
    }
}

/// Set of roles (bit `1 << (code − 1)`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct RoleSet(u8);

impl RoleSet {
    pub const EMPTY: RoleSet = RoleSet(0);
    pub const VIEWER: RoleSet = RoleSet(Role::Viewer.bit());
    pub const TRADER: RoleSet = RoleSet(Role::Trader.bit());
    pub const ADMIN: RoleSet = RoleSet(Role::Admin.bit());
    const VALID: u8 = Role::Viewer.bit() | Role::Trader.bit() | Role::Admin.bit();

    /// Bits outside the three roles (bit 2 = retired code 3) → `Err`.
    pub fn from_bits(b: u8) -> Result<RoleSet, RoleErr> {
        if b & !Self::VALID != 0 {
            Err(RoleErr)
        } else {
            Ok(RoleSet(b))
        }
    }
    pub const fn bits(self) -> u8 {
        self.0
    }
    pub const fn with(self, r: Role) -> RoleSet {
        RoleSet(self.0 | r.bit())
    }
    pub const fn contains(self, r: Role) -> bool {
        self.0 & r.bit() != 0
    }
    pub const fn union(self, o: RoleSet) -> RoleSet {
        RoleSet(self.0 | o.0)
    }
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
    /// Highest role of the set.
    pub fn highest(self) -> Option<Role> {
        Role::ALL.into_iter().rev().find(|r| self.contains(*r))
    }
    pub fn from_roles(rs: &[Role]) -> RoleSet {
        rs.iter().fold(RoleSet::EMPTY, |s, r| s.with(*r))
    }
}

/// Principal kind of an issuer (principal table).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Principal {
    /// `tg:<from.id>` via ops_bot (human; roles from the issuer table).
    Tg,
    /// `cli:<login>` (human; roles from the issuer table).
    Cli,
    /// `ops_bot` itself.
    OpsBot,
    Pipeline,
    Reconciler,
    /// hft-core itself (`cli:systemd`, never over the socket).
    Systemd,
}

impl Principal {
    /// Principal of an issuer string; unrecognised forms → `None`.
    pub fn of_issuer(s: &str) -> Option<Principal> {
        match s {
            "ops_bot" => Some(Principal::OpsBot),
            "pipeline" => Some(Principal::Pipeline),
            "reconciler" => Some(Principal::Reconciler),
            "cli:systemd" => Some(Principal::Systemd),
            _ => {
                if let Some(id) = s.strip_prefix("tg:") {
                    (!id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())).then_some(Principal::Tg)
                } else if let Some(login) = s.strip_prefix("cli:") {
                    (!login.is_empty()).then_some(Principal::Cli)
                } else {
                    None
                }
            }
        }
    }

    /// Human principals carry roles; process principals are listed per command.
    pub const fn is_human(self) -> bool {
        matches!(self, Principal::Tg | Principal::Cli)
    }
}

/// One row of the principal table (README "主体与角色码").
#[derive(Copy, Clone, Debug)]
pub struct PrincipalDesc {
    /// Principal name used in the issuer table's `[uids]` (`ops_bot`, `pipeline`, `reconciler`, `cli`).
    pub name: &'static str,
    /// System user (`None` = the operator's own login, or no socket at all for `cli:systemd`).
    pub uid: Option<&'static str>,
    /// Issuer form.
    pub issuer: &'static str,
    pub principal: Principal,
    /// Whether it reaches the gateway over the socket.
    pub via_socket: bool,
}

/// Principal table. watchtower and treasury are not principals.
pub const PRINCIPALS: &[PrincipalDesc] = &[
    PrincipalDesc {
        name: "ops_bot",
        uid: Some("hft-opsbot"),
        issuer: "tg:<from.id>",
        principal: Principal::Tg,
        via_socket: true,
    },
    PrincipalDesc {
        name: "ops_bot",
        uid: Some("hft-opsbot"),
        issuer: "ops_bot",
        principal: Principal::OpsBot,
        via_socket: true,
    },
    PrincipalDesc {
        name: "cli",
        uid: None,
        issuer: "cli:<login>",
        principal: Principal::Cli,
        via_socket: true,
    },
    PrincipalDesc {
        name: "reconciler",
        uid: Some("hft-recon"),
        issuer: "reconciler",
        principal: Principal::Reconciler,
        via_socket: true,
    },
    PrincipalDesc {
        name: "pipeline",
        uid: Some("hft-pipeline"),
        issuer: "pipeline",
        principal: Principal::Pipeline,
        via_socket: true,
    },
    PrincipalDesc {
        name: "systemd",
        uid: None,
        issuer: "cli:systemd",
        principal: Principal::Systemd,
        via_socket: false,
    },
];

/// Chat type as seen by the permission check (only `tg:*` issuers are restricted).
pub use crate::schema::ChatType;

/// Static command class.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum CmdClass {
    Downgrade,
    Upgrade,
    Neutral,
    ReadOnly,
}

/// Static confirmation rule.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ConfirmRule {
    Never,
    Always,
    /// Decided by the sequencer from state (`SetRisk`, `SetParams`).
    IfLoosening,
    /// Viewer's `Kill`: bound to the same issuer's preview `cmd_hash` + 60 s TTL.
    PreviewBound,
}

/// The role/principal/chat combination may not issue the command (→ `Forbidden`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct PermErr;

impl PermErr {
    /// Rendered result code.
    pub const fn code(self) -> crate::ResultCode {
        crate::ResultCode::Forbidden
    }
}

/// Which commands a row covers.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Match {
    GetSchema,
    /// Default-parameter `Kill` of any level.
    KillDefault,
    /// Default-parameter `Kill{Global | Account}`.
    KillStop,
    KillKeepResting,
    KillHard,
    KillFlatten,
    SetMode(RunMode),
    Resume,
    SetParams,
    SetRisk,
    ReplaceRefdata,
    CancelAll,
    Drain,
    HedgeFix,
    WriteOffResidual,
    SmokeOrder,
    StrategyCmd {
        reduce_only: bool,
    },
    InjectWarmup,
    AdoptExchange,
    SetBalanceBaseline,
    VenueNoticeDelisting,
    VenueNoticeOther,
    SimReset,
    SimTransfer,
    SimFault,
    RecordTransfer,
    AnnotateNote,
    AnnotateIncident,
    ReconcileReport,
}

/// Extra condition on a process principal's right.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Cond {
    Any,
    /// `Kill{Global | Account}` only.
    GlobalOrAccount,
    /// A concrete `group` (not `*`).
    ConcreteGroup,
    /// `Annotate{OpsNote}` only.
    OpsNote,
}

/// One row of the permission table: the README text of its nine columns plus the semantics
/// `allowed` uses.
#[derive(Copy, Clone, Debug)]
pub struct PermRow {
    /// Columns: command, scope, class, Viewer, Trader, Admin, group chat, process principals, confirm.
    pub text: [&'static str; 9],
    pub m: Match,
    pub roles: RoleSet,
    /// Human roles only through `cli:<login>`.
    pub cli_only: bool,
    /// Allowed from a Telegram group chat.
    pub group_chat: bool,
    pub procs: &'static [(Principal, Cond)],
}

const V: RoleSet = RoleSet::VIEWER;
const T: RoleSet = RoleSet::TRADER;
const A: RoleSet = RoleSet::ADMIN;
const TA: RoleSet = T.union(A);
const VTA: RoleSet = V.union(TA);

/// Core rows (consulted by `allowed`), in README order.
pub const PERM_ROWS: &[PermRow] = &[
    PermRow {
        text: [
            "`GetSchema`(只读,gateway 本地应答)",
            "core",
            "只读",
            "✓",
            "✓",
            "✓",
            "✓",
            "pipeline",
            "否",
        ],
        m: Match::GetSchema,
        roles: VTA,
        cli_only: false,
        group_chat: true,
        procs: &[(Principal::Pipeline, Cond::Any)],
    },
    PermRow {
        text: [
            "`Kill`(Account/Venue/Global,默认参数)",
            "core",
            "降级",
            "—",
            "✓",
            "✓",
            "✓",
            "`ops_bot`(Global/Account,补发兜底期间的 `/stop`);`cli:systemd`(Global/Account,仅启动时由停机标记触发)",
            "否",
        ],
        m: Match::KillDefault,
        roles: TA,
        cli_only: false,
        group_chat: true,
        procs: &[
            (Principal::OpsBot, Cond::GlobalOrAccount),
            (Principal::Systemd, Cond::GlobalOrAccount),
        ],
    },
    PermRow {
        text: [
            "`Kill{Global}` 或 `Kill{Account}`,默认参数(非 HARD、无 flatten、`cancel_resting = true`):Viewer 紧急停止",
            "core",
            "降级",
            "✓",
            "✓",
            "✓",
            "✓",
            "—",
            "Viewer:是(预览 + 同一 issuer 60 s 内确认,绑定 `cmd_hash`,不比对 `ctl_state_version`);Trader/Admin:否",
        ],
        m: Match::KillStop,
        roles: VTA,
        cli_only: false,
        group_chat: true,
        procs: &[],
    },
    PermRow {
        text: [
            "`Kill` 改 `cancel_resting = false`",
            "core",
            "降级",
            "—",
            "✓",
            "✓",
            "—",
            "—",
            "否",
        ],
        m: Match::KillKeepResting,
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`Kill{Global}` 或 `Kill{Account}` 带 `hard`(与 Venue 组合 → `InvalidArgs`)",
            "core",
            "降级",
            "—",
            "✓",
            "✓",
            "—",
            "—",
            "否",
        ],
        m: Match::KillHard,
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`Kill{Account}` 带 `flatten`(不得与 Global、`hard` 组合)",
            "core",
            "升级(启动闸门未放行、DRAINING 期间同样可发)",
            "—",
            "✓",
            "✓",
            "—",
            "—",
            "是",
        ],
        m: Match::KillFlatten,
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`SetMode → CloseOnly`",
            "core",
            "降级",
            "—",
            "✓",
            "✓",
            "✓",
            "pipeline(group 级)",
            "否",
        ],
        m: Match::SetMode(RunMode::CloseOnly),
        roles: TA,
        cli_only: false,
        group_chat: true,
        procs: &[(Principal::Pipeline, Cond::ConcreteGroup)],
    },
    PermRow {
        text: [
            "`SetMode → Shadow`",
            "core",
            "降级",
            "—",
            "✓",
            "✓",
            "—",
            "pipeline(group 级)",
            "否",
        ],
        m: Match::SetMode(RunMode::Shadow),
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[(Principal::Pipeline, Cond::ConcreteGroup)],
    },
    PermRow {
        text: ["`SetMode → Live`", "core", "升级", "—", "✓", "✓", "—", "—", "是"],
        m: Match::SetMode(RunMode::Live),
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`Resume{source, scope}`",
            "core",
            "升级",
            "—",
            "✓",
            "✓",
            "—",
            "—",
            "是",
        ],
        m: Match::Resume,
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`SetParams`",
            "core",
            "按状态:中性;人工主体 `open_allowed` 0→1、或 Live 实例(定义见 [control-plane 类别](../../docs/subsystems/control-plane.md#类别))上改动结构字段时升级",
            "—",
            "✓(仅 `cli:<login>`)",
            "✓(仅 `cli:<login>`)",
            "—",
            "pipeline",
            "放宽时是;pipeline 主体从不确认(其 blob 若被判为升级 → `NeedsConfirm`)",
        ],
        m: Match::SetParams,
        roles: TA,
        cli_only: true,
        group_chat: false,
        procs: &[(Principal::Pipeline, Cond::Any)],
    },
    PermRow {
        text: [
            "`SetRisk`(按账户作用域)",
            "core",
            "按状态:收紧降级 / 放宽升级",
            "—",
            "—",
            "✓(仅 `cli:<login>`)",
            "—",
            "—",
            "放宽时是",
        ],
        m: Match::SetRisk,
        roles: A,
        cli_only: true,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`ReplaceRefdata`(写 ConfigBlob → Drain → 重启)",
            "core",
            "中性",
            "—",
            "—",
            "✓(仅 `cli:<login>`)",
            "—",
            "—",
            "是",
        ],
        m: Match::ReplaceRefdata,
        roles: A,
        cli_only: true,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`CancelAll`",
            "core",
            "降级",
            "—",
            "✓",
            "✓",
            "—",
            "`cli:systemd`",
            "否",
        ],
        m: Match::CancelAll,
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[(Principal::Systemd, Cond::Any)],
    },
    PermRow {
        text: [
            "`Drain`",
            "core",
            "降级",
            "—",
            "✓",
            "✓",
            "—",
            "`cli:systemd`",
            "否",
        ],
        m: Match::Drain,
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[(Principal::Systemd, Cond::Any)],
    },
    PermRow {
        text: ["`HedgeFix`", "core", "中性", "—", "✓", "✓", "—", "—", "否"],
        m: Match::HedgeFix,
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`WriteOffResidual`",
            "core",
            "升级",
            "—",
            "—",
            "✓",
            "—",
            "—",
            "是",
        ],
        m: Match::WriteOffResidual,
        roles: A,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`SmokeOrder`(冒烟单)",
            "core",
            "升级",
            "—",
            "—",
            "✓",
            "—",
            "—",
            "是",
        ],
        m: Match::SmokeOrder,
        roles: A,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`StrategyCmd`,未声明 `reduce_only`(载荷 ≤ 256 B,含义归策略)",
            "core",
            "升级",
            "—",
            "✓",
            "✓",
            "—",
            "—",
            "是",
        ],
        m: Match::StrategyCmd { reduce_only: false },
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`StrategyCmd{reduce_only}`(载荷 ≤ 256 B,含义归策略;其 Open 意图一律被拒)",
            "core",
            "中性",
            "—",
            "✓",
            "✓",
            "—",
            "—",
            "否",
        ],
        m: Match::StrategyCmd { reduce_only: true },
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`InjectWarmup`(实例级,载荷 ≤ 4096 B,含义归策略)",
            "core",
            "中性",
            "—",
            "✓",
            "✓",
            "—",
            "pipeline",
            "否",
        ],
        m: Match::InjectWarmup,
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[(Principal::Pipeline, Cond::Any)],
    },
    PermRow {
        text: [
            "`AdoptExchange`(仓位或余额条目,可带入场价)",
            "core",
            "升级",
            "—",
            "✓",
            "✓",
            "—",
            "—",
            "是",
        ],
        m: Match::AdoptExchange,
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`SetBalanceBaseline`",
            "core",
            "升级",
            "—",
            "✓",
            "✓",
            "—",
            "—",
            "是",
        ],
        m: Match::SetBalanceBaseline,
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`VenueNotice{Delisting}`",
            "core",
            "降级",
            "—",
            "✓",
            "✓",
            "—",
            "—",
            "否",
        ],
        m: Match::VenueNoticeDelisting,
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`VenueNotice{FundingInterval \\| Maintenance}`",
            "core",
            "中性",
            "—",
            "✓",
            "✓",
            "—",
            "—",
            "否",
        ],
        m: Match::VenueNoticeOther,
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`SimReset`(只对 Sim 账户)",
            "core",
            "中性",
            "—",
            "✓",
            "✓",
            "—",
            "—",
            "是",
        ],
        m: Match::SimReset,
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`SimTransfer`(只对 Sim 账户)",
            "core",
            "中性",
            "—",
            "✓",
            "✓",
            "—",
            "—",
            "否",
        ],
        m: Match::SimTransfer,
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`SimFault`(受控故障注入,只对 Sim 账户)",
            "core",
            "中性",
            "—",
            "—",
            "✓(仅 `cli:<login>`)",
            "—",
            "—",
            "否",
        ],
        m: Match::SimFault,
        roles: A,
        cli_only: true,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: ["`RecordTransfer`", "core", "中性", "—", "✓", "✓", "—", "—", "否"],
        m: Match::RecordTransfer,
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`Annotate{OpsNote \\| LocalWrite}`(无状态效果,只进 journal)",
            "core",
            "中性",
            "✓",
            "✓",
            "✓",
            "—",
            "`ops_bot`(仅 `OpsNote`)",
            "否",
        ],
        m: Match::AnnotateNote,
        roles: VTA,
        cli_only: false,
        group_chat: false,
        procs: &[(Principal::OpsBot, Cond::OpsNote)],
    },
    PermRow {
        text: [
            "`Annotate{Incident}`",
            "core",
            "中性",
            "—",
            "✓",
            "✓",
            "—",
            "—",
            "否",
        ],
        m: Match::AnnotateIncident,
        roles: TA,
        cli_only: false,
        group_chat: false,
        procs: &[],
    },
    PermRow {
        text: [
            "`ReconcileReport` 记录(非命令输入)",
            "core",
            "中性",
            "—",
            "—",
            "—",
            "—",
            "reconciler",
            "否",
        ],
        m: Match::ReconcileReport,
        roles: RoleSet::EMPTY,
        cli_only: false,
        group_chat: false,
        procs: &[(Principal::Reconciler, Cond::Any)],
    },
];

/// Rows of optional commands that are not implemented: rendered into the README table, never
/// consulted by `allowed` (their commands are `Forbidden` for every principal).
pub const OPTIONAL_ROWS: &[[&str; 9]] = &[[
    "`RotateConn` / `Resubscribe` / `SnapshotNow`",
    "optional",
    "中性",
    "—",
    "—",
    "✓",
    "—",
    "—",
    "否",
]];

/// Markdown rows of the README permission table (header excluded), in order.
pub fn render_rows() -> Vec<String> {
    PERM_ROWS
        .iter()
        .map(|r| r.text)
        .chain(OPTIONAL_ROWS.iter().copied())
        .map(|cells| format!("| {} |", cells.join(" | ")))
        .collect()
}

fn is_default_kill(cancel_resting: Option<bool>, flatten: Option<bool>, hard: Option<bool>) -> bool {
    cancel_resting != Some(false) && flatten != Some(true) && hard != Some(true)
}

fn row_matches(m: Match, cmd: &CmdBody) -> bool {
    use CmdBody as B;
    match (m, cmd) {
        (Match::GetSchema, B::GetSchema { .. }) => true,
        (
            Match::KillDefault,
            B::Kill {
                cancel_resting,
                flatten,
                hard,
                ..
            },
        ) => is_default_kill(*cancel_resting, *flatten, *hard),
        (
            Match::KillStop,
            B::Kill {
                level,
                cancel_resting,
                flatten,
                hard,
                ..
            },
        ) => *level != KillLevel::Venue && is_default_kill(*cancel_resting, *flatten, *hard),
        (Match::KillKeepResting, B::Kill { cancel_resting, .. }) => *cancel_resting == Some(false),
        (Match::KillHard, B::Kill { hard, .. }) => *hard == Some(true),
        (Match::KillFlatten, B::Kill { flatten, .. }) => *flatten == Some(true),
        (Match::SetMode(want), B::SetMode { mode, .. }) => want == *mode,
        (Match::Resume, B::Resume { .. })
        | (Match::SetParams, B::SetParams { .. })
        | (Match::SetRisk, B::SetRisk { .. })
        | (Match::ReplaceRefdata, B::ReplaceRefdata { .. })
        | (Match::CancelAll, B::CancelAll { .. })
        | (Match::Drain, B::Drain { .. })
        | (Match::HedgeFix, B::HedgeFix { .. })
        | (Match::WriteOffResidual, B::WriteOffResidual { .. })
        | (Match::SmokeOrder, B::SmokeOrder { .. })
        | (Match::InjectWarmup, B::InjectWarmup { .. })
        | (Match::AdoptExchange, B::AdoptExchange { .. })
        | (Match::SetBalanceBaseline, B::SetBalanceBaseline { .. })
        | (Match::SimReset, B::SimReset { .. })
        | (Match::SimTransfer, B::SimTransfer { .. })
        | (Match::SimFault, B::SimFault { .. })
        | (Match::RecordTransfer, B::RecordTransfer { .. })
        | (Match::ReconcileReport, B::ReconcileReport { .. }) => true,
        (Match::StrategyCmd { reduce_only: want }, B::StrategyCmd { reduce_only, .. }) => {
            want == reduce_only.unwrap_or(false)
        }
        (Match::VenueNoticeDelisting, B::VenueNotice { kind, .. }) => {
            matches!(kind, NoticeKind::Delisting { .. })
        }
        (Match::VenueNoticeOther, B::VenueNotice { kind, .. }) => {
            !matches!(kind, NoticeKind::Delisting { .. })
        }
        (Match::AnnotateNote, B::Annotate { kind, .. }) => {
            matches!(kind, AnnotateKind::OpsNote | AnnotateKind::LocalWrite)
        }
        (Match::AnnotateIncident, B::Annotate { kind, .. }) => *kind == AnnotateKind::Incident,
        _ => false,
    }
}

fn cond_holds(c: Cond, cmd: &CmdBody) -> bool {
    match (c, cmd) {
        (Cond::Any, _) => true,
        (Cond::GlobalOrAccount, CmdBody::Kill { level, .. }) => *level != KillLevel::Venue,
        (Cond::ConcreteGroup, CmdBody::SetMode { group, .. }) => {
            matches!(group.as_deref(), Some(g) if g != "*" && g != "none")
        }
        (Cond::OpsNote, CmdBody::Annotate { kind, .. }) => *kind == AnnotateKind::OpsNote,
        _ => false,
    }
}

/// Rows covering `cmd` (optional commands match none).
pub fn rows_of(cmd: &CmdBody) -> impl Iterator<Item = &'static PermRow> + '_ {
    PERM_ROWS.iter().filter(move |r| row_matches(r.m, cmd))
}

fn human_ok(r: &PermRow, role: Role, principal: Principal, chat: ChatType) -> bool {
    r.roles.contains(role)
        && match principal {
            Principal::Tg => !r.cli_only && (chat == ChatType::Private || r.group_chat),
            _ => true,
        }
}

/// Highest role of `roles` allowed to issue `cmd` (human principals); process principals and
/// forbidden combinations → `None`. This is the `CmdHdr.role` the gateway writes.
pub fn effective_role(roles: RoleSet, principal: Principal, chat: ChatType, cmd: &CmdBody) -> Option<Role> {
    if !principal.is_human() {
        return None;
    }
    Role::ALL
        .into_iter()
        .rev()
        .filter(|r| roles.contains(*r))
        .find(|&role| rows_of(cmd).any(|row| human_ok(row, role, principal, chat)))
}

/// Static permission check (README "权限表"): human principals by `roles` ∩ row roles, the
/// `cli:<login>`-only marks and the group-chat column (`tg:*` only); process principals by the
/// row's process column. `chat` is ignored for non-`tg:*` principals.
pub fn allowed(roles: RoleSet, principal: Principal, chat: ChatType, cmd: &CmdBody) -> Result<(), PermErr> {
    let ok = if principal.is_human() {
        effective_role(roles, principal, chat, cmd).is_some()
    } else {
        rows_of(cmd).any(|r| {
            r.procs
                .iter()
                .any(|(p, c)| *p == principal && cond_holds(*c, cmd))
        })
    };
    if ok { Ok(()) } else { Err(PermErr) }
}

/// Static class by command and target value (README "权限表", control-plane 类别).
pub fn class_of(cmd: &CmdBody) -> CmdClass {
    use CmdBody as B;
    match cmd {
        B::GetSchema { .. } => CmdClass::ReadOnly,
        B::Kill { flatten, .. } => {
            if *flatten == Some(true) {
                CmdClass::Upgrade
            } else {
                CmdClass::Downgrade
            }
        }
        B::SetMode { mode, .. } => {
            if *mode == RunMode::Live {
                CmdClass::Upgrade
            } else {
                CmdClass::Downgrade
            }
        }
        B::CancelAll { .. } | B::Drain { .. } => CmdClass::Downgrade,
        B::VenueNotice { kind, .. } => {
            if matches!(kind, NoticeKind::Delisting { .. }) {
                CmdClass::Downgrade
            } else {
                CmdClass::Neutral
            }
        }
        B::Resume { .. }
        | B::WriteOffResidual { .. }
        | B::SmokeOrder { .. }
        | B::AdoptExchange { .. }
        | B::SetBalanceBaseline { .. } => CmdClass::Upgrade,
        B::StrategyCmd { reduce_only, .. } => {
            if reduce_only.unwrap_or(false) {
                CmdClass::Neutral
            } else {
                CmdClass::Upgrade
            }
        }
        B::SetParams { .. }
        | B::SetRisk { .. }
        | B::ReplaceRefdata { .. }
        | B::HedgeFix { .. }
        | B::InjectWarmup { .. }
        | B::SimReset { .. }
        | B::SimTransfer { .. }
        | B::SimFault { .. }
        | B::RecordTransfer { .. }
        | B::Annotate { .. }
        | B::ReconcileReport { .. }
        | B::RotateConn { .. }
        | B::Resubscribe { .. }
        | B::SnapshotNow { .. } => CmdClass::Neutral,
    }
}

/// Static confirmation rule for the effective role (`None` = process principal).
pub fn needs_confirm_static(role: Option<Role>, principal: Principal, cmd: &CmdBody) -> ConfirmRule {
    use CmdBody as B;
    let _ = principal;
    match cmd {
        B::Kill {
            level,
            cancel_resting,
            flatten,
            hard,
            ..
        } => {
            if *flatten == Some(true) {
                ConfirmRule::Always
            } else if role == Some(Role::Viewer)
                && *level != KillLevel::Venue
                && is_default_kill(*cancel_resting, *flatten, *hard)
            {
                ConfirmRule::PreviewBound
            } else {
                ConfirmRule::Never
            }
        }
        B::SetMode { mode, .. } => {
            if *mode == RunMode::Live {
                ConfirmRule::Always
            } else {
                ConfirmRule::Never
            }
        }
        B::SetParams { .. } | B::SetRisk { .. } => ConfirmRule::IfLoosening,
        B::Resume { .. }
        | B::ReplaceRefdata { .. }
        | B::SimReset { .. }
        | B::WriteOffResidual { .. }
        | B::SmokeOrder { .. }
        | B::AdoptExchange { .. }
        | B::SetBalanceBaseline { .. } => ConfirmRule::Always,
        B::StrategyCmd { reduce_only, .. } => {
            if reduce_only.unwrap_or(false) {
                ConfirmRule::Never
            } else {
                ConfirmRule::Always
            }
        }
        _ => ConfirmRule::Never,
    }
}

/// Static check of the confirmation fields: `confirmed = true` needs a non-zero
/// `expect_state_version`, except under `PreviewBound`. Failure → `invalid_request`.
pub fn check_confirm_fields(rule: ConfirmRule, req: &CmdRequest) -> Result<(), NormErr> {
    if req.confirmed && rule != ConfirmRule::PreviewBound && req.expect_state_version.unwrap_or(0) == 0 {
        return Err(NormErr::new(0));
    }
    Ok(())
}
