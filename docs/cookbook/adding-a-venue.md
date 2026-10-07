# Cookbook:接入一个新交易所(venue)

把一个新交易所产品线接进 hft-core 的逐步清单。**新增交易所 = 新建 `venue-<x>` crate 实现 `VenueProtocol` + 给出 `VenueCaps` 默认值 + 金样 + fuzz + 账户配置核验 + 保证金数据 + 强平识别;策略与执行层不改。** 以 [venue-binance](../../crates/venue-binance/README.md)(资料最全)与 [venue-okx](../../crates/venue-okx/README.md)(推送型保证金数据)为模板;本清单与它们不一致时,修这里。trait 与契约在 [hft-venue-proto](../../crates/hft-venue-proto/README.md),运行时在 [hft-venue](../../crates/hft-venue/README.md),能力表类型在 [hft-refdata](../../crates/hft-refdata/README.md);设计理由见 [13 O2](../../design/13-设计评审与优化.md) 与 Agent Note `venue-caps`。

一个"venue"是一个交易所的一个产品线(如 `binance-usdm`、`okx-spot`),只属于一种 `MarketKind`(`Spot` 或 `LinearPerp`),有自己的连接、协议与限速([glossary](../glossary.md))。同一交易所的现货与永续放在同一个 `venue-<exchange>` crate 里,各自一个实现 `VenueProtocol` 的类型。

每一步末尾的**验证**必须全部通过才进入下一步。

## 改动边界(先读)

| 允许改 | 不允许改 |
|---|---|
| 新 crate `crates/venue-<x>/` | `strategies/*`、[hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md)、[hft-exec](../../crates/hft-exec/README.md)、[hft-oms](../../crates/hft-oms/README.md)、[hft-risk](../../crates/hft-risk/README.md)、[hft-ledger](../../crates/hft-ledger/README.md)、[hft-sequencer](../../crates/hft-sequencer/README.md)、[hft-sim](../../crates/hft-sim/README.md)、[hft-venue](../../crates/hft-venue/README.md)、[hft-venue-proto](../../crates/hft-venue-proto/README.md) |
| [hft-types](../../crates/hft-types/README.md) 的 `VenueId` 常量表(只追加一行) | 任何事件体、控制命令(新交易所不需要新事件) |
| refdata 配置与 `tools/hft-cfg` 的 venue 注册(一行) | 策略参数 schema |
| [docs/tests/README.md](../tests/README.md) §3 的前缀表(追加一行)、`fixtures/<NAME>/` | — |
| [apps/hft-core](../../apps/hft-core/README.md) 装配(泛型实例化、线程计划、端点、凭据)、[kill-switch](../../apps/kill-switch/README.md) 与 [reconciler](../../apps/reconciler/README.md) 的 venue 配置/适配 | — |

