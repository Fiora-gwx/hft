# Cookbook:新增一个控制命令

新增命令 = `ControlCmd` 的一个变体 + [hft-ctl-proto](../../crates/hft-ctl-proto/README.md) 的模式与权限表一行 + sequencer 的执行分支 + ops_bot 映射([architecture §8](../architecture.md#8-新行为放哪))。命令语义的唯一定义在 [control-plane](../subsystems/control-plane.md);帧、模式、规范化与权限在 hft-ctl-proto;传输与鉴权在 [hft-ctl](../../crates/hft-ctl/README.md)。以 `Foo{account, group, x}` 为例。

**新增或修改策略不需要新命令**:策略参数走通用的 `SetParams`(body 按策略自带的 schema 解码,gateway 经装配处的 `StrategySet` 校验),模式走 `SetMode`,只读的参数描述走 `GetSchema`,只服务某个策略的人工操作(如人工开平某组)走通用的 `StrategyCmd`(载荷 ≤ 256 B,由策略定义,经 SDK `EventRef::Manual` 投递),冷启动预热数据走 `InjectWarmup`(载荷由策略定义,交给该实例的 `Model`)。只有引擎需要执行的新操作才走本页。

## 1. 先写语义

命令分 core 与 optional,见 [control-plane 命令集](../subsystems/control-plane.md#命令集):core = 首次上线(新子账户上线 ccex 策略)必须具备、必须通过上线门槛的命令;optional = 已分配编码与语义、未实现前 `perms.rs` 不含、gateway 返回 `Forbidden` 的命令。新命令在命令集表的 `scope` 列标明——只有首次上线必需的标 core;标 optional 时在语义列写触发条件(何时值得做),不写周次。角色:Admin / Trader / Viewer([control-plane 角色](../subsystems/control-plane.md#角色));Viewer 的写权限只有紧急停止(默认参数的 `Kill{Global | Account}`,确认方式是它独有的"同一 issuer 的预览 `cmd_hash` + 60 s TTL",见 [control-plane 版本与确认](../subsystems/control-plane.md#版本与确认))与无状态效果的 `Annotate{OpsNote | LocalWrite}`,新命令一律不给 Viewer,也不得复用这种不比对 `ctl_state_version` 的确认方式。进程主体(`ops_bot` 自身、pipeline、reconciler、`cli:systemd`,[主体表](../../crates/hft-ctl-proto/README.md#主体与角色码))默认不得发新命令;确需给某个主体时,在主体表写明范围。

1. 在 [control-plane.md](../subsystems/control-plane.md) 的命令集表加一行:参数、类别(降级/升级/中性)、语义、前置条件与会用到的结果码。类别判定:只减少风险 → 降级(永远允许、不确认);可能增加风险或解除保护 → 升级(二次确认;启动闸门未放行或 DRAINING 期间 `PreconditionFailed`,唯一例外是带 `flatten` 的 `Kill`,新命令不得再开例外);其余中性。只允许 Admin 的命令(如 `SmokeOrder`、`WriteOffResidual`)在权限表的 Trader 列写 `—`。
2. 若需要新结果码或新 detail 码,在[结果码与 detail 码表](../subsystems/control-plane.md#结果码)追加(编号永不复用;detail 码全表唯一,按所属结果码的编号段取下一个值)。
3. 若命令改变业务状态,写明它在回放时如何复现(只依赖命令内容与记录头 `tsc`);命令登记的、要跨重启保存的时刻(截止、冷却)存 `engine_ns`([event-model 时间戳语义](../subsystems/event-model.md#时间戳语义)),不存 TSC。

**验证**:`cargo xtask doc-check` 通过;评审确认类别正确——任何"放宽"语义都不能标成降级。

另外写明:

- **账户语义**:降级类不带账户时作用于全部账户;升级与中性类必须显式账户(唯一例外是实例级的 `InjectWarmup`,账户必须为 `*`;新命令不得再开例外);载荷中的账户必须等于 `CmdHdr.account`;目标里的 group 用 `CmdScope.group`,"全部"编码为 `GROUP_ALL`(`u32::MAX`),不是 0;带 `strategy` 的命令要求账户绑定到该实例(`AccountBound`);写明命令对实盘与模拟账户是否都适用(只适用一种时返回 `NotSim`/`NotLive`)。
- **静态类别**(只由命令与目标值决定,如 `SetMode → Live` 升级、`Kill` 带 `flatten` 升级)写进 hft-ctl-proto 的 `class_of`;**依赖状态的判定**(放宽、是否需要确认、`ctl_state_version`、闸门)只在 sequencer,gateway 不复制状态。
- **与 cap 的关系**:命令若解除保护,必须表达为 `Resume{source, scope}` 风格(只删完全匹配的一条 cap),不得直接改 `ModeTable` 或清多条 cap;若会改变有效模式,`ctl_state_version` 必须 +1。

## 2. 事件定义

按 [adding-an-event-kind](adding-an-event-kind.md) 在 `hft-event` 的 `ControlCmd` 追加 `Foo` 变体并登记新的 `CmdKind` 编号(只追加、永不复用,退役编号如 `0x000B`–`0x000D` 也不复用;目标用 `CmdScope`,大参数用 `BlobRef` 引用先写入的 `ConfigBlob`)。已有命令加字段 = 载荷尾部追加 + `cmd_ver` 加一。

**验证**:`cargo xtask gen-schema --check` 通过且生成物更新(它以 PR 目标分支的合并基点为基线,拒绝 `CmdKind` 编号的改动与复用,规则归 [hft-event](../../crates/hft-event/README.md));旧读者读到新变体得到 `UnknownKind` → `CmdResult{InvalidArgs, detail = UnknownDiscriminant}`,不 panic。

## 3. gateway:JSON 模式与规范化

1. `crates/hft-ctl-proto/src/schema.rs` 增加 `{"type":"Foo", …}` 的模式;数值用十进制字符串,按 refdata 资产指数转定点;引用(account、group)解析为稠密 id。
2. `crates/hft-ctl-proto/src/normalize.rs` 生成 `ControlCmd::Foo` 并经 `CtlRecord::encode_into` 编码。
3. 若为"放宽/收紧"可变的命令,在比较器中登记哪些字段决定类别。
4. 命令带大对象(参数表、名单)时:原始字节作为 `ConfigBlob` 分段写 ctl 环(`ByteSpsc`,单条 ≤ 容量/4),解析好的对象经 `Mailbox` 交给 sequencer,sequencer 在热线程只核对 sha256、不解析;旧对象经反向 Mailbox 交还 gateway 释放。

**验证**:单元测试覆盖每个参数错误(缺字段、越界、未知 group)→ `InvalidArgs`,且不写环;账户取值按 [control-plane 账户语义](../subsystems/control-plane.md#账户语义)第 9 条登记(必须具体 / 只能 `*` / 两者均可);`fixtures/ctl/foo_*.json` 金样帧解析结果固定。

## 4. 权限表

在 `crates/hft-ctl-proto/src/perms.rs` 加一行;[hft-ctl-proto README 权限表](../../crates/hft-ctl-proto/README.md#权限表) 由 doc-check 从代码生成并比对。

权限表是权限的唯一归属:control-plane、ops_bot 只链接它,不另写。

**验证**:鉴权测试矩阵:每个角色 × 聊天类型 × 关键参数组合的允许/拒绝;群聊只能发默认参数的降级命令;升级/中性命令缺账户 → `InvalidArgs{AccountRequired}`;`CmdHdr.account ≠ CmdScope.account` → `InvalidArgs`;Viewer 发该命令 → `Forbidden`;升级命令的第二步若带旧的 `expect_state_version` → `ConfirmInvalid{StateChanged}`,不带 → `invalid_request`(`check_confirm_fields`);`needs_confirm_static` 对新命令不返回 `PreviewBound`。

## 5. sequencer 执行分支

1. `crates/hft-sequencer/src/commands.rs` 加分支:过期与去重由公共代码完成,分支只做"校验前置条件 → 调用拥有该状态的组件 → 返回结果码"。
2. 状态改变必须落在单写者组件里(策略、风控、OMS、ModeTable),分支本身不持有状态。
3. 需要通知策略的,经 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md) 已有的 `EventRef` 变体投递(`Mode`、`Risk`、`Notice`…);确需新变体时改 SDK 并按 SDK 的兼容规则发版,不在命令分支里直接调策略。

**验证**:单元测试:`Applied`、`NoOp`(重复到达目标状态)、`PreconditionFailed`、`UnknownTarget`;`Expired`(构造 `tsc > expires_tsc`)不执行。

## 6. 回放与幂等

1. 用 testkit 生成含 `Foo` 的 journal,`cargo xtask replay-verify`。
2. 同一 `cmd_id` 发两次(gateway 缓存命中、gateway 重启后命中 sequencer 去重)。

**验证**:回放逐字节一致;两种重复路径都只执行一次(`Applied` + `Duplicate`)。

## 7. 客户端

1. Rust 客户端:`CtlClient` 不需要改(命令体是 JSON);如需便捷方法,加 `fn foo(...)`。
2. Python 客户端 `crates/hft-ctl/clients/python/hftctl.py`:加 `foo(...)` 方法(只用标准库);升级类走 `preview()` → `confirm(state_version)`(唯一的二次确认协议,见 [control-plane](../subsystems/control-plane.md#版本与确认);`state_version=None` 只用于 Viewer 的 `Kill`)。

**验证**:`python -m pytest crates/hft-ctl/clients/python` 在干净 venv 通过;对金样帧逐字节一致。

## 8. ops_bot 映射

1. `python/ops_bot` 增加 Telegram 命令或按钮,映射到 `Foo`;升级/交易类用内联按钮:先发 preview,展示 `CmdResult{preview}`,用户确认后带 `confirmed` 与 `expect_state_version` 重发(sequencer 判定 `NeedsConfirm`/`StateChanged`)。
2. 在 [ops_bot 的命令表](../../python/ops_bot/README.md) 加一行(命令名归 ops_bot),并确认它满足 [control-plane 的映射规则](../subsystems/control-plane.md#与-ops_bot-命令的映射)。
3. 结果消息带账户标签前缀,模拟账户标注"模拟"。

**验证**:dev 环境里用 Viewer、Trader 两个 Telegram 身份各试一次:Viewer 收到 `Forbidden`(紧急停止以外的新命令一律如此);Trader 完成两步确认后收到 `Applied`;journal 中能看到审计 `Note{source=CtlAudit}`、`ControlCmd`、`CmdResult` 三条记录。

## 9. 收尾

上面每一步的**验证**都写成用例:模式、规范化、权限与金样进 [hft-ctl-proto 的 TESTS.md](../../crates/hft-ctl-proto/TESTS.md),gateway 校验与端到端结果码进 [hft-ctl 的 TESTS.md](../../crates/hft-ctl/TESTS.md),执行分支进 hft-sequencer 的 `TESTS.md`(格式、编号与修改流程见 [docs/tests](../tests/README.md));`cargo xtask ci`(含 `cargo xtask test-trace`)全绿;若命令改变了运维流程,更新对应 runbook;需要决策理由的(例如为什么它是中性而不是升级)写 Agent Note。
