# Agent Note: 报价可用性按 BBO 连接健康判定,不按最后一次变化的时间

Status: proposed

> **第八轮第四次修订(2026-10-03)**:收窄第 1 条——`BboView` 的连接健康按该 inst 的 BBO 连接成员聚合(每 venue 至多 8 个连接槽,inst 维护成员位图;`conn_up` = 任一成员 up,`conn_live_tsc` = up 成员的最大值),`Views::bbo(i)` 按值返回;BBO 连接在独立的 `feed-bbo` 任务上;规格归 hft-strategy-sdk 与 hft-sequencer;见 [feed-split-and-snap-fetch](2026-10-03-feed-split-and-snap-fetch.md)。

## Problem

修订后的规格对第二腿、风险递减单与自动平仓要求"报价年龄 ≤ 阈值",年龄按该 inst 最后一次变化计算(hft-oms `price_hedge`、hft-risk 链接腿 #5 的 `hedge_quote_max_age_ms` 500、hft-exec `quote_max_age_ms` 1 000)。BBO 流只在变化时推送,安静行情下盘口 0.5 s 不变很常见 → `StaleQuote` → 3 s 后 Broken → 自动平仓每片也要求新鲜报价 → Failed;`max_mid_age_ms` 没有默认值;这还与 hft-strat-lib"安静的 inst 几秒无推送不应判为过期"矛盾([评审](../../../../design/_work/review-2026-10-03-M0b独立评审.md) P0-1)。旧判定下"没有变化"与"连接已死"无法区分。

## Proposal

`BboView` 归 [hft-strategy-sdk](../../../../crates/hft-strategy-sdk/README.md);ping 与连接状态归 [hft-venue](../../../../crates/hft-venue/README.md);使用点归 [hft-risk](../../../../crates/hft-risk/README.md) #5、[hft-oms](../../../../crates/hft-oms/README.md) `price_hedge`、[hft-exec](../../../../crates/hft-exec/README.md);DST 用例归 [testing](../../../../docs/testing.md)。

1. `BboView` 增加 `conn_live_tsc`(该 BBO 连接最近一次收到任何帧——数据、pong、心跳——的时刻)与 `resync: bool`。
2. hft-venue 对每条 BBO 连接每 `bbo_ping_ms`(1 000)发一次 WS ping,保证安静时也有回帧。
3. **判定**:连接 Live、不在重同步、`now − conn_live_tsc ≤ conn_liveness_ms`(2 000);满足时该连接上的最新报价有效,无变化即有效。
4. 对冲腿、风险递减单、自动平仓、L3 减仓、链接腿 #5 一律用此判定;`max_mid_age_ms` 退役。
5. DST 必过用例:第二腿盘口 10 s 不变时对冲照常发出,不进 Broken。

## Alternatives considered

**放宽年龄阈值(例如 5 s)。** 只是把卡死点后移,真实断流时又发现得更晚;安静与断流仍不可区分。

**决策前用 REST 再查一次盘口。** 热路径上不可接受的延迟与限速消耗。

**依赖交易所的应用层心跳。** 各所口径不同,部分 BBO 流没有心跳(待核实);WS ping 在三所都可用,统一在 hft-venue 实现。

## Acceptance criteria

- DST:第二腿盘口 10 s 不变 → 对冲发出,无 `StaleQuote`、无 Broken;BBO 连接 > `conn_liveness_ms` 没有任何帧 → 判不可用,对冲走既有失败路径;重同步期间判不可用。
- 金样:三所的 pong 帧都更新 `conn_live_tsc`。
- 全仓规格中没有 `max_mid_age_ms`(`check_hft.py` 机检)。

## Risks

- "连接健康但该 inst 已停止交易"不会被判过期:由交易状态(停牌、下架)与价格带检查兜底。
- 交易所撮合停顿而连接健康时报价冻结:损失由价格带与 `rr_max_cross` 上限约束。
- ping 计入各所每连接的入向消息限额(Binance 每连接每秒 ≤ 10 条,1 s 一次在额度内;其他所待核实)。
