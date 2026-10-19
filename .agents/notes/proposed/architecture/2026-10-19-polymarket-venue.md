# Agent Note: 接入 Polymarket CLOB(二元结果代币)

Status: proposed

## Problem

本仓库按 [ccex-first-scope](2026-10-02-ccex-first-scope.md) 首要服务 CEX-CEX。负责人决定(2026-10-19)在本系统内交易 Polymarket 的 5 分钟涨跌盘(BTC、ETH、SOL、XRP)：用同一套 journal、sequencer、OMS、风控与控制面，而不是另写一个执行器。Polymarket 与现有假设的冲突点如下。

1. **品种是动态的。** 每 5 分钟每个资产出现一个新市场，含 Up/Down 两个 token。Gamma API 约提前 24 小时创建。refdata 是启动时加载的不可变 blob，变更要计划内重启([hft-refdata](../../../../crates/hft-refdata/README.md))；每天约 2300 个新 token，不能按重启节奏加入。
2. **报价与持仓。** 价格在 0.01–0.99 之间，tick 为 0.01，部分市场为 0.001；数量单位是份。到期按结果兑付 1 或 0 USDC。卖出需要先持有 token，不能做空。
3. **费率非线性。** 吃单每份手续费为 0.07·p·(1−p)，挂单免费并有返佣。`FeePair` 只能表示线性费率。
4. **签名方式。** 每笔订单都要用 Polygon 钱包做 EIP-712(secp256k1)签名，再加 L2 HMAC 头，经 REST `POST /order` 提交，撤单也走 REST。回报来自认证的 user 频道 WS。仓库现在没有 EVM 签名，`SignAlg` 只有 HMAC 与 Ed25519。
5. **订单类型。** GTC、GTD(到期时间)、FOK、FAK、post-only。现有 `GwCmd` 没有到期时间。
6. **结算在链上。** 市场结算后，赢的 token 要在 Polygon 上 redeem，现有账本没有"到期兑付"。
7. **行情。** market 频道推 `book` 快照、带 best bid/ask 的 `price_change` 增量和 `last_trade_price`，没有序号；断线后靠重订阅取全量 `book` 恢复。服务端在消费变慢时会发 1013 断开。

## Proposal

**把每个 outcome token 当作一个现货品种。** 资产为 `PM-<asset>-<UP|DN>-slot<k>`，计价资产为 USDC.e。这样能复用 Spot 已有的语义，包括 `TargetReject::SpotShort`(不能卖出未持有的 token)和余额记账。不新增 `MarketKind`。

1. **槽位绑定，refdata 不动。**
   - refdata 为每个资产预置固定数量的槽位品种，例如每资产 8 个窗口 × 2 个 token。tick、lot、费率模型、限速都写在槽位上。
   - 新增 `ControlCmd::BindSlot{inst, external_id, window_start, window_end}`。由冷路径的发现进程(读 Gamma)提交，先写进 journal 再生效，满足"决策可见 ⟺ 已记录"，回放能逐字节复现绑定。
   - 槽位在 `window_end` + 结算宽限期之后，且持仓为 0、无挂单时，才允许重新绑定。
   - feed 和 gw 线程按绑定表把 token id 映射到 `InstId`，绑定变化经现有 `Resubscribe` 生效。
   - refdata 只在增加资产或槽位数时重启。
2. **新 crate `venue-polymarket`**，实现 `VenueProtocol`：
   - 行情：market 频道解析到 `MdSink` 的 `l2_snapshot`、`l2_level`、`bbo`、`trade`。`seq_rule` 声明"无序号，重连即全量"；`resync_method` 为重订阅。
   - 下单：`order_channel = Rest`，`needs_per_order_signing() = true`。`encode_rest_signed` 生成 EIP-712 订单并签名。签名用 `k256` 与 keccak，在 gw 线程完成，不进 sequencer。`decode_rest` 解析下单和撤单的应答。
   - 回报：user 频道解析到 `ReportSink::order`。
   - `rate_rules` 按官方限速填写。`dead_man*` 声明不支持，由 OMS 的到期撤单兜底。
