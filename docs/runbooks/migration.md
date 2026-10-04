---
description: "可选:把 cdex 从旧框架(Python cdex + 现有 Telegram bot)迁到新框架时使用——迁移前检查、切换步骤(旧引擎停止开仓 → 收尾 → 新系统启动并对账入账 → 解除 Recovery → 逐步 Live)、回退(新系统 Drain → 重启旧引擎)"
scope: optional
---

# cdex 迁移(optional)

## 适用范围

- **可选**。触发条件:决定把 cdex(策略 D/E,链上腿)从旧框架迁到新框架([ccex-first-scope](../../.agents/notes/proposed/architecture/2026-10-02-ccex-first-scope.md))。在此之前 cdex 在旧框架上照常运行,本页不适用。
- 新框架的首次上线不是迁移:ccex 策略用新子账户上线,见 [launch](launch.md)。
- 迁移对象:cdex 现有的交易所账户与 BSC 执行钱包、策略 [cdex](../../strategies/cdex/README.md)、链上组件(chain-core 等,均为 optional,迁移时启用)、pipeline 与 ops_bot 的 cdex 插件。cdex 账户与 ccex 账户是不同的子账户,一个账户只绑定一个策略实例([one-account-per-strategy-instance](../../.agents/notes/proposed/architecture/2026-10-02-one-account-per-strategy-instance.md))。
- **规则:同一交易所账户同一时刻只由一个系统交易;迁移是切换,不并行。**
- 现有 collector 保持独立服务研究,不在本页范围。

## 前置条件(迁移前检查)

全部满足才排迁移窗口;结果写进迁移记录。

