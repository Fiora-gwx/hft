---
description: "冷进程:周期性用只读 REST 拉每个 Live 账户在 Binance/OKX/Bitget 现货与 USDT 永续上的挂单、成交、仓位、余额与逐笔资金费流水,与 journal 重建的引擎视图比对,经控制环发 ReconcileReport;支撑严格的启动闸门与 Unknown 收敛;用户流缺口里漏掉的永续缩小差异在连续两轮对账(两轮都无用户流断开、用户流已稳定一个周期)后标 AUTO_EXTERNAL 供自动入账;资金费差额只告警并由账本自动校正,结算资产余额差先扣除同组资金费条目;启动时核对每把只读 key 的 UID;跳过 Sim 账户"
kind: app
tier: cold
scope: core
status: spec
---

# reconciler

## 概要

`reconciler` 是"交易所真值"的唯一持有者([architecture.md §5](../../docs/architecture.md#5-状态所有权)):它用**只读** API key 周期性拉取每个 (Live 账户, venue) 的挂单、成交历史、永续仓位、余额(现金与现货库存)与资金费流水,与从 seq journal 重建的引擎视图(同一份 [hft-ledger](../../crates/hft-ledger/README.md) 记账代码)比对,把差异作为 `ReconcileReport` 经 ctl-gateway 送进 sequencer;分级与动作只在 [hft-risk](../../crates/hft-risk/README.md) 做。它也是 `Unknown` 订单收敛的第二来源(第一来源是 gw 重连后的查单)。它不下单、不撤单、不改任何状态;不生产保证金数据(`MarginReport` 由 gw 按 `VenueCaps.margin_data` 产出)。覆盖范围按 [`VenueCaps`](../../crates/hft-refdata/README.md) 决定:core = Binance、OKX、Bitget 的现货与 USDT 永续;Sim 账户一律跳过(它们的真值在 [hft-sim](../../crates/hft-sim/README.md)),所以只含 Sim 账户的 `sim` 实例不部署本 app;dev2 联调窗口内对指向测试网的 `prod` 角色实例部署(测试网只读 key 与测试网 REST 基址),升级主机上对正式 `prod` 实例部署。hft-core 的启动闸门 `RECONCILED` 依赖它的报告(资金费条目除外)。

## 目录

- [使用本组件](#使用本组件)
  - [覆盖范围与数据来源](#覆盖范围与数据来源)
  - [对账周期与比对基准](#对账周期与比对基准)
  - [比对项](#比对项)
  - [资金费比对](#资金费比对)
  - [现货库存与余额基线](#现货库存与余额基线)
  - [ReconcileReport](#reconcilereport)
  - [启动闸门与严格放行](#启动闸门与严格放行)
  - [Unknown 订单收敛](#unknown-订单收敛)
  - [启动核验(只读 key)](#启动核验只读-key)
  - [与 gw 的分工](#与-gw-的分工)
  - [链上钱包余额扫描(可选扩展)](#链上钱包余额扫描可选扩展)
  - [配置文件字段](#配置文件字段)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```
reconciler run  --config /etc/hft/reconciler.toml             # hft-reconciler.service
reconciler --check --config …                                 # 只加载与校验(含 Sim 账户拒绝),不连接交易所、不读 journal
reconciler once --config … --account <name> [--venue okx-swap] [--dry-run]   # 一次对账;--dry-run 只打印不发报告
```

`once` 先对该账户(给出 `--venue` 时只对该 venue)做[启动核验](#启动核验只读-key),再读 journal 到末尾建引擎视图,对每个 venue 各跑一轮,条目以 JSON 打印到标准输出;不给 `--dry-run` 时同样经 hft-ctl 提交并写轮次明细,给出时不连接 ctl socket、不写 `paths.state`(游标、轮次明细与 `status.json` 都不动)。`once` 的退出码:0 = 各 venue 的一轮都完成(无论有无差异);1 = 有 venue 的一轮因 REST 查询失败未能完成,或核验超时;2 = 配置或核验错误(与 `run` 相同)。`--account` 不在配置的 `accounts` 中、`--venue` 不是该账户的 venue → 退出 2。

### 覆盖范围与数据来源

每个 (venue, MarketKind) 的比对项由 `VenueCaps` 决定(现货没有仓位项;`funding_settle` 决定资金费比对方式)。下表是 core venue 的只读端点;**全部待核实**(本机连不上各所文档站,方法见开发备忘),核实结果以各 `venue-*` README 为准(只读查询的编码与解码入口是 [hft-venue-proto](../../crates/hft-venue-proto/README.md#venueprotocol静态分派) 的只读查询族,由各 `venue-*` 实现、随 M5 落地;本 app 不另写协议知识):

| venue | 挂单 | 成交(游标) | 仓位 | 余额 | 资金费流水 | 按 client id 查单 |
|---|---|---|---|---|---|---|
| `binance-usdm` | `GET /fapi/v1/openOrders` | `GET /fapi/v1/userTrades`(按 symbol,`fromId`) | `GET /fapi/v3/positionRisk` | `GET /fapi/v3/balance` | `GET /fapi/v1/income?incomeType=FUNDING_FEE`(带 symbol) | `GET /fapi/v1/order?origClientOrderId=` |
| `binance-spot` | `GET /api/v3/openOrders` | `GET /api/v3/myTrades`(按 symbol,`fromId`) | — | `GET /api/v3/account` | — | `GET /api/v3/order?origClientOrderId=` |
| `okx-swap` / `okx-spot` | `GET /api/v5/trade/orders-pending` | `GET /api/v5/trade/fills`(近 3 天)/ `fills-history` | `GET /api/v5/account/positions`(仅 swap) | `GET /api/v5/account/balance` | `GET /api/v5/account/bills`(资金费类型) | `GET /api/v5/trade/order?clOrdId=` |
| `bitget-usdt-futures` / `bitget-spot`(UTA) | UTA 未成交订单查询 | UTA 成交查询 | UTA 当前仓位(仅永续) | UTA 资产 | UTA 账单(资金费类型) | UTA 按 `clientOid` 查单 |

- **一个账户在一个交易所只用一个市场**(现货或永续;refdata 加载时校验,违反即 `SameExchangeMarkets`,规则归 [hft-refdata](../../crates/hft-refdata/README.md)):每个 (account, venue) 的余额各比对一次,不存在同所现货与永续共用钱包而被重复计数的情形,本 app 不建模余额域。
- Binance REST 走直连的 `api1–api4`/`fapi` 域名([05](../../design/05-CEX执行层.md))。
- 可选扩展 venue(Gate、Bybit、KuCoin、Hyperliquid)启用时按同表补一行,端点逐所核实。

### 对账周期与比对基准

- 周期:每 (account, venue) `interval_s`(默认 30 s,[09](../../design/09-可观测性、风控与控制面.md) 为 30–60 s)。立即执行一轮的触发:读到 `Note{EpochStart}`(hft-core 冷启动);读到某 (venue, account) 下单连接 `ConnState` 从 `Down` 回到 `Up`;读到针对该 (account, venue) 的 `AdoptExchange`/`SetBalanceBaseline`/`CancelAll` 的 `CmdResult{Applied}`。读到 `Note{OmsNeedsRestQuery}`(OMS 请求按成交历史确认某个槽)时立即对该 (account, venue) 只做 [Unknown 订单收敛](#unknown-订单收敛) 这一步,不等周期——对冲腿的 `hedge_deadline_ms` 远短于对账周期;这一步提交的组只含 `UnknownResolved` 条目、不含 `Summary`(没有条目时不提交),不算一轮,不参与 `AUTO_EXTERNAL` 的两轮判定。
- 引擎视图:尾随 seq journal,用 hft-ledger 的 journal 冷消费者 `JournalLedger`([hft-ledger](../../crates/hft-ledger/README.md#使用本组件);消费哪些记录、常规成交按 `cum_filled` 去重、外来成交与资金费的入账口径都归它,本 app 不另写记账实现)维护每 (account, venue, inst) 的永续仓位、每 (account, venue, asset) 的现金与现货库存(按 group 归属与 unassigned)、每 inst 的资金费累计(含 sequencer 合成的 `FundingSettle{synth}`);sequencer 在 Input 内合成、不另写记录的两类外来成交(带 `AUTO_RECON` 的 `Note{ExternalFill}` 与 `Note{OmsForeignFill}`)同样经它按 [hft-ledger 外部成交](../../crates/hft-ledger/README.md#外部成交externalfill) 第 6、7 步入账(见 [ReconcileReport](#reconcilereport))。每个 slot 的状态由槽状态视图给出([实现说明](#实现说明))。
- **在途窗口**:一轮开始时记下引擎视图所在的 `S0`,发 REST,全部返回后读到 `S1`。满足任一条件的 inst 这一轮不报差异、只计入 `skipped_inflight`:① `S0` 时该 inst 有处于 `Pending*`、在途撤改、`Unknown` 或 `VERIFY` 的槽(静止挂单的 `Live` 槽不算),或墓碑期内的旧代槽;② `(S0 − user_stream_lag, S1]` 内该 inst 有 `OrderDecision`/`OrderEvent`/`ExternalFill`/`FundingSettle`——即引擎时间晚于 `S0` 所指记录的引擎时间减该 venue 的 `user_stream_lag_ms`、且 seq ≤ `S1` 的记录(引擎时间由记录头 `tsc` 与 `EngineClock` 算出)。资产项:该 (account, venue) 上以该资产为基础资产或计价/结算资产的任一 inst 这一轮在途时,该资产的余额同样不比对、计入 `skipped_inflight`。连续 3 轮在途的 inst(与资产)强制按 `S1` 视图报告。`S0_utc`(资金费窗口用)是 `S0` 所指记录的记录头 `tsc` 经段头时钟校准换算的 UTC。
- 收到 `EpochStart` 后对每个 (Live 账户, venue) 各跑一轮并各自提交,不合并。

### 比对项

| 项 | 引擎视图 | 比较量 | 适用 |
|---|---|---|---|
| 永续仓位 | `hft-ledger` 仓位(以成交与 `ExternalFill` 溯源) | 数量差(按 lot)与名义差(按 mark) | `LinearPerp` |
| 余额 | 现金 + 现货库存 + 余额基线 + 在途划转 | 每资产数量差(free + locked 合计) | 全部 |
| 挂单 | 状态为 `Live`/`Pending*` 的 slot | 集合差(孤儿单 / 幽灵单)与剩余数量差 | 全部 |
| 成交 | journal 中的成交回报与 `ExternalFill` | 按交易所订单 id + 成交 id 匹配,缺失/多出的成交 | 全部 |
| 资金费 | `PerSymbolPush`/`RateOnly`:每 (inst, 结算时刻) 已入账的 `FundingSettle` 金额(推送或合成);`BalanceOnlyPush`:每结算时刻的已入钱包额 | 逐笔金额差(见[资金费比对](#资金费比对)第 2 条) | `LinearPerp` |

- reconciler **只报差异、不分级**:条目的 `severity` 只有 `Ignore`/`Warn` 两种,按 [hft-risk 对账分级](../../crates/hft-risk/README.md#对账分级全系统唯一一次)第 2 步的容差口径填写(仓位差 < 1 lot、余额差 < 资产级 `dust`、资金费差 < `funding_dust`(= 结算资产的 `dust`,与 [hft-ledger 资金费对账校正](../../crates/hft-ledger/README.md#资金费对账校正)第 4 步的 `Matched` 同一阈值)→ `Ignore`;其余 `Warn` 并附原始数量与名义),lot 与 `dust` 读 journal 中最新生效的 refdata(资产级 `dust` 的唯一来源是 [hft-refdata `AssetMeta.dust`](../../crates/hft-refdata/README.md#数据结构));连续轮次、名义阈值、cap 与闸门只在 hft-risk 分级,告警阈值在 watchtower。结算资产的余额差先按[资金费比对](#资金费比对)第 5 条修正再定 `severity`。差异条目只为超出容差的项产生(都是 `Warn`);`UnknownResolved` 是证据而不是差异,一律 `Ignore`;`Summary.severity` 在本组有任一 `Warn` 条目(含 `Funding`)或置 `TRUNCATED` 时为 `Warn`,否则 `Ignore`。
- **挂单配对**:交易所挂单的 client order id 按 [hft-types `ClientOrderId`](../../crates/hft-types/README.md) 解出 `(epoch, slot, gen)` 且等于引擎视图中一个 `Live`/`Pending*` 槽 → 本系统挂单(含 `OrderFlags::SMOKE` 的冒烟单);剩余数量差 ≥ 1 lot → `OpenOrder{slot, gen, epoch, engine_qty = 引擎剩余量, exch_qty = 交易所剩余量}`,< 1 lot 不报。解不出或不等于任何这样的槽 → **孤儿单** `OpenOrder{slot = 0, gen = 0, epoch = 0, exch_qty = 交易所剩余量}`。引擎视图中 `Live` 而交易所没有的槽 → **幽灵单** `OpenOrder{slot, gen, epoch, exch_qty = 0}`,同一轮按 client id 查一次该单(端点见上表),结果写进轮次明细。
- 缺失成交的明细(交易所成交 id、时间、是否为强平/ADL 订单)写进 `<state>/rounds/<report_id>.json` 供人工核对;自身强平/ADL 订单按 `VenueCaps.own_liquidation_marker` 识别并在明细中标注。

### 资金费比对

资金费差额**只告警、由账本自动校正**,不参与启动闸门与 `Recon` cap:

1. **窗口**:每轮比对结算时刻落在 `[S0_utc − funding_recon_window_ms, S0_utc − funding_recon_lag_ms]` 内的结算;更晚的结算(推送、合成与交易所流水可能尚未到齐)这一轮不比对,更早的不再比对。两个参数(默认 48 h 与 600 000 ms)都归 [hft-ledger](../../crates/hft-ledger/README.md),reconciler 从 journal 中最新的引擎配置 `ConfigBlob`(引导块里的与之后生效的)读取同一值,不另设配置键。
2. **逐笔**:交易所一侧一律用带 symbol 的资金费流水(收入为正);流水的结算时刻先经 [`funding_settle_norm_ns`](../../crates/hft-types/README.md#资金费结算时刻归一化)(与 hft-ledger、sequencer 合成共用的唯一实现;venue 只给原始时刻)归一化,周期与锚取引擎视图的 `ledger.funding_grid(venue, inst)`(视图重放 `Funding` 事件时经 `on_funding_interval` 维护,从快照起步时按节 10 重建,规则归 [hft-ledger](../../crates/hft-ledger/README.md)),再按 settle_ts 配对。按 `VenueCaps.funding_settle`:
   - `PerSymbolPush`/`RateOnly`:对窗口内每个 (inst, settle_ts),交易所金额 vs 引擎在 `view_seq` 时对同一笔已入账的金额(含 sequencer 合成的 `FundingSettle{synth = 1}`);差 ≥ `funding_dust` → 一条 `Funding` 条目。
   - `BalanceOnlyPush`:钱包只按推送实额变化(合成额只做 PnL 归属),所以按结算时刻比钱包:交易所在该 settle_ts 各 symbol 金额之和 vs hft-ledger 按 (account, venue, settle_ts) 记录的**已入钱包额**(推送丢失时为 0;重复推送已由 ledger 去重);差 ≥ `funding_dust` → 一条账户级 `Funding` 条目(`inst = INST_NONE`),ledger 以 `AdjustDelta::Funding` 动钱包(规则归 hft-ledger)。各 inst 合成额与流水之间的归属差不进条目,由 hft-ledger 的推送残差校验处理。
   - 交易所有而引擎没有时引擎金额为 0,反之交易所金额为 0。
3. **处置**:`Funding` 条目进入 journal 后,hft-ledger 对差额写 `AdjustDelta::Funding` 与 `Note{FundingCorrected}`;下一轮同一笔即为 `Ignore`。watchtower 的 `FundingDiff` 规则对每一笔校正报 P1、同一笔只报一次([watchtower](../watchtower/README.md#健康规则))。不需要也不接受人工命令处置资金费条目。
4. 资金费流水查询失败 → 这一轮不发 `Funding` 条目(不影响其他比对项),连续 `unknown_alert_rounds` 轮失败 → P1(计数与告警状态写 `<state>/status.json`,格式见 [Unknown 订单收敛](#unknown-订单收敛)第 5 条)。
5. **结算资产余额的资金费修正**(分级之前,同一组报告内):资金费差额同时会以同额出现在结算资产(永续的保证金资产,如 USDT)的余额上;若不扣除,它会经 `Balance` 条目进入 `Recon` 计数与启动闸门。组装 (account, venue) 的一组报告时,先算 `funding_adj(asset) = Σ (exch_amount − engine_amount)`(本组全部 `Funding` 条目中结算资产为 `asset` 的;这些条目都对应钱包变化:`BalanceOnlyPush` 的所只有账户级条目),再把该资产的余额比较改为 `diff = exch_qty − (engine_qty + funding_adj)`;`Balance` 条目的 `engine_qty` 填修正后的值(即 ledger 自动校正之后应有的余额),修正量写进轮次明细 `<state>/rounds/<report_id>.json`。之前的轮次已校正过的笔在本组不会再出现(下一轮即 `Ignore`),所以同一笔不会被修正两次。查询失败而本组没有 `Funding` 条目时不修正。
6. **校正窗口内的余额容差**:结算时刻落在最近 `funding_recon_lag_ms` 内的结算这一轮不比对、没有 `Funding` 条目可扣。`RateOnly` 的所在这段时间里引擎钱包用的是合成值,结算资产余额比较的容差加上该资产在窗口内合成资金费的绝对值之和;`BalanceOnlyPush` 的所钱包按推送实额,窗口内尚未收到推送的结算时刻按其合成额绝对值之和计入容差。窗口过后由第 5 条与 ledger 的校正消除差额。

### 现货库存与余额基线

- 现货库存是一等对象([hft-ledger](../../crates/hft-ledger/README.md)):引擎视图里每资产 = Σ 各 group 的库存 + unassigned + 现金。
- **余额基线**:账户里不属于任何策略的资产(平台币、尘埃、历史遗留币)由交易员 `SetBalanceBaseline{account, venue}` 一次性登记(取当时交易所余额),之后余额比对 = 交易所余额 − 基线 − 引擎视图;基线只由该命令改变(命令归 [control-plane](../../docs/subsystems/control-plane.md))。
- **在途划转**:journal 中已 `Applied` 的 `RecordTransfer{account, from, to, asset, amount, ext_ref}` 由 ledger 按登记移动现金(规则归 hft-ledger);到账之前 `to` venue 的交易所余额比引擎视图少 `amount`。reconciler 对每笔这样的登记累加 `pending_in(account, to, asset) += amount`,该资产的比较量改为 `diff = exch_qty − (engine_qty − pending_in)`;某一轮 `|exch_qty − engine_qty|` < 资产 `dust`(已到账)时清除该资产的 `pending_in`。每笔登记按其 `RecordTransfer` 记录的 UTC 计龄(记录头 `tsc` 经段头时钟校准换算),`S0_utc` − 登记 UTC > `pending_in_max_s` 的登记不再计入 `pending_in`(该资产差额照常比较,超出容差即 `Warn` 的 `Balance` 条目),轮次明细把它的 `ext_ref` 标为过期;之后到账使差额回到容差内时照常 `Ignore`。`pending_in` 与对应的 `ext_ref` 写进轮次明细;进程启动后对引擎视图起点之后的每条 Applied 的 `RecordTransfer` 同样累加,已到账的由首轮的同一判定清除。
- 网页手工买卖造成的现货余额差,以 `Balance` 条目报告,交易员用扩展后的 `AdoptExchange`(可采纳余额条目与可选入场价)写成调整;未 Adopt 前该余额差持续以 `Warn` 报告。
- 正式账户上线流程([launch](../../docs/runbooks/launch.md))在首次放行前执行一次 `SetBalanceBaseline`。

### ReconcileReport

布局、`item`/`severity`/`order_status` 码与"末条到达整组生效"的规则归 [hft-event](../../crates/hft-event/README.md)(`ReconcileReport` 不是 `ControlCmd` 变体,见 [control-plane](../../docs/subsystems/control-plane.md))。一轮对账产出 `count` 条条目(同一 `report_id`,`idx` 从 0),经 `hft-ctl` 客户端一次提交整组(uid `hft-recon`;它是进程主体,issuer `reconciler`,权限上限按 [hft-ctl 鉴权](../../crates/hft-ctl/README.md#鉴权) 逐命令列出,不是角色)。填法:

| 条目 `item` | 何时产生 | 字段 |
|---|---|---|
| `Summary` | 每轮一条 | `venue`、`account`、`severity`(这一轮是否有差异)、`view_seq`(= `S0`)、`skipped_inflight`、`sflags`(语义见下) |
| `Position` | 每个超出容差的永续仓位差 | `inst`、`engine_qty`、`exch_qty`、`diff`(名义,USDT 定点)、`exch_ts_ns`、`view_seq`、`sflags`(只用 `AUTO_EXTERNAL`,见下) |
| `Balance` | 每个超出容差的余额差(结算资产先经资金费修正) | `asset`、`engine_qty`(结算资产为修正后的值)、`exch_qty`、`diff`、`view_seq` |
| `OpenOrder` | 孤儿单(`slot = 0`)与幽灵单 | `inst`、`slot`、`gen`、`epoch`、`exch_qty` |
| `Funding` | 窗口内每一笔超出容差的资金费差([资金费比对](#资金费比对)) | `account`、`venue`、`inst`(`BalanceOnlyPush` 的所为 `INST_NONE`)、`exch_ts_ns`(= 归一化后的结算时刻 settle_ts)、`exch_amount`(交易所金额)、`engine_amount`(引擎入账额;`BalanceOnlyPush` 为已入钱包额)、`diff` |
| `UnknownResolved` | `Unknown` 槽查到结果 | `slot`、`gen`、`epoch`、`exch_qty`(累计成交)、`exch_ts_ns`、`order_status`(码表归 hft-event)、`history_checked`(见下) |

- **组的组成**:`report_id` 取这一轮开始时的 UTC 纳秒,不大于已用过的最大值(本进程内,以及持久化在 `paths.state` 的上次运行的值)时取该最大值 + 1;条目顺序为 `Summary`(`idx = 0`)、`UnknownResolved`、`OpenOrder`、`Position`、`Funding`、`Balance`,同类内分别按 `slot`、`(inst, slot)`、`inst`、`(inst, exch_ts_ns)`、`asset` 升序;超过 256 条时截去尾部(`count = 256`,`Summary.sflags` 置 `TRUNCATED`)。`Balance` 条目在 `Funding` 条目选定之后计算,[资金费比对](#资金费比对)第 5 条的 `funding_adj` 只取本组实际包含的 `Funding` 条目。只做 Unknown 收敛的那一步(见[对账周期与比对基准](#对账周期与比对基准))提交的组只含 `UnknownResolved`。
- **`Summary` 的 `skipped_inflight` 与 `sflags`**(字段布局、位值与条目版本归 [hft-event](../../crates/hft-event/README.md);hft-risk 怎样使用它们见 [hft-risk 对账分级](../../crates/hft-risk/README.md#对账分级全系统唯一一次)):
  - `skipped_inflight`(u16,饱和于 65 535):这一轮因[在途窗口](#对账周期与比对基准)条件 ① ② 没有比对的 (inst 或 asset) 项数(连续 3 轮在途后强制报告的项不计)。
  - `USERSTREAM_DOWN`:`(S0 − user_stream_lag, S1]` 内读到该 (venue, account) 的 UserStream 连接 `ConnState{Down}`,或 `S0` 时它不在 Up。
  - `TRUNCATED`:本组条目按 256 截断(被截掉的条目下一轮再报;hft-risk 视为该 (account, venue) 有差异)。
  - `HISTORY_UNCHECKED`:这一轮在某个 `Unknown` 槽的成交历史窗口里见到**未归属成交**(交易所订单 id 不属于引擎已知的任何槽、墓碑或已入账的 `ExternalFill`);只有这一种情况置位。同组该 (account, venue) 的 `UnknownResolved{NotFound}` 全部带 `history_checked = 0`,未归属成交的明细写进轮次明细;watchtower 对它报 P0。
- **`Position` 条目的 `AUTO_EXTERNAL`**(用户流缺口里漏掉的强平/ADL;位值归 hft-event):当前这一轮与上一轮(同一 (account, venue) 的相邻两轮,本进程内连续)对同一 inst 都报出 `Position` 差异,且同时满足下列全部条件时,当前这一轮的条目置 `AUTO_EXTERNAL`:
  1. **永续同号缩小**:`exch_qty` 与 `engine_qty` 不反号(同号,或 `exch_qty = 0`,即全量 ADL),且 `|exch_qty| < |engine_qty|`——交易所仓位比引擎少,方向未变;
  2. **两轮稳定**:两轮的 `engine_qty` 相同、`exch_qty` 相同,且两轮 `view_seq` 之间该 inst 没有任何 `OrderDecision`/`OrderEvent`/`ExternalFill`;
  3. **无在途**:两轮都不满足[在途窗口](#对账周期与比对基准)条件 ① ②(该 inst 没有被计入 `skipped_inflight`);
  4. **无 Unknown**:两轮的 `S0` 时该 (account, venue) 没有 `Unknown` 或 `VERIFY` 槽,且两轮都没有置 `HISTORY_UNCHECKED`、`TRUNCATED`;
  5. **用户流稳定**:满足 [hft-risk `auto_external_ok`](../../crates/hft-risk/README.md#对账分级全系统唯一一次) 的用户流前置条件(两轮的 `Summary` 都没有 `USERSTREAM_DOWN`,且该 (account, venue) 用户流最近一次 Up 早于上一轮的 `view_seq`,即两轮视图点之间一直 Up;口径归 hft-risk,sequencer 复核时调同一函数)。

  条目照常是 `Warn`(分级、cap 与闸门照常归 hft-risk)。sequencer 收齐该组后复核同样的条件,通过则在这条 `ReconcileReport` Input 内确定地合成一笔 `ExternalFill{cause = Unknown}`(数量 = `|engine_qty − exch_qty|`、方向为减仓,全量 ADL 时即整个引擎仓位)入账并 P0,**不另写记录**(复核、合成取值与去重键归 [hft-sequencer](../../crates/hft-sequencer/README.md#每条-input-的处理顺序process),合成函数归 [hft-ledger](../../crates/hft-ledger/README.md#外部成交externalfill))。本 app 的引擎视图经 hft-ledger 的 journal 消费路径调用同一个合成函数 `auto_external_fill`,所以下一轮该项即 `Ignore`;交易所之后若补推同一笔真实强平/ADL,hft-ledger 以 `auto_held` 抵扣、只入账超出部分(规则归 hft-ledger)。现货(`Balance`)差异、仓位变大或变号的差异、只出现一轮的差异从不置位,仍由人工 `AdoptExchange` 处置。轮次明细记录置位的依据(两轮的 `report_id`、数量、`view_seq` 与用户流最近一次 Up 的 seq)。reconciler 重启后重新计数。
- 每条非 `Ignore` 条目由 `(report_id, idx)` 唯一标识,`AdoptExchange` 以它引用;条目带 `view_seq`,该 inst/asset 自 `view_seq` 以来有新成交时 Adopt 被拒(`StaleReport`)。
- 各类条目的处置(命令归 [control-plane](../../docs/subsystems/control-plane.md)):`Position`/`Balance` → `AdoptExchange`(非策略资产的余额差也可由 `SetBalanceBaseline` 登记为基线;带 `AUTO_EXTERNAL` 的 `Position` 由 sequencer 自动入账);孤儿单 `OpenOrder{slot = 0}` → `CancelAll` 撤掉后由下一轮 `Ignore` 收敛;`Funding` → 账本自动校正,不需要命令;`UnknownResolved` → OMS 收敛。不存在"只确认、不改账"的处置命令。

### 启动闸门与严格放行

规则归 [hft-risk 启动闸门](../../crates/hft-risk/README.md);reconciler 为它提供:

1. `EpochStart` 之后每个 (Live 账户, venue) 一组报告(< 5 s,hft-core `startup.gate_timeout_s` = 15 s)。
2. **严格**:`RECONCILED` 只在该 (account, venue) 的最新一组报告(`Funding` 条目除外)**全部 `Ignore`**,或每条非 `Ignore` 条目都已按上节处置时置位;任何量级的差异都不会被"只是 Warn"放行。reconciler 在看到处置命令生效后立即再跑一轮,新一轮全部 `Ignore` 才算收敛。
3. HARD kill 的解除、`Recon` cap 的解除(连续两轮全部 `Ignore`、期间无 UserStream `Down`、`skipped_inflight = 0`)按同一口径;`Funding` 条目不触发 `Recon` cap,也不计入这些条件;reconciler 不发任何恢复或升级命令。
4. 出现 `tail_lost`、`RecoverMismatch`、毒丸隔离或 `--recover-from-snapshot` 时,hft-risk 禁用自动解除 `Recovery`;reconciler 的行为不变,只是差异预期更多。

### Unknown 订单收敛

`Unknown` 只能由证据收敛,不能由传输错误推断([defensive-patterns](../../docs/defensive-patterns.md))。判断规则归 [hft-oms](../../crates/hft-oms/README.md);reconciler 只提供证据:

1. 每轮(以及读到 `Note{OmsNeedsRestQuery}` 时立即)从引擎视图取 `Unknown` 且年龄 > `unknown_min_age_ms`(2,000)的 slot。年龄 = 这一步开始时的本机 UTC − 该槽进入 `Unknown` 的那条记录的 UTC;下文的"决策时刻"是该槽最近一条 `OrderDecision` 记录的 UTC(记录的 UTC 都由记录头 `tsc` 经段头时钟校准换算);查询时刻是发出请求时的本机 UTC。
2. 按 client order id 查单(端点见上表)。
3. 查到 → `UnknownResolved`(累计成交、交易所时间);有成交时成交历史同步进入比对。
4. 查不到 → 再查该 inst 在 `[决策时刻 − history_lookback_s, 现在]` 的成交历史:
   - **成交记录带 client id**(或查询可按 client id 过滤)的 venue:有该订单的成交 → 按成交给出 `exch_qty` 与 `order_status = Filled/PartiallyFilled`;无 → `order_status = NotFound`、`exch_qty = 0`、`history_checked = 1`。
   - **成交记录不带 client id** 的 venue(core 中为 Binance 现货与 USDⓈ-M,以 [venue-binance](../../crates/venue-binance/README.md) 为准):窗口内出现未归属成交 → 该 (account, venue) 这一轮全部 `NotFound` 条目 `history_checked = 0`,`Summary.sflags` 置 `HISTORY_UNCHECKED`;没有未归属成交时,只有按 client id 查单在 `notfound_confirm_rounds`(2)个**不同轮次**(只做 Unknown 收敛的那一步也各算一次)都返回"订单不存在"(Binance `-2013`),且这些查询都晚于决策时刻 + `notfound_grace_ms`,才给 `history_checked = 1`;此前给 `history_checked = 0`。
   - 成交历史查询失败 → `history_checked = 0`。

   OMS 对对冲腿只在 `history_checked = 1` 时才可能收敛,否则保持 `Unknown`([hft-oms](../../crates/hft-oms/README.md))。
5. 查询本身失败(超时、429、5xx)→ 不发条目,下一轮重试;连续 `unknown_alert_rounds`(3)轮失败 → P1(watchtower `UnknownAge` 另给 P0)。连续失败的计数与告警状态写 `<state>/status.json`(每次变化时先写 `.tmp` 再改名):每 (account, venue) 一项 `{account, venue, unknown_query_fail_rounds, funding_query_fail_rounds, alert}`,任一计数 ≥ `unknown_alert_rounds` 时 `alert = "P1"`,否则 `null`;成功一次即把对应计数清 0;告警由 watchtower 读取该文件发出。

### 启动核验(只读 key)

`run` 与 `once` 在发出任何对账请求之前,对配置里每个 (Live 账户, venue) 的只读 key 依次做:

1. **权限**:调用该所 API key 权限查询接口,确认无交易、无提现权限;有 → 退出 2。
2. **UID**:用这把 key 调该所账户信息接口(端点与响应解码在对应 `venue-*` 的账户查询纯函数,与 hft-core 的 `IDENTITY` 查询同一份代码),取交易所 UID,与 refdata `AccountMeta` 中该 (account, venue) 的 `venue_uid` 比较;**不符** → 写 `<state>/fatal.json`(`{code: "UidMismatch", account, venue, utc_ns}`,UID 只记末 4 位)后退出 2,watchtower `ProcessDown` 读到该文件即报 P0([watchtower](../watchtower/README.md#健康规则))。
3. 查询失败(超时、5xx、限速)→ 按 `http.*` 退避重试,`startup_verify_s`(60)内仍未完成 → 写 `fatal.json`(`code: "IdentityUnverified"`)后退出 1(systemd 按普通失败重启,watchtower 报 P1)。

核验通过后删除旧的 `fatal.json`。配置变更(新增账户或 venue)需要重启本进程,新 key 同样先核验。

### 与 gw 的分工

| 数据 | 谁产生 | 进入方式 |
|---|---|---|
| 保证金率、维持/可用保证金、强平价、ADL 档 | gw(按 `VenueCaps.margin_data` 推送或经 REST 池轮询) | `MarginReport`(Input) |
| 自身强平/ADL/人工成交 | gw(用户流 + `own_liquidation_marker`) | `ExternalFill`(Input) |
| 账户配置(持仓模式、保证金模式)、账户 UID | gw(登录后查询) | `AccountConfigReport`(Input) |
| 挂单、成交历史、仓位、余额、资金费流水的周期性真值 | **reconciler** | `ReconcileReport` |

reconciler 不重复轮询保证金数据;gw 漏掉的外部成交会表现为仓位/余额差,由本 app 发现。两腿之间是否对平(`LegTracker`,归 [hft-oms](../../crates/hft-oms/README.md))不在这里判级:reconciler 只保证每条腿各自与交易所一致。

### 链上钱包余额扫描(可选扩展)

触发条件:链上策略与 chain-core 启用时。规则保留:按终局块号、经 ≥ 2 个独立运营商的公共 RPC 读执行钱包余额,只与终局前已 `Finalized` 的意图比较,有在途意图或读取失败的钱包这一轮不判级,外部转入不算差异;差异作为 `Balance` 条目(`venue` = 链的 `VenueId`)进入同一报告。细节与配置(`chain.bsc_rpcs`、`chain.gas_min_bnb_e18`)在启用时按 [chain-core](../chain-core/README.md) 与 [hft-chain-evm](../../crates/hft-chain-evm/README.md) 补齐。

### 配置文件字段

`/etc/hft/reconciler.toml`,`schema = 1`。

| 键 | 类型 | 含义 | 默认 / 来源 |
|---|---|---|---|
| `instance.journal_cold` / `.journal_ram` / `.ctl_socket` | path | 本 app 的键(reconciler.toml 自己的 `[instance]` 表,与 hft-core 配置的同名表无关):被对账的 hft-core 实例的冷副本根、热段根与 ctl socket,取值与该实例配置的 `paths.journal_cold`、`paths.journal_ram`、`paths.ctl_socket` 相同 | 必填 |
| `accounts` | [`{name, venues: {venue → readonly_key_secret}}`] | 只读 key;`name` 即 refdata `AccountMeta.label`;**只能列 Live 账户**(加载时按 refdata `AccountMeta.kind` 校验,出现 Sim 账户即退出 2);秘密由 deploy 的 `hft-secrets` 预取(Secrets Manager 前缀归 deploy) | 必填 |
| `refdata_blob` | path | 加载校验与启动核验用的 refdata(`AccountMeta.kind`、`label`、`venue_uid`、账户的 venue 集合);比对时的 `VenueCaps`、lot 与 `dust` 取引擎视图中 journal 最新的 refdata | 必填;与被对账实例 hft-core 的 `blobs.refdata` 同一文件 |
| `rest_base` | {venue → URL} | 各 venue 的 REST 基址(正式或测试网/模拟盘,与被对账实例的端点一致) | 必填 |
| `startup_verify_s` | u16 | [启动核验](#启动核验只读-key)的总时限 | 60 |
| `interval_s` | u16 | 每 (account, venue) 周期 | 30 |
| `user_stream_lag_ms` | {venue → u32} | 用户流相对 REST 的滞后上界 | 2 000 |
| `http.timeout_ms` / `http.max_rps` | u32 / {venue → u16} | 单次请求超时 / 每所独立令牌桶的速率(按该所 REST 权重预算的 ≤ 20% 设定,见开发备忘) | 5 000 / 必填(`accounts` 用到的每个 venue 一项) |
| `egress_bind` | IP | REST 请求的源地址 | 必填;deploy 生成器填 `[egress].prod` 的私有 IP([deploy 出口 IP](../../deploy/README.md#出口-ip)) |
| `unknown_min_age_ms` / `unknown_alert_rounds` | u32 / u8 | 见上 | 2 000 / 3 |
| `history_lookback_s` | u32 | 查不到订单时成交历史回看窗口 | 60 |
| `notfound_grace_ms` | u32 | 成交记录不带 client id 的 venue:"订单不存在"的查询须晚于决策时刻多久才计入确认 | 10 000(待核实,方法见开发备忘) |
| `notfound_confirm_rounds` | u8 | 同上:需要几个不同轮次的"订单不存在" | 2 |
| `pending_in_max_s` | u32 | 在途划转登记计入 `pending_in` 的最长时间([现货库存与余额基线](#现货库存与余额基线)) | 86 400 |
| `paths.state` | path | 成交与流水游标、轮次明细、`fatal.json`、`status.json`、上次使用的 `report_id` | `/var/lib/hft/reconciler` |
| `chain.*` | 表 | 可选扩展,见上节 | — |

加载校验(`--check` 与启动相同,收集全部错误后退出 2):未知键、`schema ≠ 1`、缺必填键;`startup_verify_s`、`interval_s`、`http.timeout_ms`、`http.max_rps` 的每一项、`unknown_alert_rounds`、`notfound_confirm_rounds`、`pending_in_max_s` 为 0;取值超出类型范围;`refdata_blob` 读不出或 hft-refdata 加载失败;`accounts` 为空、`name` 不在 refdata、某 venue 不是该账户在 refdata 中的 venue、秘密名不以 `-readonly` 结尾;`accounts` 用到的 venue 在 `rest_base` 或 `http.max_rps` 中缺项。

内存上限按主机类见 [hft-core 内存表](../hft-core/README.md#内存与-journal-段按主机类唯一归属)。

## 实现说明

<details><summary>实现要点</summary>

- 结构:`tokio` 运行时;每 (account, venue) 一个任务按周期执行一轮;journal 尾随与 `hft-ledger` 视图在一个专用线程上(单写者),任务通过快照请求取某个 `S0` 时刻的视图副本。
- REST 客户端:每所一个签名器(签名纯函数来自 `venue-*`,只读端点),独立令牌桶,用响应中的限速余量(如 Binance `X-MBX-USED-WEIGHT`)闭环:响应报告的已用权重达到该所计数窗口限额的 20% 时,该窗口结束前暂停该所的请求。
- 成交与资金费游标持久化在 `paths.state`;游标丢失时成交回退到 24 h 窗口、资金费回退到 `funding_recon_window_ms` 窗口,并去重。
- Binance 的按 symbol 成交查询:只查引擎视图中 `S0` 前 24 h 内有过订单或持仓的 symbol,另每 6 h(从进程启动后的首次对账起)全量扫一次有余额的资产对应 symbol。
- 差异计算(输入两份视图、同组 `Funding` 条目、容差、上一轮每 (account, venue, inst) 的 `Position` 差异记录与上一轮的 `USERSTREAM_DOWN`、`view_seq`,以及用户流最近一次 Up 的 seq,输出条目列表与这一轮的差异记录)是 `src/diff.rs` 的纯函数,只做逐项相减、资金费修正、`Ignore`/`Warn` 标注与 `AUTO_EXTERNAL` 判定,属性测试在本 app 的 `tests/`;上一轮的差异记录只在内存(进程重启即清空);分级规则只在 [hft-risk 对账分级](../../crates/hft-risk/README.md#对账分级全系统唯一一次),本 app 不复制。
- 引擎视图中的槽状态:用 hft-oms 的只读类型(槽状态枚举、订单 id `(epoch, slot, gen)`)解读 `StateSnapshot` 的 OMS 节与其后的 `OrderDecision`/`OrderEvent`/OMS 迁移 Note,只区分"终态 / 非终态 / `Unknown` / 墓碑期",不复制 OMS 状态机。

**规划的源文件**

| 文件 | 职责 |
|---|---|
| `src/main.rs` | 子命令 |
| `src/config.rs` | TOML 与校验(含 Sim 账户拒绝) |
| `src/identity.rs` | 启动核验:key 权限、UID 与 `venue_uid`、`fatal.json` |
| `src/view.rs` | journal 尾随 + 引擎视图(hft-ledger `JournalLedger`)+ 槽状态视图 + `S0` 快照 |
| `src/diff.rs` | 差异计算纯函数(逐项相减、结算资产资金费修正、`Ignore`/`Warn` 标注、`AUTO_EXTERNAL` 的两轮判定) |
| `src/rest/{binance_usdm,binance_spot,okx,bitget}.rs` | 每所只读查询编排(编码与解码经 hft-venue-proto 只读查询族,由 `venue-*` 实现) |
| `src/funding.rs` | 资金费逐笔比对(窗口、按 (inst, settle_ts) 配对、`Funding` 条目) |
| `src/unknown.rs` | Unknown 收敛与成交历史确认 |
| `src/report.rs` | 组装 `ReconcileReport` 并经 `hft-ctl` 送出 |
| `src/chain.rs`(可选扩展) | 钱包余额扫描 |

</details>

## 延迟与资源契约

冷进程,无热路径约束。

| 指标 | 目标 |
|---|---|
| 一轮(单 account × 单 venue)REST 往返 | Binance < 50 ms、OKX/Bitget < 200 ms(p99,同 AZ;OKX/Bitget 经 CDN,待实测) |
| `Note{EpochStart}` → 报告进入 journal | < 5 s |
| REST 权重占用 | ≤ 该所 IP/账户限额的 20%(与 prod 实例共用 `[egress].prod` 出口,[deploy 出口 IP](../../deploy/README.md#出口-ip)) |
| CPU / 内存 | ≤ 0.5 核;内存按 [hft-core 内存表](../hft-core/README.md#内存与-journal-段按主机类唯一归属) |

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:`hft-event`、`hft-journal`(只用 `JournalReader`)、`hft-ledger`(引擎视图即其 `JournalLedger`,同一记账代码)、`hft-oms`(只读类型:槽状态枚举与订单 id,用于解读快照与 OMS 事件;不调用其状态机)、`hft-risk`(只读类型;分级规则见其[对账分级](../../crates/hft-risk/README.md#对账分级全系统唯一一次),本 app 不调用其分级代码)、`hft-refdata`、`hft-venue-proto`(只读查询族与账户查询的接口)、core `venue-*`(其实现:只读端点与账户信息查询的编码与响应解码纯函数)、`hft-ctl`(客户端)。
- 外部:`tokio`、`reqwest`(rustls)、`serde`;可选扩展 `alloy`(只用 `eth_call` 编码)。
- 禁止:交易/撤单端点的任何封装;交易权限的 key(加载时检查 key 名后缀 `-readonly`,并在启动时调用各所 API key 权限查询接口确认无交易权限)。

## 延伸阅读

- [05](../../design/05-CEX执行层.md)、[09](../../design/09-可观测性、风控与控制面.md)、[07](../../design/07-策略层.md)、[12](../../design/12-风险与未决问题.md)
- [defensive-patterns](../../docs/defensive-patterns.md)、[hft-ledger](../../crates/hft-ledger/README.md)、[hft-risk](../../crates/hft-risk/README.md)、[hft-oms](../../crates/hft-oms/README.md)、[hft-refdata `VenueCaps`](../../crates/hft-refdata/README.md)
- [round8-decisions](../../design/_work/round8-decisions.md)

## 已知限制与后续工作

- 手续费返佣、结算时间差导致的短暂差异由 `dust` 与在途机制吸收;频繁 `Warn` 时按资产调 `dust`。
- 结算资产的资金费修正只扣同一组报告里的 `Funding` 条目;资金费流水查询失败的轮次不修正,资金费差额会以 `Warn` 余额条目出现,下一轮流水恢复后消失(单轮 `Warn` 不触发 `Recon`)。
- Binance 现货与 Bitget 没有交易所侧死手,孤儿单风险更高;reconciler 只发现,撤单由 hft-core 或 kill-switch 执行。
- 跨所划转(treasury 建议后人工执行)造成的余额差:人工划转后经 `RecordTransfer` 登记,引擎视图记为在途,到账前该余额差不报;reconciler 以 `ext_ref` 与交易所记录核对,见 [treasury](../../python/treasury/README.md)。
- OKX 成交接口只回看 3 天(`fills`),更早用 `fills-history`;reconciler 停机超过回看窗口时只从当前状态重建基准并告警。
- `refdata_blob` 读文件,只用于加载校验与启动核验:被对账实例执行 `ReplaceRefdata` 之后 journal 中的 refdata 可能比该文件新(如账户集合或 `venue_uid`),须同步替换文件并重启 reconciler,否则加载校验与启动核验按旧值进行;比对一直用 journal 中最新的 refdata。

### 开发备忘

<details><summary>给实现者</summary>

- **待核实**(全部 core 端点):上表每个端点的路径、版本、分页、限速权重与字段;OKX 统一账户与 Bitget UTA 模式下,只用现货或只用永续的子账户其余额接口返回哪些资产(比对取该账户全部资产);各所资金费流水的类型码与是否逐笔带 symbol 和结算时刻;Bitget UTA v3 的具体路径。方法:官方 API 文档(在可访问的网络环境)+ 测试网/模拟盘只读 key 实测(测试网不提供的接口在正式账户开通后以只读 key 补测),结果写进对应 `venue-*` README 的开发备忘,再回填本表;各所测试网/模拟盘是否提供资金费流水同样待核实。
- **待核实**(方法:官方文档 + 测试网只读 key 实测,同上):各所"API key 权限查询"接口(Binance `GET /sapi/v1/account/apiRestrictions`、OKX `GET /api/v5/account/config`、Bitget 对应接口);没有的所只能靠 key 管理流程保证只读,并在上线流程中人工核对。
- **待核实**(方法:测试网/模拟盘下单后按 client id 查成交历史):查不到订单时成交历史能否按 client order id 过滤(OKX `clOrdId`、Bitget `clientOid`);不能时按交易所订单 id 与时间窗匹配,并按"成交记录不带 client id 的 venue"一支处理。
- **待核实**:`notfound_grace_ms` 的取值——Binance 对"执行状态未知"(如 `-1007` 超时)的订单,多久之后按 client id 查单的"不存在"才可信;方法:官方文档 + 测试网在下单超时后持续查单与查成交的实测,结果写进 venue-binance 开发备忘后回填默认值。
- reconciler 与 hft-core 共用 `prod` 出口(只读 key 的白名单只含该 EIP),REST 权重会挤占交易连接的 IP 限额,所以 `http.max_rps` 按该所 IP 权重预算的 ≤ 20% 设定;各所按 IP 还是按账户计权重待核实(方法同上)。
- 实现顺序:Binance USD-M → Binance 现货 → OKX → Bitget;每所先挂单/仓位/余额,再成交与资金费,最后 Unknown 收敛。

</details>
