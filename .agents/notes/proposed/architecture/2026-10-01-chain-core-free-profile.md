# Agent Note: cdex 免费版 = chain-core free profile(公共 RPC,cold tier)

Status: proposed

> **第八轮(2026-10-02)**:取代"free profile 是 core"——chain-core(含 free profile)整体转 optional,触发条件是决定把 cdex 迁到新框架;两种 profile 的设计本身不变;见 [ccex-first-scope](2026-10-02-ccex-first-scope.md)。

## Problem

原设计把自建 BSC 节点、块流与多 builder 直投作为 chain-core 的前提([06 §2](../../../../design/06-链上执行层.md))。用户决定这些付费服务(及 Solana 付费服务)暂缓,cdex 按新框架实现但先用免费资源(第四轮裁决 R4-3),并且**直接做在新框架里**,不对旧 Python cdex 做加固(第七轮裁决 R7-3、R7-4)。现有 Python cdex 的事故并不来自"没有 builder":9/24 的 403 被当作失败、`pending` nonce 不可信、无链上过期导致的迟到成交、0.5 s 轮询回执、单源回执的幻影成交、收到成交后对冲中位 146 ms / p90 30 s——这些是正确性与可回放性问题,不需要付费通道就能修。

## Proposal

第四轮 R4-3、第六轮 A3、第七轮 R7-3(主控记录);实现在 [`hft-chain-evm`](../../../../crates/hft-chain-evm/README.md) 与 [`apps/chain-core`](../../../../apps/chain-core/README.md),策略侧在 `hft-strategies` 的 D(chain-first):

1. **free profile 是 core**,builder profile 是可选扩展(触发条件:包含率与被抢跑成为主要损失、值得付费通道)。两者在**预检、签名、gas 记账、块头来源、证据来源、`Sent` 语义**上不同,共用路由表、onboarding 与 chain journal;由 `send.profile` 选择。
2. chain-core 是 **cold tier** 进程:允许 tokio 与成熟的 RPC 客户端;与 hft-core 同机,经外部 venue 协议([`hft-ext-proto`](../../../../crates/hft-ext-proto/README.md))走共享内存环;策略看到的是 `PoolQuote`/`TxLifecycle` 事件,chain-first 对冲由 `LegLink` 在收到 `IncludedOk` 的同一事件内发出。
3. free profile:多个免费公共 RPC(HTTP)与可用的免费 WS(`newHeads`);端点表、令牌桶、冷却、封禁、降权全部配置化;**同一笔已签名交易并发提交** K 个端点;`Sent` = 至少一个端点接受;回执来源配置 ≥ 3 个**独立运营商**(按封禁/限流域划分,不按 URL),`IncludedOk` 需 ≥ 2 个独立运营商一致;可用独立来源 < 2 持续 N 块 → P0 转人工,不推断;预检用公共 RPC `eth_call`。
4. `TxTracker` 按"nonce 槽 → 候选交易集合"设计;本地 nonce 分配器只在包含/作废后推进。
5. **SwapGuard 必需**:`maxBlock = 发送时块号 + N`(默认 3),公共 mempool 中滞留的交易过期后上链只会 revert;`Dropped` 仍是"终局块号 > `maxBlock` 且 nonce 未被本交易消耗"([tx-onchain-expiry](2026-10-01-tx-onchain-expiry.md))。
6. 公共 mempool 下的 nonce 规则:同 nonce 复用必须提价 ≥ 12.5%;同一意图不提价加速,过了 `maxBlock` 交给合约过期;滞留不去的旧交易用 `Cancel`(0 值自转账)清掉;被替换者判 `DROPPED(NONCE_SUPERSEDED)`。
7. gas 是显式成本:`IncludedReverted` 的 gas 记入发起它的 group;每钱包日 gas 上限、每路由连续 revert 上限,触及即自动非 hard 暂停(平仓放行);`risk_reducing` 意图另设 gas 硬上限,超过 → P0 转人工。
8. 成交量一律是基础资产(代币)数量:Buy 取 `amount_out`,Sell 取 `amount_in`;计价资产数量只进名义与 ledger。
9. 如实区分:改进的是正确性与可回放性;**不改变的是包含速度与被抢跑**,只报告不设目标([latency-budget §4.1](../../../../docs/latency-budget.md))。

## Alternatives considered

**等 builder 通道再做 cdex。** 付费暂缓意味着无限期推迟,而 cdex 的主要损失来自正确性问题,今天就能修;builder profile 是同一进程的另一个 profile,将来是配置切换,不是重写。

**先在旧 Python cdex 内加固,Rust chain-core 以后再做(第六轮方案)。** 规则要写两遍,且旧系统在迁移时整体停用;用户决定直接做在新框架上([cdex-hardening-in-python-first](../../rejected/architecture/2026-10-01-cdex-hardening-in-python-first.md) 已否决)。

**chain-core 做成 hot tier、不用 tokio。** free profile 的延迟由公共 RPC 往返(毫秒级)主导,热化没有收益;成熟的 RPC/WS 客户端都基于 tokio,自写同步客户端只增加缺陷面。builder profile 立项时再评估热化。

**free profile 不用包装合约,只靠 Router `deadline`。** `deadline` 是时间戳,且不覆盖授权、划转与非 Router 路径;公共 mempool 的滞留时间不可控,块号过期是唯一可靠的上界。

**单端点发送、失败后换端点重发。** 串行重试更慢,且容易把传输错误误当失败;并发投递同一哈希天然幂等。

**同一意图过期前提价加速。** 价格变化后 `minOut` 可能失效,每次替换都是新的竞态;以新意图、新 `minOut` 重来更简单,也与"重试必须重新判定"(AIN、JCT)一致。

## Acceptance criteria

- 属性测试与 DST 覆盖端点限速/封禁、公共 mempool 滞留后迟到上链、同 nonce 替换竞争、各 RPC 回执不一致、独立来源不足([testing](../../../../docs/testing.md#34-dst));全部不变量零违反。
- BSC 测试网:SwapGuard 与生命周期(含迟到上链)演练通过;迁移前小额实盘中传输错误被当作失败 0 次、nonce 冲突导致的重复 swap 0 次、过期后成交 0 次、未解释的 `Unknown` 0 次([testing §8](../../../../docs/testing.md#8-上线门槛))。
- Shadow 运行期间与旧 Python cdex 的信号离线对照;每日报表给出包含块数、被抢跑率、revert 率与 gas 合计(前两项预期与旧系统持平)。
- 部署主机上 chain-core 平均 CPU ≤ 0.2 核、常驻内存 ≤ 200 MB(cold tier,不占隔离核)。

## Risks

- 免费端点随时改限额、下线或要求 API key(`rpc.ankr.com/bsc` 已发生);靠端点表配置化、降权与 `SetEndpoint` 维护;独立运营商不足 3 时按规则转人工。
- 公共 mempool 下被抢跑与 revert 是常态,gas 成为显式成本;日 gas 上限兜底。
- 免费 WS 的可靠性未知:无 WS 时回执靠轮询,证据检测变慢(仍优于现状的 0.5 s 轮询 + 单源)。
- 有意放弃:包含速度与排位的改善(需要 builder profile)。