1. **新框架已上线运行**:至少一个 ccex 实例已按 [launch](launch.md) 进入 D 阶段(正式实盘);上线门槛([testing §8](../testing.md#8-上线门槛))对新框架整体成立。
2. **链上组件的测试**:chain-core 在 BSC 上的测试(SwapGuard 过期、`Unknown` 不推断、≥ 2 独立运营商回执、nonce 槽候选集合)全部通过;`strategies/cdex` 的 DST 场景与金样全部通过。
3. **观察期**:cdex 策略在新框架上的观察期——CEX 腿可用模拟账户;链上腿不在 `hft-sim` 覆盖范围内时,用 Shadow 运行 ≥ 14 天,并与同期旧引擎的信号做离线对照(读双方日志),一致率与分歧原因逐条记录。`replayer verify` 对这段 journal 每夜一致。
4. **实盘验证**(限额由运营在 `RiskParams` 设定,建议保守):二选一——(a)旧引擎**暂停**(停止开仓、确认无挂单)的窗口内,新系统在同一账户上以 Live 运行并收尾;(b)用一个独立的子账户验证。包括开仓、对冲、平仓、链上 swap 与 `TxLifecycle`、对账闸门。窗口结束时新系统 Drain,账户交还旧引擎。
5. **账户与资金**:cdex 账户的 key 已按 [launch](launch.md) A 阶段的要求配置(交易 key 无提现权限 + IP 白名单、`venue_uid` 登记、`ACCOUNT_CFG` 核验);BSC 执行钱包按 [bsc-node 执行钱包运维](bsc-node.md#执行钱包运维) 启用、完成授权、有足够 gas;记下迁移前各所余额与链上钱包余额。
6. **配置**:refdata、`EngineConfig`、`RiskParams` 由 `tools/hft-cfg` 生成并评审;cdex 参数由 pipeline 的 cdex 插件产出,与旧引擎当前的名单、阈值、仓位上限逐项对照(差异报告存档)。
7. **kill 与死手演练**:最近 30 天内完成一次 [kill 演练](incident-response.md#kill-演练),另含 `/kill chain`。
8. **reconciler**:对 cdex 账户的两所(仓位、挂单、余额、现货库存)与 BSC 钱包的对账在观察期内连续运行,报告与真值一致。
9. **运维**:ops_bot 的 cdex 插件覆盖老板常用工作流(`/p /b /pl /account /engine /hedgefix /pipeline` 的语义),老板(Viewer)确认只读面板可用;watchtower 告警通道打通。

## 步骤

1. `[只读]` 选窗口(避开资金费结算前后 ±5 min 与高波动时段),写迁移记录的开头:授权人、时间、两边版本。
2. `[需授权]` **旧引擎停止开仓**:在旧 bot 里关闭开仓,旧引擎只做平仓与对冲收尾。
3. **平仓或确认剩余仓位**:按运营决定二选一——(a)旧引擎把仓位平到 0;(b)保留部分携带仓位,逐个币记录剩余仓位(两腿数量、现货/链上代币余额、入场价)。确认交易所上**没有旧引擎的挂单**、链上没有未终局的旧交易。
4. `[需授权]` **停旧引擎与旧 bot**:停止旧 cdex 的全部服务,确认进程已退出、不再连接交易所与 RPC。从这一刻起账户只属于新系统。
5. `[需授权]` **新系统接入**:按 `hft-cfg` 新增 cdex 账户的 `AccountMeta` 并 `ReplaceRefdata`(Drain + 重启);启用 `chain` 组(`hft-chain-core@bsc`,[deploy](../../deploy/README.md));cdex 实例的期望模式为 `Shadow` 或 `CloseOnly`,账户带 `Recovery` cap。
6. **对账并入账**:等 reconciler 对每个 cdex 账户、每个 venue 与 BSC 钱包各出一组报告;第 3 步保留的剩余仓位作为差异出现——永续仓位与**现货/代币余额**逐项 `/adopt <a> <venue> <条目> <report_id> [entry_px]`(Trader + 确认),预览中的数量与入场价与第 3 步的记录逐项核对;同一币两腿都采纳完之前该 group 保持 `Broken`,不会自动补腿;启动闸门放行前,Broken 的自动有界平仓也处于等待([incident-response A](incident-response.md#a-裸敞口nakedexposure断腿broken)),采纳期间若可能放行闸门,先 `/kill account <a> hard`。非策略资产用 `/baseline` 登记。之后等连续两轮 `Ignore`。
7. `[需授权]` **解除 Recovery**:启动闸门全部通过后,交易员 `/resume recovery <a>`(Trader + 确认),核对有效模式。
8. `[需授权]` **逐步 Live**:`/mode <cdex 实例> <a> live`(Trader + 确认);观察 ≥ 30 min(信号、成交、两腿间隔、裸敞口、对账)后再开下一个实例。
9. 第一天:每 2 h 核对一次 `/p` 与交易所/链上真值;P0/P1 按 [incident-response](incident-response.md) 处理。

## 验收

- 迁移完成后 24 h:无未解释的 P0;reconciler 对两所与 BSC 钱包全部 `Ignore`;第 3 步保留的仓位全部入账且两腿一致;旧系统进程全部停止。
- 迁移后 7 天:同一口径 PnL 与迁移前旧系统同期表现可比,差异有解释;老板日常工作流在 ops_bot 上完成。

## 回退

1. `[需授权]` cdex 实例 `/mode … closeonly` 或全系统 `Drain`(`/shutdown`);chain-core `Drain`(等在途链上交易有结论)。
2. 确认交易所上没有新系统的挂单、链上没有未终局的新交易;记录新系统留下的仓位(以 reconciler 最后一轮报告为准)。
3. `[需授权]` 重新启动旧引擎与旧 bot(旧系统迁移期间未改动),按旧系统的流程核对并接管剩余仓位;新框架上从 refdata 中移除 cdex 账户(或保持其实例为 `Shadow` 且不持 key)。
4. 回退原因写迁移记录;若涉及事故,写 postmortem。修复后重新从"迁移前检查"开始。

## 记录

`ops-log/<date>-cdex-migration.md`:授权人、两边版本、迁移前检查每项的证据链接、迁移前后余额、第 3 步的剩余仓位清单与第 6 步的 `AdoptExchange` 对应关系(交易所单号、`report_id`)、每步 UTC 时间、第一天与第七天的核对结果。