交易所差异只能落在 `venue-<x>` 的代码与 `VenueCaps` 里。若发现某个差异无法用现有 `VenueCaps` 字段与契约表达(例如一种新的资金费推送方式),**停下来**:先改 hft-refdata 的 `VenueCaps` 与 [hft-venue-proto 契约](../../crates/hft-venue-proto/README.md#实现方必须遵守的契约)(写 Agent Note),再继续;不要在策略或执行层加交易所分支。

## 0. 先把事实写下来

1. 在 [research/02](../../design/research/02-exchange-connectivity.md) 或新的调研笔记里确认,两种 MarketKind 分别写:
   - 连接:撮合位置与前置(直连 / CloudFront / Cloudflare)、最快 BBO 频道及节拍、分档快照频道(档位、节拍、序号与时间字段、订阅后是否立即推当前快照)、增量深度频道与序号规则(可选扩展用)、心跳要求(**按产品线与连接种类分别写**:服务端 ping 间隔、无 pong 断开时限、是否回应 WS 协议 Ping)、BBO 订阅后是否立即推当前值、连接与订阅限速(Ping/Pong 是否计入入向消息数)、连接被交易所强制断开的时长(写进 refdata `VenueMeta.ws_max_conn_age_s`);
   - 下单:通道(WS 下单是否可用——决定 egress 形态,**M3 之前必须核实**)、认证与"登录后免签"、批量/改单、IOC/FOK/post-only 写法、过期保护字段(及其生效位置:网关还是撮合入口——决定查询"不存在"能否定终态)、死手能力(倒计时的最小值与刷新方式)、client id 约束、时间戳字段语义、错误码全表(哪些是"结果未知";码是数字还是字符串)、成交历史接口是否带 client id;成交 id 是全局唯一还是按交易对编号、强平/ADL 成交是否带成交 id;
   - 账户:账户模式(持仓模式、保证金模式、多资产/统一账户)及其查询端点;**每把交易 key**(下单会话 key 与 REST key,若不是同一把)查询 UID 的方法与 API key 权限;现货账户的借币 / 现货杠杆 / 自动借币开关(必须能读到并关闭)或"现货钱包不能借币"的依据;永续每个 inst 的实际杠杆如何读取;一个账户在同一交易所只用一个产品线(refdata `SameExchangeMarkets`),所以统一账户下现货与合约共用钱包的细节只需写明、不需建模;
   - 资金与风险:手续费扣哪个币(含平台币抵扣规则)、资金费实收是否推送、是否带品种、推送里的结算时刻字段与能区分同一时刻不同推送的字段、保证金率/强平价/ADL 档的来源(推送还是轮询、权重)、自身强平/ADL/交割成交在回报里如何识别;
   - 用户流:全部私有事件名(哪些要解码、哪些可忽略);同一账户的私有频道能否在两条连接上同时订阅(先建后断的前提,**M3 之前必须核实**);私有流断开期间发生的成交(含强平、ADL)在重连或重订阅后是否补推(测试网/模拟盘断流演练,[hft-venue 开发备忘](../../crates/hft-venue/README.md#开发备忘)第 8 条);
   - 测试网/模拟盘:是否存在、行情与下单端点、测试网符号与主网的映射(上线门槛 ⑥ 用测试网行情与测试网 refdata)、能否读到 UID(决定测试账户的 `uid_policy`,见第 7 步)、限制。
2. 每一条标来源等级(官方 / 自测 / 社区);找不到一手来源的标"待核实",并写出核实方法(经东京服务器抓官方页、测试网/模拟盘实测、向交易所书面确认;只有正式实盘才会出现的样本——真实强平、ADL——在上线后从 raw journal 补核,上线前必须确定的项不得依赖它)。本机连不上多数交易所文档站,抓到的原文缓存到 `architecture-v2/research/_src/`。
3. 从东京测试实例测每个端点逐 A 记录的 TCP 握手 RTT 与应用层往返(`research/_src/probe2.py`、`ws_rtt.py`)。

**验证**:事实表覆盖上面全部条目;每个"待核实"都有核实方法;两种 MarketKind 都有结论(不适用的写原因)。

## 1. 建 crate

```
crates/venue-<exchange>/
  README.md          按 crates/AGENTS.md 模板;frontmatter tier: pure、scope(core 或 optional;optional 在概要顶部写触发条件)、status: spec
  TESTS.md           测试规格(格式、编号与前缀登记归 docs/tests/README.md;scope: core 的 venue 必有)
  Cargo.toml         依赖见下
  src/lib.rs         每个产品线一个零大小类型:pub struct <Exchange><Product>;
  src/caps.rs        每个产品线的 default_caps()
  src/cid.rs         client id 外形(包装 hft-types ClientOrderId)与请求 id
  src/sign.rs        登录/REST 签名(只被 warm 线程与 REST 路径调用)
  src/md*.rs         行情解码
  src/orders.rs      订单模板与 encode
  src/reports.rs     回报与用户流解码(含用户流事件表)
  src/external.rs    外部成交识别(own_liquidation_marker)
  src/acct.rs        账户配置与保证金查询的请求与解码
  src/codes.rs       错误码两张表:确定拒绝 / 结果未知(未登记码默认结果未知)
  src/mock.rs        `mock` 特性:反向编解码(给 optional 的 mock-exchange)
  tests/golden.rs    金样驱动(用 hft-venue-proto 的 testkit feature)
  tests/contract.rs  hft-venue-proto testkit 的全部契约检查
  benches/           classify / decode / encode / acct_decode 基准
  fuzz/              cargo-fuzz 目标
```

`Cargo.toml` 允许的依赖:`hft-types`、`hft-refdata`、`hft-event`、`hft-venue-proto`、`sonic-rs` 与 `serde`(`derive`,只为 sonic-rs 专用结构体)、签名库(`hmac`+`sha2`、`ed25519-dalek`、`k256` 等按需)、`base64`/`hex`、`memchr`。`serde_json` 只进 `[dev-dependencies]`。**不得**依赖 `hft-venue`、`hft-net`、`hft-journal`、`hft-rt`、`tokio` 或任何 I/O crate([crates/AGENTS.md 依赖方向](../../crates/AGENTS.md#依赖方向))。

把 crate 加进 workspace `members`;在 [hft-types](../../crates/hft-types/README.md) 的 `VenueId` 常量表追加编号(永不复用);在 [README.md 组件一览](../../README.md) 加一行。

**验证**:

1. `cargo check -p venue-<exchange>` 单独构建通过,且 `cargo tree -p venue-<exchange> -e normal` 里没有 `hft-venue`、`hft-net`、`hft-journal`、`hft-rt`、`tokio`。
2. `cargo xtask deps` 通过。
3. `cargo xtask doc-check` 通过(README 章节结构、相对链接、[必备节](../../crates/hft-venue-proto/README.md#每个-venue-readme-的必备节))。

## 2. 实现 `VenueProtocol`

按这个顺序实现,每组写完就补单元测试(全部要求见 [hft-venue-proto 契约](../../crates/hft-venue-proto/README.md#实现方必须遵守的契约)):

1. **常量与会话**:`NAME`(venue 名,不要求与 `VenueId` 常量名同形)、`VENUE`、`MARKET`、`ORDER_FRAME_CAP`、`BBO_SNAPSHOT_BATCH`、`CANCEL_ALL_SWEEP`、`routes()`(BBO 流必须单独一条 `RouteKind::Bbo` 路由——独立连接、双路赛马、由 `feed-bbo` 任务承载、进 bbo 环;一条 `RouteKind::DepthTopN` 分档快照路由并给出 `topn_levels`;成交/标记价/资金费是 `RouteKind::Md` 路由;后两类单路,由 `feed-md` 任务承载)、`bootstrap`(收该账户的 REST 凭证)、`endpoint`、`login`/`login_result`、`subscribe`、`private_subscribe`(只会发给私有频道归属连接,热备不订阅)、`heartbeat`、`app_ping`、`bbo_probe`(BBO 连接每 `feed.bbo_ping_ms` 的存活探测,服务端不回 WS Pong 时改用应用层 ping)、`session_identity_request`/`decode_identity`(会话 key 与 REST key 不是同一把时,用会话 key 查 UID;契约 23)。心跳规则是协议常量,写进代码并在 README 的"心跳与 BBO 探测"节按连接种类标来源。
2. **行情**(热路径方法显式接收 `rd: &RefData`,`FeedCtx`/`GwCtx` 只放本所私有状态):`classify`(带连接所属路由,同名推送按路由区分)、`peek`、`seq_rule`、`decode_md`、`control`、`resync_method`;分档快照类经 `MdSink::l2_snapshot` 产出 `L2Snapshot{origin = TopN}`,`resync_method = None`(契约 26);订阅后不立即推当前 BBO 的所另实现 `bbo_snapshot_request`/`decode_bbo_snapshot`(契约 24),由 warm 任务 snap-fetch 在它自己的 `FeedCtx` 上调用,不得依赖 `decode_md` 写下的状态(契约 25)。增量深度是可选扩展:要支持它时再实现 `align_first` 与 `snapshot_request`/`decode_snapshot`,并写「L2 序号归一化」表。价格/数量保持字符串,按 refdata 的**线格式指数**(`px_wire_exp`/`qty_wire_exp`,已含合约乘数与大面值合约的平移)直接解析成资产单位定点;运行时不出现任何乘除合约乘数(`ctVal` 等);禁止 `serde_json::Value` 与 f64。README 写「分档快照流」表(频道、档位、节拍、序号与时间字段,未核实的写方法)。资金费率与下次结算时刻经 `MdSink::funding` 产出,实际结算周期随事件给出。
3. **时间戳**:按 [event-model 时间戳语义](../subsystems/event-model.md#时间戳语义) 映射;没有就写 0;不按数值大小猜单位。
4. **下单**:`build_templates`(收该账户的 REST 凭证,契约 7)、`encode`(热,零分配)、`render_cid`/`parse_cid`(只加本所外形)、`classify_report`、`decode_report`;只能走 REST 的订单操作(全撤、查单等)用 `order_channel` 声明并实现 `encode_rest`,gw 的全部 REST 响应经 `decode_rest`(槽取自 `RestOrigin`,契约 29);需要会话维护(如 listenKey 续期)的实现 `session_keepalive_request`。查询结果与撤改拒绝按契约 28 填 `OrderEvent.detail`;时间戳越界、限速、用户流终止与维护预告置 `ReportOutcome.ctl`(契约 30);批量响应中结果未知的条目逐条产出 `OutcomeUnknown`(契约 8)。只有 `codes.rs` 登记为确定拒绝的码产出 Reject;原始码按契约 8 编码成 `raw_code`/`reject_code`;成交类与查询结果必须给出 `cum_filled` 与 `avg_px`;reduce-only 字段只在 `oflags` 带 `REDUCE_ONLY` 时填,产品线不支持时返回 `Unsupported`。README 写「累计量与费用来源」「结果未知码表」。
5. **回报里的交易所差异**(都只在本 crate 内处理):
   - 每条带费用的事件填**实际** `fee` 与 `fee_asset`;与 `fee_asset_rule` 不符时置 `fee_rule_deviation`,不改数(契约 19)。
   - 自身强平/ADL/交割/人工成交按契约 18 的顺序判定,产出 `ExternalFill`;README 写「外部成交识别」表。
   - 资金费按本所 `funding_settle` 产出或不产出 `FundingSettle`,venue 从不合成(契约 14)。
   - 不认得或不需要的用户流事件返回 `skip`,不是 `DecodeErr`(契约 20);README 写「用户流事件表」。
   - 同一笔成交、同一条资金费推送只产出一次(`TradeIdRing`,契约 22):成交键 `(inst, 成交 id)`,成交 id 缺失、0 或负(强平/ADL)时用 `(inst, 订单 id, 本笔后的累计成交量)`;资金费推送键 `(inst, 结算时刻, disc)`,README 的用户流事件表写明 `disc` 取哪个字段。
6. **账户查询**:`acct_queries(Config)` 与 `acct_queries(Margin)` 的请求序列(每个 `AcctQuery` 带名义 `weight`)、`acct_request`、`acct_decode`、`acct_finish`;后三者由 warm 任务 acct-fetch 用它自己的 `AcctCtx` 调用,不读写 `GwCtx`(契约 27)。Config 产出 `AccountConfigReport`(读不到即 `UNREADABLE`;REST key 的 UID、key 权限、现货借币开关、永续每 inst 的杠杆条目也在这里;会话 key 的 UID 由运行时并入同一报告的 `uid_keys`);Margin 产出 `MarginReport`(统一口径:`margin_ratio = maint_margin / margin_balance`);推送型保证金数据的所在 `decode_report` 里产出并提供兜底请求。README 写「账户配置核验」「保证金数据换算」表(含权重与频率,确认不超过 IP 配额的 10%)。另实现 reconciler 用的只读查询族 `ro_request`/`ro_decode`(挂单、成交、资金费流水、余额、持仓;契约 31),README 写「只读查询」表。
7. **限速、过期、死手**:`rate_rules()`、`stale_guard()`(没有过期字段的返回 `None`;`recvWindow` 类返回 `RecvWindow`——两者的查询"不存在"都不定终态,由成交历史确认或对账收敛;只有带过期时刻且生效位置已核实的 `ExpireAt` 才允许非对冲槽在 grace 后定终态,契约 10)、`dead_man()`/`dead_man_refresh`(倒计时类必须满足 `timeout_ms ≥ max(min_ms, 2 × refresh_ms)`;没有交易所侧死手就返回 `None`,由 kill-switch 兜底;变体与 refdata `DeadManCap` 同名一一对应、时长一律毫秒,字段对应见 [hft-venue-proto `DeadManSpec`](../../crates/hft-venue-proto/README.md#辅助类型),交易所线上以秒计的在 `dead_man_refresh` 里换算)。
8. 需要逐单签名的所实现 `needs_per_order_signing() = true`,签名放在 `encode` 里并单独基准。

**验证**:

1. `cargo test -p venue-<exchange>` 通过,包括:client id 往返与格式、`encode` 输出能被 `sonic-rs` 解析且字段与输入 `GwCmd` 一致、定点溢出返回错误不 panic。
2. `cargo bench -p venue-<exchange>`:每个入口的 p50 与 p99 不宽于 [hft-venue-proto 统一门槛表](../../crates/hft-venue-proto/README.md#延迟与资源契约)(`benches/budgets.toml` 由 `doc-check` 比对;门槛只在那张表维护)。

## 3. `VenueCaps` 默认值

1. 在 `src/caps.rs` 为每个产品线实现 `default_caps(asset)`(平台币等以资产名给出,经参数按 refdata 资产表解析),字段按 [hft-refdata](../../crates/hft-refdata/README.md) 的 `VenueCaps`:`fee_asset_rule`、`reduce_only`、`required_position_mode`/`required_margin_mode`(及多资产要求)、`required_spot_borrow`(现货能借币的所为 `Some(false)`,现货钱包不能借币的所为 `None` 并核验账户类型)、`funding_settle`、`margin_data`、`own_liquidation_marker`(按本系统要求的持仓模式给取值,例如单向持仓下的强平标记)、`ignorable_user_events`、能力位 `amend`/`post_only`/`ioc`/`fok`/`batch_cancel`、`amend_qty_down_only`(改单只能减量时)、`dms`(`DeadManCap` 取值)、`testnet`。
2. README 写「VenueCaps 默认值」表,每个字段标来源等级;默认值尚待核实时(方法写在开发备忘)取**偏安全**的一侧(例:不确定资金费是否带品种 → `BalanceOnlyPush` 或 `RateOnly`,由 sequencer 合成;不确定保证金是否推送 → `Poll`;不确定是否有过期保护、或过期字段的生效位置尚待核实 → `stale_guard = None`;不确定是否支持 FOK → 能力位关)。
3. `tools/hft-cfg` 注册该 venue:以 `default_caps()` 为底、叠加 refdata 配置里的覆盖写入 refdata blob;与 `stale_guard()`、`dead_man()` 不一致时加载失败。

**验证**:`tests/contract.rs` 的 `check_default_caps` 通过;hft-cfg 对该 venue 生成的 refdata 加载通过,`VenueCaps` 生效值与 README 表一致(hft-cfg 导出 JSON 比对)。

## 4. 抽金样

1. 来源(按优先级):现有 raw journal / 采集系统数据 → 测试网/模拟盘 → 正式实盘开始后的 raw journal(补充只在实盘出现的样本)。放 `fixtures/<NAME>/<msg_type>/<case>.{in,expected.jsonl,meta.toml}`(`<NAME>` = `VenueProtocol::NAME`,每个产品线一个目录),规则见 [fixtures](../../fixtures/README.md)。
2. 覆盖(两种 MarketKind 各一套):
   - 行情:每个消息类别至少一条正常样本;分档快照另需重复序号、档数不足 N;支持增量深度时另需删档、空边、序号断裂、快照对齐的 Drop / Apply / Gap;
   - 下单:每个已登记错误码、每个"结果未知"码、限速、过期拒单、撤单时已成交;
   - 回报:全部订单状态、"成交先于回执"、带费用与不带费用(之后 `FeeOnly`)、平台币抵扣与不抵扣两种扣费;
   - 外部成交:强平、ADL、交割、人工单各一条(真实样本取不到的按官方原文构造并在文件名标 `synthetic`);成交 id 为 0 或负的样本、两个 inst 成交 id 相同的样本;
   - 数量换算:合约乘数 ≠ 1 的永续与大面值合约(1000 倍类)的下单帧与成交推送各一组;
   - 用户流:可忽略事件、合成的未知事件名、资金费结算推送(含同一推送重复送达);
   - 账户查询:每个 `AccountConfigReport` 建议位至少一个触发样本、保证金响应(含无强平价、各 ADL 档);
   - 只读查询:挂单、成交(含翻页)、资金费流水、余额与持仓响应各一组(随 M5)。
3. 期望输出写成归一化事件的规范 JSON(格式归 [fixtures 金样文件格式](../../fixtures/README.md#金样文件格式)),由金样驱动把解码输出渲染后逐行比较。
4. 金样不对时修 fixture 来源,**不改归一化逻辑去迁就**([testing](../testing.md))。

**验证**:`cargo test -p venue-<exchange> --test golden` 与 `--test contract` 全绿;该 venue `TESTS.md` 中 `golden` 用例列出的每类样本都有文件。

## 5. fuzz

1. 每个产品线一个解码器目标,名字 = `VenueProtocol::NAME`,目标内按首字节分派到行情、快照、回报与账户查询解码(有 SBE 的加 SBE 分支);这是上线门槛 ③ 计数的目标(清单与计法归 [testing](../testing.md))。另可加不计入 ③ 的 `fuzz_<x>_encode_roundtrip`。
2. 语料 = 金样 payload + 变异;构建用 `panic=abort`(单热进程里任何解析 panic 都是全停)。

**验证**:解码器目标满足 [testing §8](../testing.md#8-上线门槛) 门槛 ③(core 所);崩溃样本最小化后进 `fixtures/<NAME>/regressions/`。

## 6. 假交易所、测试网与 mock 回环

1. 在 `apps/hft-core/tests` 的进程内夹具(真实 `GwRuntime<P>`/`FeedRuntime<P>` + sequencer,交易所侧帧由测试注入,与 [scenarios](../tests/scenarios.md) 的 C12、FP-8 同一夹具;hft-testkit 不依赖 hft-venue)为该 venue 加场景(行情回放、登录/订阅/下单应答、断连、快照延迟、限速错误、时钟越界拒单、回报乱序、晚到费用、外部成交、账户查询失败、私有频道切换)。
2. 在交易所测试网/模拟盘上跑真实往返(dev2 上 `prod` 角色的测试网实例,行情与下单都指向测试网:`feed_env = "testnet"`、`order_env = "testnet"` 与测试网 refdata,[hft-core 实例角色](../../apps/hft-core/README.md#实例角色prod-与-sim)):登录、行情、下单/撤单、用户流、账户配置核验(含每把 key 的 UID)、保证金查询。测试网/模拟盘不可用或不覆盖的接口逐项记录,按 [testing §8](../testing.md#8-上线门槛) 门槛 ⑥ 的规则处理;不以实盘小单替代。
3. (可选,wire 级 mock,需要在统一升级主机上做延迟验收时)实现 `mock` 特性的四个反向编解码函数,在 [mock-exchange](../../apps/mock-exchange/README.md) 加场景,用**生产二进制**跑回环(只有含 `endpoint_override` 的测试配置才加载 mock CA,见 [hft-net](../../crates/hft-net/README.md))。

**验证**:

1. 行情:分档快照连接断开 → 其上每个 inst `ConnState{Resync, reason=CONN_LOST}` → 新连接首条 `L2Snapshot{TopN}` → `ConnState{Up}`;从不发 REST 深度快照(支持增量深度时另验:断裂 → `Resync` → `L2Snapshot` → 连续 `L2Batch` → `Up`,重连后必重同步);BBO 双路去重输出无重复无缺失,深度连接断开不影响 bbo 环;盘口 10 s 不变时 BBO 连接每秒仍有 `ConnState{Alive}` 或数据帧([hft-venue §3a](../../crates/hft-venue/README.md#3a-bbo-连接健康))。
2. 下单:每条订单类 `GwCmd` 恰好一条 `SendReport`;kill 位矩阵的判定与交易所无关,由 [hft-venue TESTS](../../crates/hft-venue/TESTS.md) 覆盖,本步只核对该 venue 账户的 kill 自检经真实 `KillWord` 通过;连接断开时在途单进 `Unknown` 并经查询收敛。
3. 账户:启动后产出一条 `AccountConfigReport` 且 `mismatches = 0`、`uid_keys` 覆盖全部交易 key、永续每个可交易 inst 有杠杆条目(测试账户按 `VenueCaps` 要求配置好);`MarginReport` 按 `margin_data` 的方式与频率产出;人为改错一项配置(如持仓模式、打开现货借币)→ 对应位置位;换一把别的子账户的 key → `UID` 置位、hft-core 以退出码 2 退出。
4. 用户流:测试网上运行 24 h,未知事件计数逐条有结论(加入可忽略名单或补解码)。
5. `RawTx` 里没有登录帧与签名(用哨兵密钥全文扫描)。
6. (仅统一升级主机类 `c16`/`c32`,wire 级 mock 回环或测试网下单)记录主机内 `tick_to_trade`,数字对照 [latency-budget §3](../latency-budget.md),判定口径(硬线与目标)归 [testing §8](../testing.md#8-上线门槛) 门槛 ⑩;`dev2` 不验延迟。

## 7. refdata

1. 在 `tools/hft-cfg`(refdata 的唯一来源)加该 venue:`VenueId`、每个 inst 的 `InstMeta`(交易所符号、数值 id 如 OKX `instIdCode`/Hyperliquid 资产下标、tick、lot、合约乘数写成 `qty_unit_shift`/`px_unit_shift`(只支持 10 的幂)、价格规则)、所属资产(决定 `px_exp/qty_exp`,同一资产跨 venue 必须一致)、费率档、`RateLimitTable`(官方默认 × 账户等级)、资金费周期、维护窗口、`VenueCaps` 覆盖(若有)。模拟盘与实盘的数值 id 不同时生成两套。
2. 账户:为用到该 venue 的每个账户在 `AccountMeta` 登记 `venue_uid`(启动闸门 `IDENTITY` 对每把交易 key 比对;测试网读不到 UID 的账户用 `uid_policy = "unverifiable"`,适用条件与放行方式归 hft-refdata)、`is_prod`(测试网/模拟盘账户为 false)与绑定的策略实例([hft-refdata](../../crates/hft-refdata/README.md))。同一账户的 venue 集合里不得有同一交易所的两个产品线(加载失败 `SameExchangeMarkets`;Sim 账户同样适用)。
3. 缺引用、指数不一致、未知 venue、`VenueCaps` 与协议不一致一律让加载失败([AGENTS.md](../../AGENTS.md#常驻规则))。

**验证**:refdata blob 加载通过;对该 venue 每个 inst,`encode` 能用该 inst 的 tick/lot 生成合法帧(属性测试);同资产在各 venue 的指数相同。

## 8. hft-core 装配

在 [apps/hft-core](../../apps/hft-core/README.md) 的配置与装配代码里:

1. 以泛型参数实例化 `FeedRuntime<P>`/`SnapFetchTask<P>`/`GwRuntime<P>`/`AcctFetchTask<P>`,并在当前主机类的 `[[threads]]` 线程计划里放入行情任务与(下单时)`gw:<venue>` 任务([hft-core 线程计划](../../apps/hft-core/README.md#线程计划)):统一升级主机上是 `feed-bbo:<venue>` 与 `feed-md:<venue>` 两个任务(`feed-bbo:*` 可跨 venue 同线程,但不得与 `feed-md:*` 同线程),dev2/c4 可用单个 `feed:<venue>`;`SnapFetchTask<P>` 并入已有的 warm 任务 `snap-fetch`,(下单时)`AcctFetchTask<P>` 并入 warm 任务 `acct-fetch`([hft-venue §1](../../crates/hft-venue/README.md#1-线程与环由-hft-core-装配))。**`gw:<venue>` 独占一个线程**(所有主机类),统一升级主机上独占一个隔离核;核不够就换更大的主机类,不与行情任务合并。
2. `endpoints[]`:部署期钉选的 IP、SNI、路径;REST 端点表(按 venue README 的端点表顺序,下标即 `RestReqBuf.endpoint`;账户查询可能用到第二个主机,[hft-venue §4a](../../crates/hft-venue/README.md#4a-账户查询保证金轮询与配置核验));直连优先于 CDN 前置。
3. 凭据:API key/私钥进 Secrets Manager,启动期载入;登录只在 warm 线程;journal 不记录。交易 key 不带提现权限、绑 IP 白名单([runbooks/launch](../runbooks/launch.md));会话 key 与 REST key 分开时两把都登记(`[[accounts]].keys` 的 `session`/`rest`)。
4. 环与 raw journal:名单与各自的存在条件归 [hft-venue §1](../../crates/hft-venue/README.md#1-线程与环由-hft-core-装配)(`bbo-`、`md-`(+ bulk)、`feedctl-`、`snapreq-`/`snap-`、行情拆分时的 `-bbo` 一组,下单时的 `rep-`(+ bulk)、`out-`、`acctreq-`、`acct-`,以及 `raw-feed-`、`raw-gw-`);容量按 hft-core 环容量表。
5. kill 字:该 gw 一个 `KillWord`(`AtomicU32`),全局位掩码由 hft-core 给出,账户位按 refdata `venue_account_ix` 取 `account(ix)` 与 `account_hard(ix)`(位定义见 [hft-types](../../crates/hft-types/README.md),语义见 [hft-risk](../../crates/hft-risk/README.md));启动 kill 自检经真实的 store/load 覆盖该 gw。
6. 死手:交易所侧死手的刷新周期(`refresh_ms`)进 `DeadMan` 定时器,倒计时取 `timeout_ms`(≥ 2 × `refresh_ms`)。不论有无死手,每个 (Live 账户, 该 venue) 都在 kill-switch 配置里加撤单目标与它专用的凭证(缺凭证 kill-switch 启动失败;凭证要求与 `exclude` 规则归 [kill-switch 配置文件字段](../../apps/kill-switch/README.md#配置文件字段))。
7. 模拟账户:不需要额外装配——`sim` 实例里 `AccountKind::Sim` 的账户可直接绑定该 venue 的行情,撮合规则取自 refdata 与 `VenueCaps`,延迟与保证金模型取自 hft-core 配置的 `[sim.latency_models]`/`[sim.margin_models]`([hft-sim 配置](../../crates/hft-sim/README.md#配置))。

**验证**:

1. hft-core 以只行情模式启动,该 venue 的 `ConnState{Up}` 出现在 seq journal。
2. 测试网/模拟盘账户(dev2 上 `order_env = "testnet"` 的 `prod` 实例)启动后启动闸门 `ACCOUNT_CFG`、`IDENTITY` 置位放行(`uid_policy = "unverifiable"` 的账户按 hft-refdata 的放行方式);`MarginReport` 按频率出现。
3. `cargo xtask replay-verify <journal>` 对含该 venue 的 1 小时 journal 逐字节一致。
4. 一个绑定该 venue 的模拟账户运行 1 小时:拒单规则(步长、最小名义、reduceOnly 支持与否)与手续费扣币和测试网实测一致。

## 9. 运维接入

1. [reconciler](../../apps/reconciler/README.md):按 `VenueCaps` 覆盖该 venue 的挂单、成交、持仓、余额与资金费流水(带品种,用于校正合成的资金费)。
2. [watchtower](../../apps/watchtower/README.md):该 venue 的分段直方图、未知事件计数、`fee_rule_deviation` 计数、保证金数据过期告警。
3. [runbooks](../runbooks/README.md):连接轮换、维护窗口、密钥轮换、账户配置(持仓模式、保证金模式、扣费开关)的设置步骤。

**验证**:reconciler 对测试账户一轮对账结果全部 `Ignore`;watchtower 看板出现该 venue。

## 10. 上线与收尾

1. core 的所按 [testing §8 上线门槛](../testing.md#8-上线门槛) 走完(测试网/模拟盘联调与连续 3 天冒烟、模拟账户长跑、统一升级主机上的延迟验收)后进入正式实盘,流程见 [runbooks/launch](../runbooks/launch.md);上线门槛里没有实盘小单阶段。optional 的所在被某个策略用到之前同样走完这些步骤。之后每个改动 `venue-*`、`hft-venue`、`hft-net` 的版本先在 dev2 测试网窗口冒烟,再上升级主机([release-and-deploy](../runbooks/release-and-deploy.md))。
2. venue README 的"开发备忘"里已核实的项目移入正文(改来源等级),仍未核实的保留;frontmatter `status` 在第一次合入实现时改为 `impl`。
3. 若过程中改了 `VenueProtocol` 或 `VenueCaps`,同一改动更新 [hft-venue-proto](../../crates/hft-venue-proto/README.md)、[hft-refdata](../../crates/hft-refdata/README.md) 与所有实现;持久的取舍(例如不接某个通道)写 Agent Note。

**验证**:`cargo xtask doc-check` 通过;本次改动的 diff 只涉及"改动边界"表左列的路径(CI 中按路径白名单检查);PR 描述附测试网冒烟与重放一致性结果。