3. **类型与能力的增量**(事件 schema 只追加)：
   - `SignAlg::Secp256k1Eip712` 加钱包私钥与 funder 地址凭据，私钥从 Secrets Manager 载入后清零，不进 journal。
   - `GwCmd` 尾部追加 `expire_engine_ns`，对应 GTD。
   - `VenueCaps` 增加 `FeeModel::{Linear(FeePair), BinaryQuadratic{theta}}`，风控与账本按模型计算手续费。
   - 新事件 `Settlement{inst, payout_per_unit}`。持仓按 0 或 1 结转为 USDC，链上 redeem 作为 treasury 的转账对账。
4. **链上操作放在冷路径。** USDC 和 CTF 的授权、redeem、合并(merge)由 `python/treasury` 经 `RecordTransfer` 登记。不启用冻结的 `hft-chain-evm`。
5. **模拟账户。** `hft-sim` 增加二元 token 的撮合规则：价格区间、tick、按结果结算。按"评估用模拟账户"的规则，上线前先在 `sim` 实例上跑。
6. **策略。** 新策略包放 `strategies/<name>`，只依赖 SDK；名字和代码中不出现交易所名，交易所差异全部进 `VenueCaps`。现有跨所策略保留但不启用。

### 里程碑

| 阶段 | 内容 | 退出条件 |
|---|---|---|
| P1 行情 | `venue-polymarket` 的行情部分、槽位绑定、发现进程、`hft-core` 装配 | 与独立录制器 pmrec 同时段的盘口逐条一致，连续 24 小时无未恢复缺口 |
| P2 下单 | EIP-712 签名、REST 下单与撤单、user 频道回报、GTD、`FeeModel` | 签名与官方 SDK 金样一致；`SmokeOrder` 零成交冒烟：远离盘口的 post-only 最小单下单后撤单，回报和对账闭合 |
| P3 结算 | `Settlement` 事件、redeem 对账、余额闸门 | 一个完整结算周期的账本与链上余额一致 |
| P4 模拟 | `hft-sim` 二元撮合、`sim` 实例 | 模拟账户连续运行，回放 `replay-verify` 与 DST 通过 |

## Alternatives considered

- **独立的 Python 执行器(官方 py-clob-client)。** 上手最快，但会形成第二套风控、账本和恢复逻辑，与"一个账户只由一个系统交易"冲突。负责人已否决，可以作为 P2 签名金样的对照实现。
- **每个窗口热更新 refdata。** 需要实现冻结的 refdata 热替换，并且会每 5 分钟改一次宇宙，否决。
- **新增 `MarketKind::Binary`。** 改动面覆盖 exec、risk、ledger 的全部分支。现货语义已经能表达"持有才能卖"，只缺按结果结算，用 `Settlement` 事件补上即可。
- **启用 `hft-chain-evm` 做链上兑付。** 它是冻结组件，范围是 swap 交易；redeem 频率低，放冷路径就够。

## Risks

- **吃单延迟。** Polymarket 对吃单有约 50–150 ms 的延迟，部分边际来自排队位置。eu-west-1 到撮合服务器在 eu-west-2，`latency-budget` 的分段需要为这个 venue 单独标注。
- **签名开销。** secp256k1 每笔签名约几十微秒，必须保证不在 sequencer 线程执行，并测量 gw 线程的 p99。
- **市场发现依赖 Gamma。** Gamma 是中心化 API，发现失败时只能不绑定，不能猜测 token id。槽位用尽时拒绝新窗口并告警。
- **地域与账户。** 爱尔兰(IE)的 API 不受地域限制(只限网页)。直接托管在 eu-west-2 需要 KYC/KYB，属于运营决定，不在本提案范围。
- **范围。** 本提案改变 [ccex-first-scope](2026-10-02-ccex-first-scope.md) 的首要服务范围，需要负责人确认后同步更新 AGENTS.md 和 design 文档。

## Acceptance criteria

- `venue-polymarket` 的 README 和 TESTS 与代码同一提交落地；`cargo xtask ci`、`deps`、`doc-check`、`test-trace` 全部通过。
- 槽位绑定和 `Settlement` 走 journal，`replay-verify` 逐字节一致。
- 各阶段的退出条件按上表完成，并保留证据(录制对比、冒烟回报、对账报告)。
