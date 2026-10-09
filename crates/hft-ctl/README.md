---
description: "冷进程(ops_bot、pipeline、reconciler、运维 CLI)向 hft-core 发命令的唯一通道:warm 任务 ctl-gateway(Unix socket、鉴权、角色上限、SetParams 的通用解析与按 schema 解码、GetSchema 应答、写 ctl 环、回传 CmdResult)与 Rust/纯 Python 客户端;帧、模式与权限表在 hft-ctl-proto"
kind: crate
tier: warm
scope: core
status: spec
---

# hft-ctl

## 概要

`hft-ctl` 拥有**控制面的传输与鉴权**:ctl-gateway 的实现(监听本机 Unix socket、按对端 uid 与签发者鉴权、执行角色上限、调用 [hft-ctl-proto](../hft-ctl-proto/README.md) 规范化命令、对 `SetParams` 做通用的容器解析与账户绑定检查并调装配处的 `StrategySet::decode_params` 解码 body、本地应答只读的 `GetSchema`、写唯一的 ctl 环、从 seq journal 尾部取回 `CmdResult` 交还客户端)以及 Rust 与纯 Python 两个客户端。帧格式、JSON 模式与**权限表**在 hft-ctl-proto。gateway **不复制 sequencer 状态**:不保存确认令牌、不判定放宽、不读 `ctl_state_version`——这些都由 sequencer 判定。gateway 对策略是**泛型**的(`Gateway<S: StrategySet>`,`S` 是 [strategies/registry](../../strategies/registry/README.md)(crate `hft-strategy-registry`)里唯一一处 `strategy_set!` 生成的类型,由 apps/hft-core 传入),所以**新增策略不改本 crate**。它不拥有命令的语义与执行(命令集、参数、结果码、幂等与版本规则在 [control-plane](../../docs/subsystems/control-plane.md),执行在 [hft-sequencer](../hft-sequencer/README.md));命令与结果的记录体定义在 [hft-event](../hft-event/README.md)。gateway 线程运行在 hft-core 的 warm 线程上,不在 tick-to-trade 路径;客户端在冷进程里。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### gateway(warm 任务,`poll_once` 形态)

```rust
pub struct GatewayConfig {
    pub socket_path: PathBuf,             // = hft-core 配置 paths.ctl_socket,位于本实例运行目录 /run/hft/core-<instance>/(属组 hft-ctl,0660;布局归 deploy);禁 TCP
    pub principals: PrincipalTable,       // uid → 主体与角色上限;issuer → 角色集合(主体表与角色码唯一归属 hft-ctl-proto);装配处以 PrincipalTable::load(hft-core paths.ctl_principals) 在启动时加载一次,见鉴权
    pub result_wait_ms: u32,              // 等 CmdResult 的上限,默认 5000
    pub write_rate_per_min: u16,          // 每 issuer 写命令限速,默认 12(降级类、只读与 InjectWarmup 不计入)
    pub warmup_rate_per_s: u16,           // 每 issuer 的 InjectWarmup 块限速,默认 50(≈ 200 KiB/s);超出 → RateLimited
    pub dedup_capacity: u32,              // 最近 cmd_id 缓存,默认 4096
    pub advisor_mem_budget_mib: u32,      // 装配时从 EngineConfig ECF1 段 5 `advisor_mem_budget_mib` 读入(与 sequencer 同一个值;字段归 hft-sequencer,取值归 apps/hft-core),SetParams 校验 ⑧ 用
    pub advisor_ckpt_max_mib: u32,        // 同上,读 ECF1 段 5 `advisor_ckpt_max_mib`;SetParams 校验 ⑧ 用
    pub max_groups_per_instance: u16,     // 装配时读 ECF1 Capacities `max_groups_per_instance`:交给 S::decode_params 的 GroupTable 容量(超出 → SDK TooManyGroups)
}
impl GatewayConfig { pub fn new(socket_path, principals, advisor_mem_budget_mib, advisor_ckpt_max_mib, max_groups_per_instance) -> GatewayConfig; } // 其余字段取默认值;没有默认值的三项是参数
pub struct Gateway<S: StrategySet, R: CtlRing = ByteProducer, J: ResultSource = JournalReader> { /* listener、连接表、Core(挂起表、去重缓存、限速、比较基准)、结果源 */ }
impl<S: StrategySet, R: CtlRing, J: ResultSource> Gateway<S, R, J> {
    pub fn new(cfg: GatewayConfig, rd: RefData,
               ctl: R, ring_bytes: usize,                  // ctl 环:hft-rt ByteSpsc,容量 ring_bytes ≥ 256 KiB(否则 GatewayError),单条 ≤ 容量/4
               parsed: MailboxSender<ParsedConfig<S>>,     // 解析好的大对象(StrategyParams 解码结果、RiskParams;类型归 hft-sequencer)
               retired: MailboxReceiver<ParsedConfig<S>>,  // sequencer 交还的旧对象,在本任务释放
               internal: Consumer<InternalCmd>,            // housekeeping 的 SIGTERM 路径(issuer = cli:systemd)
               refdata_check: RefdataAssemblyCheck,        // 装配处提供:hft-core 启动步 2 的同一组装配校验函数,ReplaceRefdata 用
               risk_decode: RiskDecode,                    // 装配处提供:hft-risk 的 RiskParams 解码与作用域检查(Err = BadValue/OutOfScope),SetRisk 用
               active_params: Vec<ParsedConfig<S>>,        // 装配处另行解码的启动时生效 StrategyParams(每 (实例, 账户) 一份),SetParams 的比较基准
               journal: J /* CmdResult 源:seq journal 读者 */, clock: TscUtc, engine: EngineInfo) -> Result<Gateway<S, R, J>, GatewayError>;
    pub fn adopt(&mut self, stream: UnixStream, uid: u32); // 接受后、SO_PEERCRED 之后的路径;测试以 socket pair 注入凭证
}
pub trait CtlRing { fn fits(&mut self, lens: &[usize]) -> bool; fn write_all(&mut self, recs: &[Rec]) -> bool; } // 一个请求的全部记录一次写入或一条不写
pub trait ResultSource { fn next_result(&mut self) -> Option<CmdResult>; }             // JournalReader 实现:尾随 seq journal 取 CmdResult
impl<S: StrategySet, R: CtlRing, J: ResultSource> hft_rt::PollTask for Gateway<S, R, J> {
    fn poll_once(&mut self, now: Tsc) -> Polled;           // 非阻塞:accept/recv/send 各至多一批 + 尾随一批 CmdResult;不睡眠
    fn has_pending(&self) -> bool;                         // 有连接待读或待写
    fn wait_fds(&self, out: &mut WaitFds);                   // listener 与各连接 fd,交给 TaskLoop 的 epoll
    fn next_deadline(&self) -> Option<Tsc>;                // 最早的挂起超时
}
```

