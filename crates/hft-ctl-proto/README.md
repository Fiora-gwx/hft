---
description: "控制面的纯协议层:长度前缀帧、请求/响应 JSON 模式、JSON → ControlCmd 的规范化编码、角色(Admin/Trader/Viewer)与权限表;ctl-gateway、Rust/Python 客户端、hft-cfg 与 optional 的 chain-core gateway 共用"
kind: crate
tier: warm
scope: core
status: spec
---

# hft-ctl-proto

## 概要

`hft-ctl-proto` 是控制面的**纯协议** crate:线上帧格式、请求与响应的 JSON 模式、把一个已鉴权的 JSON 请求规范化为 hft-event `ControlCmd` 字节(`CtlRecord::encode_into(&mut [u8])`)、`cmd_hash` 的规范形式、以及**角色 × 命令 × 参数 × 确认的权限表**——权限表以代码(`perms.rs`)为唯一来源,本 README 的表由 `cargo xtask doc-check` 从代码生成并比对。它不做 I/O、不持状态、不知道 sequencer 的任何状态,也不解码策略参数 body(那由 gateway 调装配处的 `StrategySet::decode_params`,见 [hft-ctl](../hft-ctl/README.md));传输、socket、鉴权流程在 hft-ctl,命令语义与结果码在 [control-plane](../../docs/subsystems/control-plane.md),记录体布局在 [hft-event](../hft-event/README.md)。**新增策略不改本 crate**:参数经通用的 `SetParams` 携带,schema 由策略自带。使用方:hft-ctl 的 gateway 与 Rust 客户端、`tools/hft-cfg`、纯 Python 客户端(只依赖本 README 的模式与 `fixtures/ctl/` 金样,不链接本 crate)。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```rust
pub const PROTO_VERSION: u8 = 2;
pub const MAX_FRAME: usize = 1 << 20;
pub const MAX_BLOB: usize = 786_432;   // blob_b64 解码后的上限 = MAX_FRAME × 3/4(base64 后放得进一帧)

/// 帧:u32 大端长度 N | N 字节紧凑 UTF-8 JSON。增量解析,调用方提供缓冲。
pub fn frame_len(hdr: &[u8; 4]) -> Result<usize, FrameErr>;          // N > MAX_FRAME → TooLong
pub fn encode_frame(json: &[u8], out: &mut Vec<u8>);

/// 请求/响应的类型化视图(serde,只在 warm/cold 使用)。
pub fn parse_request(body: &[u8]) -> Result<CmdRequest, SchemaErr>;   // 模式层校验;SchemaErr → invalid_request(detail 只可能是 UnknownDiscriminant)
pub struct CmdRequest { pub v: u8, pub cmd_id: Uuid, pub issuer: Issuer, pub issued_at_ns: i64, pub expires_at_ns: i64,
                        pub account: AccountSel, pub preview: bool, pub confirmed: bool, pub expect_state_version: Option<u64>,
                        pub view_account: Option<AccountName>, pub chat_type: Option<ChatType>, pub cmd: CmdBody }
pub enum CmdBody { /* 每个 ControlCmd 变体一个 JSON 形状(type 字段区分)+ 只读请求 GetSchema */ }
pub struct CmdResponse { pub v: u8, pub cmd_id: Uuid, pub status: TransportStatus, pub code: Option<ResultCode> /* status = accepted 时 None */, pub detail: Option<Detail> /* 名字 + arg,码表归 control-plane */, pub seq: Option<u64>,
                         pub cmd_hash: Option<Hex32> /* gateway 规范化时算出的 cmd_hash;只读请求与规范化之前就被拒的请求为 None */,
                         pub ctl_state_version: Option<u64>, pub preview: Option<Box<RawValue>> /* PreviewBody */, pub engine: EngineInfo,
                         pub body: Box<serde_json::value::RawValue> /* 原样透传,不改键序 */ }

/// 规范化:JSON 命令体 → hft-event ControlCmd 字节(数值按 refdata 资产指数转定点,名字解析为稠密 id)。
pub fn normalize(req: &CmdRequest, rd: &RefData) -> Result<Normalized, NormErr>;  // NormErr → InvalidArgs{detail}
pub struct Normalized { pub kind: EventKind /* ControlCmd、ReconcileReport 或 ReconcileBatch */, pub hdr: CmdHdr, pub record: Vec<u8> /* ControlCmd 逻辑体 = CmdHdr + 载荷 */,
                        pub cmd_hash: [u8; 32], pub blobs: Vec<BlobPart> /* ≤ 64 段 ConfigBlob,体 = PartHdr + 段 */, pub blob: Option<Vec<u8>> /* 解码后的大参数 */,
                        pub reports: Vec<ReconcileReport> }
impl Normalized { pub fn params_blob(&self) -> Option<&[u8]> /* SetParams:原始 SPB1 字节,交给 gateway 做结构与 body 校验 */;
                  pub fn set_gateway_fields(&mut self, expires_tsc: u64, peer_uid: u32, role: u8);
                  pub fn ring_records(&self) -> Vec<(EventKind, RecordFlags, Vec<u8>)> /* 写环顺序:ConfigBlob 各段,再一段 ControlCmd(PartHdr + 逻辑体);或各条 ReconcileReport/单条 ReconcileBatch */ }

/// 权限:纯函数,gateway 调用;不需要任何 sequencer 状态。
pub fn class_of(cmd: &CmdBody) -> CmdClass;                  // Downgrade | Upgrade | Neutral | ReadOnly,按命令与目标值静态判定;
                                                              // 类别"按状态"的 SetParams、SetRisk 静态返回 Neutral(放宽与否由 sequencer 判定)
pub fn allowed(role: RoleSet, principal: Principal, chat: ChatType, cmd: &CmdBody) -> Result<(), PermErr>; // → Forbidden
pub fn effective_role(roles: RoleSet, principal: Principal, chat: ChatType, cmd: &CmdBody) -> Option<Role>; // 人工主体允许该命令的最高角色(= CmdHdr.role)
pub fn needs_confirm_static(role: Option<Role> /* 进程主体 None */, principal: Principal, cmd: &CmdBody) -> ConfirmRule;
pub enum ConfirmRule { Never, Always, IfLoosening /* 由 sequencer 按状态判定(SetRisk、人工与 pipeline 的 SetParams) */, PreviewBound /* Viewer 的 Kill:绑定预览 cmd_hash + 60 s TTL */ }
/// 确认字段的静态检查(gateway 在得到有效角色后调用):confirmed = true 时,PreviewBound 以外的命令必须带非 0 的 expect_state_version。
pub fn check_confirm_fields(rule: ConfirmRule, req: &CmdRequest) -> Result<(), NormErr>;   // → invalid_request(NormErr.detail = 0)
pub const VIEWER_CONFIRM_TTL_MS: u32 = 60_000;   // 语义归 control-plane;sequencer 执行,客户端用于提示
```

### 请求与响应

```json
{"v":2,"cmd_id":"5b0a…","issuer":"cli:alice","issued_at_ns":1790000000000000000,"expires_at_ns":1790000030000000000,
 "account":"pc-live","preview":false,"confirmed":true,"expect_state_version":4182,
 "view_account":"pc-live","chat_type":"private",
 "cmd":{"type":"SetMode","strategy":"pc-main","group":"*","mode":"Live"}}
```

```json
{"v":2,"cmd_id":"5b0a…","status":"ok","code":"Applied","seq":123456789,"cmd_hash":"9f3c…","ctl_state_version":4183,
 "engine":{"version":"0.4.0","epoch":17,"refdata":"sha256:…","params":{"pc-main/pc-live":42},"risk":7},"body":{}}
```

- `strategy` 写**实例名**(refdata `Instances` 的 `name`),`account` 写账户名(refdata `AccountMeta.label`);`normalize` 解析为 `StrategyId`/`AccountId`,未知 → `InvalidArgs{UnknownName}`。`"*"` 的编码:`account`、`strategy`、`venue`、`inst` → 0;`group` → `GROUP_ALL`(`u32::MAX`);`"none"` 只在 `StrategyCmd.group` 与 `AdoptExchange.group`(缺省即 `"none"` = unassigned)合法,编码为 `GROUP_NONE`(0)。
- `status`(传输层):`ok`、`accepted`(已写环、等待超时,用 `result` 补查)、`invalid_request`、`unauthorized`、`busy`、`internal_error`。`code` 与 `detail`(如 `{"name":"ParamInvalid","arg":"ModelMemBudget"}`):码表在 [control-plane 结果码](../../docs/subsystems/control-plane.md#结果码)。`detail` 的 `arg`:码表给出名字的取值写名字(`ParamInvalid` 的 kind 写 `"Strategy"`、`"ModelMemBudget"`、`"NonProdOnly"`,字段下标不是 `0xFFF` 时写成 `"Strategy:<下标>"`;`RefdataAssembly` 的失败项名;`GroupCheck`、`RefdataRecheck` 的 `LoadErrCode` 变体名;`SmokeRiskReject` 的 `RejectBits` 位名),其余写十进制字符串,arg 为 0 且无名字时省略;不认识的 detail 码以十进制字符串作 `name`。sequencer 侧错误到结果码与 detail 的映射以 [hft-sequencer 命令路径](../hft-sequencer/README.md#命令路径) 为准,本 crate 只按码渲染名字:hft-risk `ModeErr::UnknownPending` → `PreconditionFailed{UnknownPresent}`(306);hft-exec `ExecCmdErr`(`HedgeFix`)的 `UnknownGroup` → `InvalidArgs{UnknownGroup}`(115),`NotQuiescent`、`Hard`、`NothingToFix`、`QuoteUnavailable`、`ChainLeg` → `PreconditionFailed{同名 detail}`(316–320)。`seq` = 该命令 `ControlCmd` 记录的 seq(`CmdResult.cmd_seq`)。两者的对应:`ok` → `code` 是结果码(sequencer 的 `CmdResult.status`,或 gateway 产生的码,如 `Forbidden`、`RateLimited`、`Expired`、规范化与 gateway 校验的 `InvalidArgs{detail}`);`accepted` → `code = None`;`invalid_request` → `InvalidArgs`;`unauthorized` → `Unauthorized`;`busy` → `Busy`;`internal_error` → `Internal`。
- **字段约定**(下文逐条给出的形状之外的命令同此):字段名 = [control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)表的参数名;作用域写成顶层的 `account`、`strategy`、`group`、`venue`、`inst`(`strategy`、`group`、`venue`、`inst` 省略即 `"*"`;`account` 的省略按账户语义;命令体里也写了 `account` 的(如下文 `SetParams`、`SmokeOrder` 的形状)与信封的 `account` 都给出时必须相同,否则 `InvalidArgs{AccountMismatch}`,只给出一处时以那一处为准);`AdoptExchange` 的 `inst`(`item = Position`)与 `asset`(`item = Balance`)按 item 必填;枚举写变体名字符串(如 `"mode":"Live"`、`"level":"Account"`、`"source":"Kill"`、`"side":"Buy"`、`"kind":"OpsNote"`);数值(含 id、版本、时刻、`ttl_ms`)一律十进制字符串,整数越出其类型范围、十进制超出该资产指数的精度或溢出定点范围 → `InvalidArgs{BadValue}`;布尔写 JSON 布尔;字节载荷写 base64 的 `payload_b64`,`SetParams`、`SetRisk`、`ReplaceRefdata` 的大对象写 `blob_b64`(解码后 ≤ `MAX_BLOB`,超出 → `InvalidArgs{TooLong}`);base64 解码失败 → `InvalidArgs{BadValue}`;省略的可选参数取 control-plane 参数列写明的缺省值。
- **模式层错误 → `invalid_request`**(`code = InvalidArgs`;不进入权限判定之后的步骤):帧内不是 UTF-8 JSON、`v ≠ 2`、`issuer` 超过 24 字节、缺必填字段、未知字段、类型不符(含 JSON 浮点数与非十进制的数值串)、未知 `type` 或未知枚举名(这两种 `detail = UnknownDiscriminant`,其余省略 `detail`)、`preview` 与 `confirmed` 同为 true、`expires_at_ns ≤ issued_at_ns` 或 `expires_at_ns − issued_at_ns` > 300 s、`check_confirm_fields` 失败。
- `account`:名字或 `*`;规则(降级缺省 = 全部,升级/中性必须显式,逐命令的取值见第 9 条)在 [control-plane 账户语义](../../docs/subsystems/control-plane.md#账户语义),`normalize` 执行;`view_account`/`chat_type` 只进审计 Note,永不补全账户(`view_account` 解析不出时审计记 0,不拒绝)。`chat_type ∈ {"private", "group"}`:`issuer` 为 `tg:*` 时必填(缺 → `invalid_request`),权限表的"群聊"列只对 `tg:*` 生效;其他 issuer 给出也不参与权限判定。
- `cmd_hash` = SHA-256(`CmdHdr.account`(1 B)‖ `cmd`(u16 小端)‖ `cmd_ver`(u16 小端)‖ 命令载荷(hft-event 编码,含完整 `CmdScope`;`SetParams`/`SetRisk`/`ReplaceRefdata` 的载荷含 blob 的 `sha256`));信封的其余字段(`cmd_id`、`issuer`、时刻、`preview`、`confirmed`、`expect_state_version`、`base_version`)不进 `cmd_hash`。gateway 在每个写请求的响应里回带它(十六进制),客户端用于显示与核对预览,确认请求不携带它。
- `SetParams` 的 JSON 形状:`{"type":"SetParams","strategy":"pc-main","account":"pc-live","blob_b64":"…","base_version":"41"}`;`normalize` 只做 base64 解码、长度检查(≤ `MAX_BLOB`;短于 `StrategyParamsHdr` → `InvalidArgs{BadValue}`)、`StrategyParamsHdr` 的 `strategy/account` 与 JSON 一致(否则 `InvalidArgs{ScopeMismatch}`)、分段与 sha256;容器结构、账户绑定与 body 解码在 gateway(见 [hft-ctl](../hft-ctl/README.md))。
- `GetSchema` 的 JSON 形状:`{"type":"GetSchema","strategy_kind":"<种类名>"}`(或 `"kind_id":"1"`;两者恰好给一个,否则 `invalid_request`;未知种类 → `InvalidArgs{UnknownName}`);只读,不规范化为 `ControlCmd`,由 gateway 直接应答,`body` = SDK `params` 产生的 schema JSON。
- `group` 一律写十进制 `GroupId`(`(StrategyId << 24) | n`,构造归 [hft-types](../hft-types/README.md#id-与枚举);如实例 1 的第 101 个 group = `"16777317"`)。
- `StrategyCmd` 的 JSON 形状:`{"type":"StrategyCmd","strategy":"pc-main","group":"16777317" | "none","reduce_only":false,"payload_b64":"…"}`,`account` 必须显式;解码后 ≤ 256 B(否则 `InvalidArgs{TooLong}`)。载荷由策略定义,本 crate 不解析。
- `SmokeOrder` 的 JSON 形状:`{"type":"SmokeOrder","account":"<label>","venue":"binance-usdm","inst":"binance-usdm:BTCUSDT","side":"Buy","ttl_ms":"10000"}`;`ttl_ms` 缺省时 `normalize` 填 control-plane 规定的缺省值,越出范围 → `InvalidArgs{BadValue}`。`WriteOffResidual`:`{"type":"WriteOffResidual","account":"<label>","group":"16777317"}`。两者都是升级类,`account` 必须显式。`SimFault`:`{"type":"SimFault","account":"<label>","venue":"binance-usdm","fault":"RejectNew","delay_ms":"0"}`;`fault` 写变体名,`delay_ms` 只在 `fault = DelayReports` 时为 1..=60 000,其余必须为 0,否则 `InvalidArgs{BadValue}`;`account` 必须显式。
- `ReconcileReport` 的 JSON 形状:`{"type":"ReconcileReport","report_id":"…","items":[{…}, …]}`,每个条目的字段名与 [hft-event `ReconcileReport`](../hft-event/README.md#控制与系统0x04xx0x05xx) 相同(`venue`、`account`、`inst`、`asset` 写名字,`item`、`severity`、`order_status` 写码名,数量与金额写十进制字符串,`sflags` 写位名数组);`idx` 与 `count` 由 `normalize` 按条目顺序填写(条目 ≥ 1,否则 `invalid_request`);不是 `ControlCmd`,没有 `cmd_hash`。
- `BindSlot` 的 JSON 形状:`{"type":"BindSlot","venue":"polymarket-clob","inst":"polymarket-clob:btc-up-0","start_s":"1792425300","end_s":"1792425600","wire_id":"9326…8930","account":"*"}`;`wire_id` 是交易所 id 的十进制 uint256(最多 78 位,`"0"` = 解绑),规范化为 32 字节大端;`venue` 与 `inst` 按通用名字规则(协议名或十进制 id;`<venue>:<符号>` 或十进制 id);`account` 必须给出且只能是 `"*"`(venue 级中性命令,编码为 0;缺省 → `InvalidArgs{AccountRequired}`)。执行语义归 [hft-sequencer](../hft-sequencer/README.md)。
- `InjectWarmup` 的 JSON 形状:`{"type":"InjectWarmup","strategy":"pc-main","local_kind":"3","chunk_no":"0","last":false,"payload_b64":"…"}`;`account` 必须缺省或为 `"*"`(实例级命令,编码为 0;给出具体账户 → `InvalidArgs{AccountNotAllowed}`);解码后 ≤ 4096 B。一个请求只携带一块:块由生产者(策略的 pipeline 插件)产出,每块自身完整;客户端按块列表逐块发送并递增 `chunk_no`,不切字节(见 [hft-ctl 客户端](../hft-ctl/README.md#客户端))。

### 二次确认

协议的唯一定义在 [control-plane 版本与确认](../../docs/subsystems/control-plane.md#版本与确认):先发带 `preview = true` 的请求,再用新 `cmd_id`、同一命令体、`confirmed = true` 重发;是否需要确认、版本是否已变、预览是否仍有效都由 sequencer 判定。本 crate 保证:

- 模式里有 `preview`、`confirmed`、`expect_state_version` 三个字段,没有任何令牌字段;
- `confirmed = true` 时 `expect_state_version` 必填且不为 0(`CmdHdr` 中 0 表示不检查;`check_confirm_fields`,gateway 在鉴权得到有效角色后调用,缺或为 0 → `invalid_request`);**唯一例外**是 `needs_confirm_static` 返回 `PreviewBound` 的命令——Viewer 的 `Kill`:它的确认绑定同一 issuer 的预览 `cmd_hash` 与 `VIEWER_CONFIRM_TTL_MS`(60 s),`expect_state_version` 缺省(编码为 0),填了也被 sequencer 忽略;
- 同一命令体的预览与确认产生相同的 `cmd_hash`(`preview`/`confirmed`/`expect_state_version` 不进 `cmd_hash`)。

### 策略参数 schema JSON

格式、规范化与 `schema_digest` 的唯一定义在 [hft-strategy-sdk 的 `params` 模块](../hft-strategy-sdk/README.md)(含 `FieldDesc.since` 等字段);本 crate 只把它当作 `GetSchema` 应答的 `body` 原样透传,不渲染、不解析。发布制品里的 schema 文件由 `hft-core schemas --out <dir>` 导出(见 [tools hft-cfg](../../tools/README.md#hft-cfg))。

### 权限表

**唯一归属**:代码 `src/perms.rs`;本表由 doc-check 生成比对;[control-plane](../../docs/subsystems/control-plane.md)、[hft-ctl](../hft-ctl/README.md)、[ops_bot](../../python/ops_bot/README.md) 只链接这里(群聊放行列表即"群聊"列)。角色 `Admin`、`Trader`、`Viewer`(**老板 = Viewer**:唯一改变状态的写权限是"紧急停止"——非 HARD、不带 flatten、默认 `cancel_resting` 的 `Kill{Global | Account}`,须二次确认;另可发无状态效果的 `Annotate{OpsNote | LocalWrite}`;不能 `Resume`,不能发任何其他写命令)的定义见 [control-plane 角色](../../docs/subsystems/control-plane.md#角色)。"scope"列:`core` = 首次上线必须具备;`optional` = 已分配编码与语义,未实现前 `perms.rs` 的 `PERM_ROWS` 不含该行(只在不参与 `allowed` 判定、仅供本表生成的 `OPTIONAL_ROWS` 中),gateway 返回 `Forbidden`(触发条件见 [control-plane 命令集](../../docs/subsystems/control-plane.md#命令集))。类别按命令与**目标值**静态判定(`SetMode → Live` 升级;`→ CloseOnly`/`→ Shadow` 降级);"按状态"的类别由 sequencer 判定([control-plane 类别](../../docs/subsystems/control-plane.md#类别))。"进程主体"列的主体见下节。"确认"列:`是` = 一律要求二次确认;`放宽时是` = sequencer 判定为升级(放宽)时要求确认,收紧或中性时不要求;`否` = sequencer 不要求(客户端仍可先预览)。

| 命令 / 参数组合 | scope | 类别 | Viewer | Trader | Admin | 群聊 | 进程主体 | 确认 |
|---|---|---|---|---|---|---|---|---|
| `GetSchema`(只读,gateway 本地应答) | core | 只读 | ✓ | ✓ | ✓ | ✓ | pipeline | 否 |
| `Kill`(Account/Venue/Global,默认参数) | core | 降级 | — | ✓ | ✓ | ✓ | `ops_bot`(Global/Account,补发兜底期间的 `/stop`);`cli:systemd`(Global/Account,仅启动时由停机标记触发) | 否 |
| `Kill{Global}` 或 `Kill{Account}`,默认参数(非 HARD、无 flatten、`cancel_resting = true`):Viewer 紧急停止 | core | 降级 | ✓ | ✓ | ✓ | ✓ | — | Viewer:是(预览 + 同一 issuer 60 s 内确认,绑定 `cmd_hash`,不比对 `ctl_state_version`);Trader/Admin:否 |
| `Kill` 改 `cancel_resting = false` | core | 降级 | — | ✓ | ✓ | — | — | 否 |
| `Kill{Global}` 或 `Kill{Account}` 带 `hard`(与 Venue 组合 → `InvalidArgs`) | core | 降级 | — | ✓ | ✓ | — | — | 否 |
| `Kill{Account}` 带 `flatten`(不得与 Global、`hard` 组合) | core | 升级(启动闸门未放行、DRAINING 期间同样可发) | — | ✓ | ✓ | — | — | 是 |
| `SetMode → CloseOnly` | core | 降级 | — | ✓ | ✓ | ✓ | pipeline(group 级) | 否 |
| `SetMode → Shadow` | core | 降级 | — | ✓ | ✓ | — | pipeline(group 级) | 否 |
| `SetMode → Live` | core | 升级 | — | ✓ | ✓ | — | — | 是 |
| `Resume{source, scope}` | core | 升级 | — | ✓ | ✓ | — | — | 是 |
| `SetParams` | core | 按状态:中性;人工主体 `open_allowed` 0→1、或 Live 实例(定义见 [control-plane 类别](../../docs/subsystems/control-plane.md#类别))上改动结构字段时升级 | — | ✓(仅 `cli:<login>`) | ✓(仅 `cli:<login>`) | — | pipeline | 放宽时是;pipeline 主体从不确认(其 blob 若被判为升级 → `NeedsConfirm`) |
| `SetRisk`(按账户作用域) | core | 按状态:收紧降级 / 放宽升级 | — | — | ✓(仅 `cli:<login>`) | — | — | 放宽时是 |
| `ReplaceRefdata`(写 ConfigBlob → Drain → 重启) | core | 中性 | — | — | ✓(仅 `cli:<login>`) | — | — | 是 |
| `CancelAll` | core | 降级 | — | ✓ | ✓ | — | `cli:systemd` | 否 |
| `Drain` | core | 降级 | — | ✓ | ✓ | — | `cli:systemd` | 否 |
| `HedgeFix` | core | 中性 | — | ✓ | ✓ | — | — | 否 |
| `WriteOffResidual` | core | 升级 | — | — | ✓ | — | — | 是 |
| `SmokeOrder`(冒烟单) | core | 升级 | — | — | ✓ | — | — | 是 |
| `StrategyCmd`,未声明 `reduce_only`(载荷 ≤ 256 B,含义归策略) | core | 升级 | — | ✓ | ✓ | — | — | 是 |
| `StrategyCmd{reduce_only}`(载荷 ≤ 256 B,含义归策略;其 Open 意图一律被拒) | core | 中性 | — | ✓ | ✓ | — | — | 否 |
| `InjectWarmup`(实例级,载荷 ≤ 4096 B,含义归策略) | core | 中性 | — | ✓ | ✓ | — | pipeline | 否 |
| `AdoptExchange`(仓位或余额条目,可带入场价) | core | 升级 | — | ✓ | ✓ | — | — | 是 |
| `SetBalanceBaseline` | core | 升级 | — | ✓ | ✓ | — | — | 是 |
| `VenueNotice{Delisting}` | core | 降级 | — | ✓ | ✓ | — | — | 否 |
| `VenueNotice{FundingInterval \| Maintenance}` | core | 中性 | — | ✓ | ✓ | — | — | 否 |
| `BindSlot`(运行期绑定 venue 的槽位 inst ↔ 交易所 id) | core | 中性 | — | ✓ | ✓ | — | pipeline | 否 |
| `SimReset`(只对 Sim 账户) | core | 中性 | — | ✓ | ✓ | — | — | 是 |
| `SimTransfer`(只对 Sim 账户) | core | 中性 | — | ✓ | ✓ | — | — | 否 |
| `SimFault`(受控故障注入,只对 Sim 账户) | core | 中性 | — | — | ✓(仅 `cli:<login>`) | — | — | 否 |
| `RecordTransfer` | core | 中性 | — | ✓ | ✓ | — | — | 否 |
| `Annotate{OpsNote \| LocalWrite}`(无状态效果,只进 journal) | core | 中性 | ✓ | ✓ | ✓ | — | `ops_bot`(仅 `OpsNote`) | 否 |
| `Annotate{Incident}` | core | 中性 | — | ✓ | ✓ | — | — | 否 |
| `ReconcileReport` 记录(非命令输入) | core | 中性 | — | — | — | — | reconciler | 否 |
| `RotateConn` / `Resubscribe` / `SnapshotNow` | optional | 中性 | — | — | ✓ | — | — | 否 |

Viewer 的 `Kill` 只匹配上表"Viewer 紧急停止"一行:带 `hard`、`flatten`、`cancel_resting = false` 或 `level = Venue` 的 `Kill` 对 Viewer 一律 `Forbidden`(gateway 静态判定);缺确认由 sequencer 返回 `NeedsConfirm`,预览失效(超过 60 s、换了 issuer、命令体不同或已被使用)返回 `ConfirmInvalid{NoValidPreview}`(`needs_confirm_static` 对 Viewer 的 `Kill` 返回 `PreviewBound`,sequencer 按 `CmdHdr.role = Viewer` 判定)。`SmokeOrder`、`WriteOffResidual` 经 Telegram 时只在私聊中、由 Admin 角色的 `tg:<from.id>` 发出。

参数通道:`StrategyParams` 由 pipeline 主体按策略插件下发(不需确认;上界由 `RiskParams` 约束,pipeline 不升级 `RunMode`,插件不拥有结构字段);人工改策略参数经 ops_bot 的参数编辑命令(命令名归 [ops_bot](../../python/ops_bot/README.md);改 pipeline 的策略文件,pipeline 以自己的身份下发),或 `hft-cfg` 生成 blob 由 `cli:<login>`(Trader/Admin)发送并二次确认——Live 实例的结构字段只经后一条路;`RiskParams` 与 refdata 只经 `hft-cfg` + `cli:<login>`(Admin)。**Telegram(`tg:*`)不直接发 `SetParams`/`SetRisk`/`ReplaceRefdata`**。模拟账户上的命令与实盘同一权限(账户类型只影响语义,不影响角色),预览第一行标明账户类型。

optional 的 chain-core 会话(链上策略接入时启用)沿用同一帧与模式,命令行见 [hft-ctl chain-core 会话](../hft-ctl/README.md#chain-core-的-ctl-会话)。

### 主体与角色码

**唯一归属**(uid 与组以 [deploy 单元表](../../deploy/README.md#systemd-单元)为准;hft-ctl 只执行):

| 主体 | uid | issuer | 角色上限 |
|---|---|---|---|
| ops_bot(代表 Telegram 用户) | `hft-opsbot` | `tg:<from.id>` | Viewer + Trader + Admin(上表中"仅 `cli:<login>`"的命令除外);每个 Telegram 用户的角色在 gateway 的 issuer 表里配置,老板的 `from.id` 只配 Viewer(只读 + 紧急停止) |
| ops_bot 自身(不代表任何 Telegram 用户) | `hft-opsbot` | `ops_bot` | 只允许默认参数的 `Kill{Global \| Account}`(非 HARD、不带 flatten;恢复后自动补发兜底期间未进 journal 的 `/stop`,范围与原 `/stop` 相同,[ops_bot 紧急停止](../../python/ops_bot/README.md#紧急停止stop))与 `Annotate{OpsNote}` |
| 运维 CLI | 运维登录用户(须能访问 ctl socket:`hft-ctl` 组成员资格的登记方式归 deploy) | `cli:<login>` | 个人配置的角色 |
| reconciler | `hft-recon` | `reconciler` | 只允许写 `ReconcileReport` 记录(不发任何 `ControlCmd`) |
| pipeline | `hft-pipeline` | `pipeline` | 只允许 `SetParams`、group 级 `SetMode → Shadow/CloseOnly`(`group` 为具体 GroupId;`"*"` → `Forbidden`)、`InjectWarmup`、`BindSlot`(市场发现)、`GetSchema` |
| hft-core 自身(进程内,不经 socket) | — | `cli:systemd` | 只允许 `CancelAll`、`Drain`(SIGTERM 路径,经 gateway 的内部通道),以及**仅在启动时由 kill-switch 停机标记触发**的默认参数 `Kill{Global \| Account}`(非 HARD、不带 flatten、范围 = 标记的 `scope`;由启动步 5 写成 Input,流程归 [apps/hft-core](../../apps/hft-core/README.md)) |
| watchtower、treasury | — | — | **不是主体**:只读 journal、只告警或给建议,不发命令(行情/时钟/磁盘/保证金类降级由 hft-risk 的条件型 cap 负责) |

pipeline、reconciler、`ops_bot` 自身与 `cli:systemd` 是**进程主体**,权限以本表逐命令列出,不是角色。`CmdHdr.role` 码:Viewer = 1、Trader = 2、Admin = 4;3 退役、永不复用;进程主体写 0 并以 issuer 区分;角色集合按 `1 << (code − 1)` 成位。人工主体的 `CmdHdr.role` 写有效权限中允许该命令的最高角色(Admin > Trader > Viewer),`needs_confirm_static` 以它为 `role`。

`ReconcileReport` 请求可带 `receipts_b64`,内容为按顺序连接的 ExternalFill v3 固定体。提供时只能提交一个完整账户/venue 分组,规范化为单条 ReconcileBatch;不提供时保持普通报告逐条编码。空、非整行、超过事件协议上限、身份错配、未知品种或未启用 PERP_CASH 的凭据返回 InvalidArgs;成交语义不成立的凭据同样返回 InvalidArgs{BadValue}:数量或价格 ≤ 0、现金覆盖量不等于数量、填充字节非零、account/venue 与分组不同、成因不是 Adl/Liquidation/Unknown、身份形状不对(有 trade_id 时 fill_seq 必须为 0;无 trade_id 时 exch_order_id 与 fill_seq 都必须非 0)。权限仍仅 reconciler,无 cmd_hash,不经过命令去重缓存。字段按机器原始凭据传输,金额和费用不由 gateway 根据余额差生成。

## 实现说明

<details><summary>实现要点</summary>

- `normalize` 的步骤:① 版本与字段校验;② 账户语义(缺省、显式、`*`);③ 名字 → 稠密 id(strategy 实例、group、venue、account、inst、asset),未知 → `InvalidArgs{UnknownName}`;同时给出具体 strategy 与具体账户的命令按 control-plane 账户语义第 6 条检查绑定(`InvalidArgs{AccountBound}`);④ 十进制 → 定点(按资产指数,溢出 → `InvalidArgs`);⑤ 大参数(SPB1、RiskParams、refdata)分段为 `BlobPart` 并计算 sha256,`ControlCmd` 只携带 `BlobRef`;⑥ 生成 `CmdHdr`(preview/confirmed/expect_state_version 原样;`expires_tsc`、`peer_uid`、`role` 留 0,由 gateway 填写)与 `cmd_hash`。按 ①→⑥ 的顺序取第一个失败。`GetSchema` 不经 `normalize`。
- `class_of` 与 `allowed` 不读任何运行状态;需要状态的判定(放宽、Live 实例上的结构字段改动、确认、版本、闸门、Draining、账户类型是否 Sim)全部在 sequencer。
- 权限表的生成:`perms.rs` 中每行是一个 `const PermRow`;`xtask doc-check` 渲染为 Markdown 并与本 README 的表逐行比对。

| 文件 | 职责 |
|---|---|
| `src/frame.rs` | 帧编解码 |
| `src/schema.rs` | `CmdRequest`/`CmdBody` 的严格解析与规范 JSON 写出 |
| `src/response.rs` | `CmdResponse`、`EngineInfo`、预览体渲染 |
| `src/codes.rs` | 结果码、detail 码的名字与 arg 渲染 |
| `src/names.rs` | 名字 ↔ 稠密 id(venue 名 = hft-core `venues.<v>.protocol` 名) |
| `src/normalize.rs` | JSON → `CtlRecord` 与 `cmd_hash` |
| `src/perms.rs` | 权限表(`PERM_ROWS` + `OPTIONAL_ROWS`)、主体表、角色码(唯一来源) |
| `fixtures/ctl/*.json` | 金样帧(Rust 与 Python 客户端共用);请求 JSON 为规范形(`CmdRequest::to_json`:键序固定、`preview`/`confirmed` 只在为 true 时写出、缺省的可选键省略) |

</details>

## 延迟与资源契约

无热路径约束(warm:不做 I/O、不建线程、不读时钟,只在 warm 的 gateway 与冷客户端里调用;因依赖 `serde_json` 不能标为 pure)。`normalize` 对一帧上限大小的请求(`blob_b64` 解码后为 `MAX_BLOB`)p99 ≤ 1 ms(`ctlproto/normalize_1m`)。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:`hft-types`、`hft-refdata`、`hft-event`(`ControlCmd`、`CmdHdr`、`CtlRecord`、`BlobRef`、SPB1 头)。不依赖 hft-strategy-sdk 与任何 `strategies/*`(schema JSON 只透传)。
- 外部:`serde`/`serde_json`、`uuid`、`sha2`、`base64`、`zerocopy`(hft-event 记录体的字节视图)。
- 禁止:I/O、线程、`tokio`、任何运行时状态、L1 引擎 crate(参数与风控参数的解码在 hft-ctl 的 gateway 里调用)。

## 延伸阅读

- [control-plane](../../docs/subsystems/control-plane.md)、[hft-ctl](../hft-ctl/README.md)、[cookbook/adding-a-control-command](../../docs/cookbook/adding-a-control-command.md)、[hft-strategy-sdk](../hft-strategy-sdk/README.md)(`ParamSchemaDesc`)、[tools hft-cfg](../../tools/README.md#hft-cfg)
- 决策出处:[round8-decisions](../../design/_work/round8-decisions.md)(参数 schema 驱动、老板只读、老板紧急停止、紧急停止的确认方式);决策记录 `viewer-emergency-stop`、`warmup-injection`([Agent Notes](../../.agents/notes/README.md))
- 现协议原文:`E:\trading\Trading\schemas\control-v1.md`([research/00 §1.5](../../design/research/00-existing-codebase-inventory.md))

## 已知限制与后续工作

- issuer 真实性依赖客户端进程自证。可选扩展:端到端签名(`sig` 字段;触发条件:需要防御 ops_bot 主机被攻破后伪造 issuer,或运维人员增加)。
- 角色只有三个;风控放宽与交易权目前都在 Trader/Admin 内按命令区分。可选扩展:更细的角色(触发条件:运维人员增加、需要把风控放宽权与交易权分给不同人)。

### 开发备忘

<details><summary>给实现者</summary>

- 记录体字段的布局归 hft-event(`CmdHdr.hflags.PREVIEW`、`expect_state_version`、`CmdResult.ctl_state_version`/`rflags`/`detail`/`preview`、`CtlRecord::encode_into`、SPB1 头);`PreviewBody` 由 `CmdPreview` 渲染(键名 = 字段名;账户与 venue 写名字,inst 写 `<venue>:<symbol>`,数量写十进制字符串;`wo_venue`、`wo_inst`、`wo_leg_ix`、`wo_qty` 只在 `wo_venue ≠ 0` 时出现),`rflags.PREVIEW_TTL` 时渲染为 `"confirm_within_s": 60`;`Detail` 按 control-plane detail 码表把 `CmdResult.detail` 渲染为名字与 arg。
- 实现顺序:frame → schema → normalize + 金样 → perms + 生成器 → fuzz。
- 实例名、账户标签的解析表来自 refdata;`normalize` 不接受数字 id 字符串以外的别名(避免同名歧义)。

</details>