`CtlRecord`(`encode_into(&mut [u8])`)与 `TscUtc` 以基础层为准:前者在 [hft-event](../hft-event/README.md),后者在 [hft-rt](../hft-rt/README.md);`StrategySet` 在 [hft-strategy-sdk](../hft-strategy-sdk/README.md)。

- 任务模型:由 hft-rt `TaskLoop` 调度,与其他可阻塞任务不同线程;每连接一请求一响应后关闭;无 tokio。处理逻辑在与 socket 无关的 `Core<S, R>`(`on_request(conn, peer_uid, body, now)`、`on_result`、`on_tick`、`on_internal`),`Gateway` 只做 I/O。
- ctl 环里 Multi kind(`ControlCmd`、`ConfigBlob`)的 `CtlRecord` 体 = `PartHdr` + 段(即该段 journal 记录体);`ControlCmd` 恒为单段,`ConfigBlob` 按 `PART_MAX_CHUNK` 分段,逻辑体 = `BlobHdr` + blob。
- `ParsedConfig<S>` 的类型归 hft-sequencer;该 crate 落地前暂定义在本 crate `parsed.rs`(字段同下文),届时原样迁移并改为引用。
- 处理一个写请求,按以下顺序取第一个失败:① 对端 uid([鉴权](#鉴权)第 1 条)→ ② 读帧与模式(`invalid_request`)→ ③ issuer(鉴权第 2 条)→ ④ `hft_ctl_proto::allowed`(uid 上限 ∩ issuer 角色 ∩ 聊天类型;按命令与**目标值**静态分类;`Forbidden`)→ ⑤ `check_confirm_fields`(`invalid_request`)→ ⑥ `cmd_id` 去重缓存([CmdResult 回传](#cmdresult-回传))→ ⑦ 过期(`Expired`)→ ⑧ 限速(`RateLimited`)→ ⑨ `normalize`(`InvalidArgs`)→ ⑩ 命令相关的 gateway 校验(下节)→ ⑪ 写 `CtlRecord`(其引用的 `ConfigBlob` 分段在 `ControlCmd` 之前,另加审计 `Note`)进 ctl 环;对 `SetParams`、`SetRisk` 与 `ReplaceRefdata`(`RefData::load(新 blob, 当前)` 的结果,成败都交给 sequencer 复核)另把**解析好的对象**放进 `Mailbox`(单槽:上一个未被取走时回 `Busy`)。一个请求的全部记录一次写入或一条不写。只读的 `GetSchema` 在 ⑤ 之后直接应答;`ReconcileReport` 跳过 ⑥ 与 ⑧,整组写入 ctl 环后回 `Applied`(没有 `CmdResult`,不进去重缓存)。preview、确认、版本、放宽、闸门、账户类型相关的前置条件都只是**透传**给 sequencer 判定。
- ctl 环满(剩余空间放不下该请求的全部记录)→ 立即回 `Busy`,不排队不阻塞,该请求的审计 `Note` 也不写(环没有空间)。

### gateway 侧的命令校验

只做**不需要 sequencer 状态**的校验;失败直接回 `InvalidArgs{detail}`(hft-event 编解码的错误按同一映射:`ParamsErr` 见下表 `SetParams` ①,`UnknownKind`/`InvalidEnum` → `UnknownDiscriminant`(100))(`ReplaceRefdata` 的装配校验回 `Rejected{RefdataAssembly}`)并审计,不写环。

| 命令 | gateway 校验 |
|---|---|
| `SetParams` | ⓪ `kind_id ∉ S::kinds()` → `SchemaUnsupported`(先于 ③ 的种类比对);① `hft_event::parse_spb1`(失败的 `ParamsErr`:`DupGroup` → `InvalidArgs{GroupIdDup}`(111),其余值 → `InvalidArgs{BadGroup}`(112);容器结构:长度、`GroupSpec`(含 `GroupId` 的构造规则:高 8 位 = 该实例)、`LegDef`;body 不透明,其布局与覆盖表规则在 ⑥ 由 SDK `params` 校验);② `StrategyParamsHdr.strategy/account` 与命令一致(`ScopeMismatch`);③ `rd.check_binding(account, strategy, kind_id)`:账户已绑定到该策略实例、种类一致(`AccountBound`、`KindMismatch`);④ 对每个 `GroupSpec` 调 `rd.check_group`(腿 inst 可交易、venue 在账户内、同基础资产、同报价资产、任两腿不在同一交易所 → `SameExchangeLegs`);`GroupId` 在 blob 内唯一(`GroupIdDup`);⑤ `kind_id ∈ S::kinds()`、`schema_id/schema_ver` 是编译进来的版本、`schema_digest` 与编译进来的 schema 一致(`SchemaUnsupported`、`SchemaMismatch`);⑥ `S::decode_params(kind_id, body, schema_ver, &groups)` 解码、`S::validate_params` 校验(策略与 SDK 返回的 `ParamError`(含 `TooManyGroups`)映射为 `InvalidArgs{ParamInvalid}`,kind `Strategy`,arg 带字段下标或"未指明");⑦ 该种类 `StrategyDesc.non_prod_only` 为真而 `rd.account(account).is_prod` → `ParamInvalid`(kind `NonProdOnly`);⑧ `S::model_mem_bytes(&params, n_groups)` 与 `S::model_snapshot_bytes(&params, n_groups)` 算出新参数下的 Model 内存上界 `model_mem` 与检查点上界 `model_ckpt`,单个实例已超过 `advisor_mem_budget_mib`、或 2 × `model_ckpt` 已超过 `advisor_ckpt_max_mib` → `ParamInvalid`(kind `ModelMemBudget`);再与该 (实例, 账户) 的生效参数副本(见下)比较,按 SDK 的规则判定是否改动了 `FieldDesc.structural` 字段(`structural_changed`),并调 `S::reset_preview` 生成预览所需的估算。全部通过后,解码结果 + `GroupTable` + `model_mem`、`model_ckpt`、`structural_changed`、`reset_preview`(字段归 hft-sequencer `ParsedConfig`)经 Mailbox 交给 sequencer;sequencer 只核对 sha256 并复核 ③ 与 ⑦,再做需要状态的检查(跨 group 的 `GroupId` 全局唯一、`InstOwned`、有仓位 group 不得移除、全部实例的内存上界之和与检查点上界之和不超过预算、类别是否为升级)。detail 码见 [control-plane 结果码](../../docs/subsystems/control-plane.md#结果码) |
| `SetRisk` | 按 hft-risk 的编码解析 `RiskParams`(解析失败 → `BadValue`),作用域外字段改动 → `OutOfScope`;与 `ExecConfig` 的一致性(`ExecRiskMismatch`)由 sequencer 判定 |
| `SimReset`、`SimTransfer`、`SimFault` | 目标账户 `AccountMeta.kind == Sim`(`NotSim`;`prod` 实例没有 Sim 账户,`SimFault` 在那里一律以此被拒);`SimTransfer` 的两个 venue 与 `SimFault` 的 venue 都在该账户 `venues` 内、资产存在(否则 `UnknownName`) |
| `VenueNotice` | venue 存在;`inst ≠ 0` 时属于该 venue(两者不满足 → `UnknownName`);`FundingInterval` 的 `new_s ∈ {3600, 7200, 14400, 28800}`(其他值 → `InvalidArgs{BadValue}`;是否另有别的周期待核实(方法:见开发备忘));`Maintenance` 的 `from_ns < to_ns`(否则 `BadValue`) |
| `AdoptExchange` | `item = Balance` 时 `asset` 存在(否则 `UnknownName`)、`group` 为 `GROUP_NONE` 或高 8 位是该账户绑定的实例(否则 `BadGroup`;是否属于当前参数由 sequencer 复核) |
| `Kill` | `hard` 只与 Global 或 Account 组合;`flatten` 只与 Account 组合且不与 `hard` 同用(否则 `InvalidArgs{BadCombination}`);issuer 角色只有 Viewer 时只接受默认参数的 Global/Account(其余组合在 `allowed` 阶段已 `Forbidden`) |
| `SmokeOrder` | 目标账户 `AccountMeta.kind == Live`(`NotLive`);`venue` 在该账户 `venues` 内、`inst` 属于该 venue(否则 `InvalidArgs{UnknownName}`);`ttl_ms` 在 control-plane 规定的范围内(`BadValue`);其余前置条件(同时至多一张、Kill、报价)由 sequencer 判定 |
| `WriteOffResidual` | `group` 不是 `GROUP_NONE`/`GROUP_ALL` 且高 8 位是该账户绑定的实例(`BadGroup`);group 是否属于当前参数、残余与在途由 sequencer 判定 |
| `ReplaceRefdata` | `refdata_check(新 blob, 当前配置)`:账户名前缀、Live 账户的交易 key、模拟撮合配置干校验、`DustBelowMinNotional`、线程计划覆盖全部 venue;不通过 → 回 `Rejected{RefdataAssembly}`(arg = 失败项,码表归 control-plane),不写环;跨版本不变量由 sequencer 以 `RefData::load` 复核 |
| `StrategyCmd` | 实例存在;`CmdHdr.account ≠ 0` 且 `rd.check_binding` 通过(`AccountBound`);`len ≤ 256`;`group ≠ GROUP_ALL`(group 是否属于该 (实例, 账户) 的当前参数由 sequencer 判定);载荷按 `StrategySet::cmds(kind_id)` 的 `CmdDesc` 用 SDK `params::decode_cmd` 校验(未知 `op`、字段越界 → `InvalidArgs{ParamInvalid}`),`reduce_only` 须与该 `op` 的 `CmdDesc.reduce_only` 一致(不一致同为 `ParamInvalid`,kind `Strategy`,字段未指明) |
| `InjectWarmup` | 实例存在;`CmdHdr.account = 0`(`AccountNotAllowed`);`len ≤ 4096`;不计入 `write_rate_per_min`,按 `warmup_rate_per_s` 限速 |

**`SetParams` 的比较基准**:gateway 为每个 (实例, 账户) 保留一份生效参数的解码副本(只来自装配与 journal,不读 sequencer 内存)——装配时由 hft-core 经 `active_params` 交入启动时生效的参数;尾随 journal 读到本 gateway 写出的 `SetParams` 的 `CmdResult{Applied}` 时换成该次解码结果的副本。同一 (实例, 账户) 上一条 `SetParams` 的结果尚未读到时,新的 `SetParams` 回 `Busy`(基准未定,客户端稍后重试)。预算两项只读 `GatewayConfig`(即 ECF1 段 5),不另设配置。

### `GetSchema`(只读,本地应答)

`{"type":"GetSchema","strategy_kind":"…"}` 由 gateway 在本任务内直接回 `body` = SDK `params` 对 `S::kinds()` 中该种类描述产生的 schema JSON:不写 ctl 环、不产生 `ControlCmd`/`CmdResult`,只写一条审计 `Note{CtlAudit}`。所有白名单身份(含 Viewer)可用;不限速。内容与发布制品里由 `hft-core schemas --out <dir>` 导出的文件逐字节相同(CI 比对)。

### 客户端

```rust
pub struct CtlClient { /* Unix socket 路径 + issuer */ }
impl CtlClient {
    pub fn send(&self, cmd: &CmdRequest, timeout: Duration) -> Result<CmdResponse, ClientError>;
    pub fn preview(&self, cmd: &CmdRequest) -> Result<CmdResponse, ClientError>;     // 带 PREVIEW 标志;sequencer 不改风险与模式状态,回 preview 与 ctl_state_version
    pub fn confirm(&self, cmd: &CmdRequest, state_version: Option<u64>) -> Result<CmdResponse, ClientError>;
        // 新 cmd_id、同一命令体、confirmed = true;一般命令传 Some(预览的 ctl_state_version);
        // Viewer 的 Kill(预览回 rflags.PREVIEW_TTL)传 None,须同一 issuer 在 60 s 内调用
    pub fn result(&self, cmd_id: Uuid) -> Result<CmdResponse, ClientError>;           // 超时后补查:以同一 cmd_id 与原请求体重发(客户端保留本进程发出的请求),
        // 语义即"重复 cmd_id"规则(见 CmdResult 回传);本进程没有该 cmd_id 的请求 → ClientError::UnknownCmdId
    pub fn get_schema(&self, kind: &str) -> Result<serde_json::Value, ClientError>;
    /// 逐块发送生产者(策略的 pipeline 插件)给出的块列表:每块 ≤ WARMUP_MAX_PAYLOAD(4096 B)且自身完整,
    /// 块边界与块内格式归策略,客户端不切字节、不合并;chunk_no 从 0 递增、末块 last = true。
    /// 任一块 > 4096 B → 一块都不发,返回 ClientError::ChunkTooLong;Busy/RateLimited 时退避重试(同一 cmd_id),
    /// 任一块被拒即停止并返回已确认的块数。
    pub fn inject_warmup(&self, instance: &str, local_kind: u16, chunks: &[&[u8]]) -> Result<u32, ClientError>;
}
```

纯 Python 客户端 `crates/hft-ctl/clients/python/hftctl.py`:单文件、只用标准库(`socket`、`struct`、`json`、`uuid`、`time`、`base64`),**不依赖 PyO3/hft-py**,ops_bot、pipeline、运维脚本直接拷贝或以路径依赖引用;提供 `send`、`preview`、`confirm(state_version=None)`、`result`(语义均同 Rust)、`get_schema`、`inject_warmup(instance, local_kind, chunks)`(与 Rust 同一逐块规则,参数是插件产出的块列表;pipeline 的策略插件用它注入预热数据);与 Rust 客户端共用 `fixtures/ctl/` 的金样帧做一致性测试。二次确认只有 [control-plane](../../docs/subsystems/control-plane.md#版本与确认) 定义的一种协议。

## 实现说明

<details><summary>实现要点</summary>

### 线协议

帧、请求/响应模式、规范化与 `cmd_hash` 的唯一定义在 [hft-ctl-proto](../hft-ctl-proto/README.md#请求与响应)。gateway 侧补充:

- `expires_at_ns` 必填(推荐 30 s,上限 300 s);gateway 用 TSC↔UTC 换算为 `expires_tsc` 写进 `CmdHdr`,sequencer 以记录头 `tsc` 判断;收到时已过期(gateway 的 UTC 严格晚于 `expires_at_ns`,相等不算)→ 直接 `Expired`,不写环。
- `cmd_id` 原样写进 `CmdHdr.cmd_id` 与 `CmdResult.cmd_id`;挂起表、缓存与 sequencer 去重都以它为键。

### 鉴权

0. **签发者表的加载**:`PrincipalTable::load(path) -> Result<PrincipalTable, PrincipalsErr>`,`path` = hft-core 配置 `paths.ctl_principals`(位置与"只在启动时加载、改动须重启 hft-core"归 [hft-core 配置文件字段](../../apps/hft-core/README.md#配置文件字段));运行中不重读。文件 TOML,两张表:`[uids]` 把系统用户名映射到主体名(`"hft-opsbot" = "ops_bot"`、`"hft-pipeline" = "pipeline"`、`"hft-recon" = "reconciler"`;运维登录用户写 `"<login>" = "cli"`;主体名与上限归 [hft-ctl-proto 主体表](../hft-ctl-proto/README.md#主体与角色码)),用户名在加载时经 `getpwnam` 解析为 uid;`[issuers]` 把 `tg:<from.id>` 与 `cli:<login>` 映射到角色名数组(`"Viewer"`、`"Trader"`、`"Admin"`)。`PrincipalsErr`:`Io`(读不到)、`Parse`(TOML 错误或未知键)、`UnknownUser`(用户名解析不到 uid)、`UnknownPrincipal`(主体名不在主体表)、`UnknownRole`、`BadIssuer`(不是 `tg:<十进制>` 或 `cli:<login>`,或 login 不是 `[uids]` 中的 `cli` 用户)、`DupEntry`(同一用户名或 issuer 出现两次)。
1. **对端 uid**:`SO_PEERCRED` 取 uid/gid/pid;uid 不在 `principals` → 不读请求,回 `cmd_id` 全零的 `unauthorized` 并关闭。每个 uid 有**权限上限**,uid 名、issuer 与上限的唯一定义是 [hft-ctl-proto 主体表](../hft-ctl-proto/README.md#主体与角色码)(同一个 uid `hft-opsbot` 既以 `tg:<from.id>` 代表 Telegram 用户,也以 `ops_bot` 代表它自身);hft-core 自身的 `cli:systemd` 不经 socket(SIGTERM 路径经 `internal` 通道)。
2. **签发者**:每个 uid 只接受主体表中该 uid 的 issuer 形式(`hft-opsbot` → `tg:<from.id>` 与 `ops_bot`;`hft-pipeline` → `pipeline`;`hft-recon` → `reconciler`;运维登录用户 → `cli:<该 uid 的登录名>`),其他组合与经 socket 到达的 `cli:systemd` → `unauthorized`;`issuer` 在 `principals.issuers` 中查角色集合,查不到 → `unauthorized`。**有效权限 = uid 上限 ∩ issuer 角色 ∩ 聊天类型限制**——ops_bot 负责按 Telegram `from.id` 认人并在请求里带 `chat_type`;群聊放行哪些命令、Viewer(老板)能做什么,以 [hft-ctl-proto 权限表](../hft-ctl-proto/README.md#权限表)为准;面板数据由 ops_bot 直接读 journal/Parquet,不经本通道;gateway 负责 ops_bot 进程不能越过它的上限([09 §5.3](../../design/09-可观测性、风控与控制面.md))。
3. 类别由 `hft_ctl_proto::class_of` 按命令与目标值静态判定,只用于角色上限与限速;**是否需要确认由 sequencer 判定**,gateway 不保存令牌、不保存预览;gateway 只在得到有效角色后调 `check_confirm_fields`(`confirmed = true` 而 `expect_state_version` 缺或为 0 → `invalid_request`,Viewer 的 `Kill` 除外)。
4. 限速:每 issuer 写命令 ≤ `write_rate_per_min`(滑动窗口:时刻落在 (now − 60 s, now] 内的计入请求已有 `write_rate_per_min` 条时,新的计入请求 → `RateLimited`,它本身不计入;计入的是通过 `allowed`、类别为升级或中性的请求,含 preview 请求,之后的步骤是否失败不影响计数;`InjectWarmup` 按最近 1 s 的 `warmup_rate_per_s` 同样计数);**降级类与只读不限速、降级类除 Viewer 的紧急停止外不要求确认**;`InjectWarmup` 只受 `warmup_rate_per_s` 约束(一批预热数据有数百块)。
5. 所有请求(含被拒的、只读的;环满而回 `Busy` 的除外)写一条 `Note{source=CtlAudit}` 到 ctl 环,`v[]` 布局唯一定义在 [hft-event Note 源表](../hft-event/README.md#控制与系统0x04xx0x05xx)。本 crate 的填法:写环放行的命令 `result_code` 写 0(gateway 已放行,执行结果见 `CmdResult`),gateway 拒绝的写拒绝码;`role` 写 `CmdHdr.role`,被 `allowed` 拒绝的写该 issuer 角色集合中的最高角色;`GetSchema`、`ReconcileReport` 与解析不出命令的请求 `cmd_kind` 写 0;解析不出 `cmd_id` 时写全零;未读请求的(未知 uid)issuer 字节写 0;`chat_type` 码 `tg:*` 取请求值、`cli:<login>` 写 Cli、`pipeline` 写 Pipeline、其余主体(含 `cli:systemd`)写 0;对端 uid 写 `SO_PEERCRED` 的 uid(未知 uid 同样写;`internal` 通道来的 `cli:systemd` 写 0),写环的命令同时填 `CmdHdr.peer_uid`;审计进 journal;被拒的不写 `ControlCmd`。`view_account` 与 `chat_type` **只进这条 Note**,`CmdHdr`/`ControlCmd` 不携带它们(命令的目标账户是 `CmdHdr.account` = `CmdScope.account`)。

### 权限表

权限表(角色 × 命令 × 参数 × 确认)的唯一归属是 [hft-ctl-proto 权限表](../hft-ctl-proto/README.md#权限表)(代码生成);本 crate 只执行它。

### chain-core 的 ctl 会话

**optional**:链上策略接入时启用(chain-core 为 optional 组件)。chain-core 运行自己的 gateway 实例:同一线协议、信封、幂等、过期、结果码与二次确认协议(确认由 chain-core 主循环判定),socket `/run/hft/chain-core-<chain>/ctl.sock`(属主 `hft-chain:hft-ctl`,`0660`),命令写进 chain journal;命令语义在 [chain-core 控制面](../../apps/chain-core/README.md)。与 hft-core 的 gateway 不共享状态,不做转发。

| 对端 | uid | 权限上限 | issuer |
|---|---|---|---|
| ops_bot | `hft-opsbot` | Viewer + Trader + Admin | `tg:<from.id>` |
| 运维 CLI | 运维登录用户 | 个人配置的角色 | `cli:<login>` |
| chain-core 自身(SIGTERM 路径) | 进程内 SPSC | Admin | `cli:systemd`(只允许 `Drain`) |

| 命令 | 类别 | Viewer | Trader | Admin | 二次确认 |
|---|---|---|---|---|---|
| `Pause{hard = false}`(放行风险递减意图) | 降级 | — | ✓ | ✓ | 否 |
| `Pause{hard = true}`(拒绝全部新意图) | 降级 | — | ✓ | ✓ | 否 |
| `Drain` | 降级 | — | ✓ | ✓ | 否 |
| `Resume{source, scope}`(只删完全匹配的一条 `Ctl`/`GasBudget` cap) | 升级 | — | ✓ | ✓ | 是 |
| `ClearTaint` | 升级 | — | ✓ | ✓ | 是 |
| `SetBidCap`(仅 builder profile) | 升级 | — | — | ✓ | 是 |
| `SetEndpoint{name, enabled = false}`(端点停用) | 降级 | — | ✓ | ✓ | 否 |
| `SetEndpoint{name, enabled = true}`(启用端点) | 升级 | — | — | ✓ | 是 |

命令语义见 [hft-chain-evm](../hft-chain-evm/README.md) 与 [chain-core](../../apps/chain-core/README.md)。

### CmdResult 回传

响应 JSON 带规范化时算出的 `cmd_hash`(字段归 [hft-ctl-proto `CmdResponse`](../hft-ctl-proto/README.md#请求与响应))。gateway 维护 `pending: FixedMap<cmd_id: Uuid, (conn, deadline)>`(容量 = 并发连接上限 64;已满时新连接的请求回 `Busy`);journal 尾随逻辑在同一线程的 `poll` 循环里:读到 `CmdResult{cmd_id}` → 找到挂起连接 → 写响应并关闭;到达后 `result_wait_ms` 仍未读到(`now ≥ 到达 + result_wait_ms`)→ 回 `accepted`,结果到达后保存在 `dedup` 缓存,`result(cmd_id)` 可取。`dedup` 缓存只登记已写环的命令(写环时登记,读到 `CmdResult` 时填结果;满时淘汰最早登记的一项);gateway 自己拒绝的请求不登记,同一 `cmd_id` 重发时重新判定。重复 `cmd_id`:缓存命中且已有结果 → 直接返回缓存结果,不再写环;命中但结果未到 → 回 `accepted`,不再写环;缓存未命中(gateway 重启或已淘汰)→ 写环,由 sequencer 的去重表返回 `Duplicate`。

### 规划源文件

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | 公开 API |
| `src/core.rs` | 与 socket 无关的处理:鉴权第 2 条起的全部步骤、限速、挂起表、去重、gateway 侧命令校验(含 SetParams 的 SPB1 解析、绑定检查与 `S::decode_params`)、`GetSchema` 本地应答、审计 Note |
| `src/gateway.rs` | `PollTask` 实现:Unix socket、`SO_PEERCRED`、连接状态机、`ResultSource` |
| `src/principals.rs`、`src/config.rs`、`src/ring.rs`、`src/parsed.rs` | 签发者表、配置、ctl 环接口、`ParsedConfig`(暂居) |
| `src/client.rs` | Rust 客户端 |
| `clients/python/hftctl.py`、`clients/python/test_hftctl.py` | 纯 Python 客户端与测试 |
| `tests/golden_frames.rs` | 对 `fixtures/ctl/*.json` 的金样 |

</details>

## 延迟与资源契约

gateway 不在 tick-to-trade 路径,无微秒预算;约束的是**它对热核的影响为零**与命令时延。

| 项 | 目标 | 测量 |
|---|---|---|
| 请求到达 → 写入 ctl 环 | p99 ≤ 2 ms(含 JSON 解析与校验);`SetParams`(500 group)p99 ≤ 20 ms(含 body 解码与 `validate`) | `ctl/ingest`、`ctl/ingest_setparams_500` 基准 + 审计 Note 时间戳 |
| `GetSchema` 应答 | p99 ≤ 2 ms | `ctl/get_schema` |
| `InjectWarmup`(4 KiB 一块)到达 → 写入 ctl 环 | p99 ≤ 1 ms;持续注入时对热核的影响只经 ctl 环(sequencer 每轮按 ctl 批量上限处理) | `ctl/ingest_warmup_4k` |
| 写入 ctl 环 → sequencer 处理 | ≤ 1 轮 `step`(ctl 优先级最高) | gateway 写环时读的 TSC 与该 `ControlCmd` 记录头 `tsc` 之差,gateway 尾随 journal 时配对 |
| 端到端(客户端发出 → 收到 `ok`) | p99 ≤ 50 ms | 集成测试 |
| 内存 | 帧缓冲 1 MiB × 并发 4 + 缓存 4096 项 + 一份待交接的 `ParsedConfig` | 启动时分配(`ParsedConfig` 在本任务分配,sequencer 不释放堆内存) |

- 运行在 warm 线程,不与热核共享物理核;只经 ctl 环(SPSC)、Mailbox 与 journal(只读)与热线程交互,不持有热路径需要的锁([crates/AGENTS.md tier](../AGENTS.md#tier))。
- gateway 故障不影响交易:线程退出 → 命令不可用 → housekeeping 告警;kill 能力由独立的 [kill-switch](../../apps/kill-switch/README.md) 进程兜底(REST 撤单),不依赖本通道。本通道不可达时 ops_bot 的紧急停止改走 kill-switch 的 `stop_core`(撤单、写停机标记、停 `hft-core@prod`;hft-core 下次启动据标记带 `Kill` 起步);流程唯一定义在 [control-plane 版本与确认](../../docs/subsystems/control-plane.md#版本与确认)。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:`hft-ctl-proto`(帧、模式、规范化、权限)、`hft-types`、`hft-event`(含 SPB1 解析)、`hft-refdata`(`check_binding`/`check_group`)、[hft-strategy-sdk](../hft-strategy-sdk/README.md)(`StrategySet` trait 与 `params` 的 schema JSON;具体的策略集合由 apps/hft-core 从 hft-strategy-registry 以泛型参数传入)、`hft-risk`(`RiskParams` 解码)、`hft-sequencer`(仅 `ParsedConfig<S>` 类型)、`hft-journal`(只读者)、`hft-rt`(`ByteSpsc` 生产端、`Mailbox`、`PollTask`、TSC↔UTC)。
- 外部:`serde_json`(仅 gateway 与客户端;warm/cold)、`sha2`(blob 的 sha256)、`base64`、`toml`(签发者表)、`zerocopy`、`libc`(`SO_PEERCRED`、`getpwnam`)。
- 禁止:`tokio`(gateway 在热进程内)、任何 `strategies/*`(只经 `S: StrategySet` 泛型)、任何 hot crate 反向依赖本 crate(sequencer 只看到 ctl 环里的记录与 Mailbox 里的对象)。

## 延伸阅读

- [09 §5 控制面、§4.1 kill](../../design/09-可观测性、风控与控制面.md)、[01 §3.3](../../design/01-总体架构.md)
- [research/00 §1.5 control-v1、§4 opsd 角色与两步确认](../../design/research/00-existing-codebase-inventory.md)
- [control-plane](../../docs/subsystems/control-plane.md)、[cookbook/adding-a-control-command](../../docs/cookbook/adding-a-control-command.md)、[python/ops_bot](../../python/ops_bot/README.md)、[hft-strategy-sdk](../hft-strategy-sdk/README.md)
- 决策出处:[round8-decisions](../../design/_work/round8-decisions.md)

## 已知限制与后续工作

- 只有本机 Unix socket,不做远程转发。可选扩展:optional 的 chain-core 若迁到另一主机,需在该主机部署同一 uid 的转发客户端,或改由 hft-core 经外部 venue 会话转发(链上策略接入立项时定)。
- hft-rt `ByteProducer` 没有剩余空间查询:生产实现的 `CtlRing::fits` 只能乐观返回真,一个请求的后续记录放不下时已写入的前段留在环里(惰性的 `ConfigBlob` 段;命令本身在最后写)。hft-rt 补充按字节的剩余空间查询后改为精确判定(测试环 `VecRing` 已精确)。
- `WaitFds` 至多 64 个 fd:监听者加 64 个连接时最后一个连接不进 epoll 集合(仍被轮询)。
- issuer 真实性依赖客户端进程(ops_bot)的自证。可选扩展:端到端签名(触发条件见 [hft-ctl-proto](../hft-ctl-proto/README.md#已知限制与后续工作))。
- 读命令(状态查询)不走本通道:冷进程直接读 journal/Parquet;gateway 不提供状态服务(`GetSchema` 是编译期静态数据,例外)。
- `SetParams` 的 body 解码在 gateway(warm)里执行策略代码(`ParamSet::decode/validate`);它们必须是纯函数且有界,策略 crate 的 `validate` 预算由 [hft-strategy-sdk](../hft-strategy-sdk/README.md) 规定。

### 开发备忘

<details><summary>给实现者</summary>

- 审计注记用 hft-event 的 `Note.source = CtlAudit`(经输入环到达、只记录不驱动状态,回放既不喂也不比)。
- 角色码(`CmdHdr.role`)与主体表只在 [hft-ctl-proto](../hft-ctl-proto/README.md#主体与角色码);本 crate 的 `PrincipalTable` 按它加载。
- JSON 里的价格/数量用十进制字符串(`"0.0123"`),gateway 按 refdata 的资产指数转定点;禁止 JSON 浮点数进入参数(防精度丢失)。
- 待核实:各 core venue 允许的资金费结算周期集合(`VenueNotice{FundingInterval}` 的合法值;方法:各所合约信息接口与公告历史)。
- 实现顺序:`PollTask` 骨架 → 鉴权与角色上限 → 规范化写环 → 过期/幂等 → CmdResult 回传 → SetParams 校验与 Mailbox → GetSchema → Python 客户端(preview/confirm/get_schema)。

</details>
