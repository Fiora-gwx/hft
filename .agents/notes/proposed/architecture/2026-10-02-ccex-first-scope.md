# Agent Note: 新框架首要服务 CEX-CEX;链上组件转 optional;交付 = 新子账户上线

Status: proposed

> **第八轮补充(2026-10-03)**:收窄——不做同一交易所的现货 × 永续(R8-1 的"同所现货 × 永续可接入"取消);CI 只用免费的 GitHub 托管 runner,长时间门槛改为累计量;统一升级后模拟在独立的 `sim` 实例运行;见 [no-same-exchange-spot-perp](2026-10-03-no-same-exchange-spot-perp.md)、[ci-free-hosted](2026-10-03-ci-free-hosted.md)、[sim-only-instance](2026-10-03-sim-only-instance.md)。

## Problem

第七轮把交付定为"建成 → 测试 → 一次性迁移现有 cdex"([build-test-migrate](../process/2026-10-01-build-test-migrate.md)),core 因此包含 chain-core free profile、链上腿、策略 D/E 与覆盖 cdex 全部 Telegram 工作流的 ops_bot。主策略适配评审([review 2026-10-02](../../../../design/_work/review-2026-10-02-主策略适配与执行正确性.md) §0、§5)指出:后续主要交易是 CEX-CEX 组合携带(老板的 portfolio-arb 雏形),规格里没有承载它的接口;core 估计 12.5–18 万行 Rust,把 ccex 策略的上线绑在链上全栈与一次性迁移上。cdex 在旧框架上照常运行,迁移不急([round8-decisions](../../../../design/_work/round8-decisions.md) R8-1)。

## Proposal

1. **scope 语义**:`core` = 首次上线(新子账户上线 ccex 策略)必须具备、必须通过[上线门槛](../../../../docs/testing.md#8-上线门槛)的组件;`optional` = 可选扩展,写触发条件,不写周次。
2. **首批交易所**:Binance、OKX、Bitget,各接现货 + USDT 本位永续;Binance 现货 SBE、Gate、Bybit、KuCoin、Hyperliquid 为 optional(R8-6 第 5 条)。
3. **链上全部 optional**:`apps/chain-core`、`chain-grpc-relay`、`hft-chain-evm`、`hft-ext-proto`、`hft-amm`、`hft-chain-solana`、`contracts/`、策略 D/E(`strategies/cdex`)。规格保留;外部 venue 协议的接口与事件编号段不删。触发条件:决定把 cdex 迁到新框架。
4. **首个策略**:组合携带(永续空 × 现货多,[`strategies/portfolio-carry`](../../../../strategies/portfolio-carry/README.md));方向只做空永续、多现货。跨所吃单(原 A)、挂单 + 对冲(原 B)为 optional。
5. **交付**:按里程碑 M0–M7([11 章](../../../../design/11-实施路线图.md))。开发与测试在本地(只写代码)+ CI + 现有服务器 `dev2`(功能与正确性,受 dev2 护栏约束,与 cdex 正式账户共机);"大体没问题后"在 M7 统一升级到 `c8`(或更高),延迟验收只在升级后。
6. **上线 = 新子账户**:ccex 策略用新开的子账户直接上线([runbooks/launch](../../../../docs/runbooks/launch.md)),不接管旧仓位;[migration](../../../../docs/runbooks/migration.md) 降为 optional(cdex 迁移时用)。
7. **延迟优先不变**(R8-2):忙等绑核、RAM journal + persist、帧模板、热备连接、conn-builder、BBO 独立连接仍是 core 设计。

## Alternatives considered

**维持第七轮"一次性迁移 cdex"作为首要交付。** cdex 在旧框架上能跑,迁移收益主要是正确性;而首要业务是 ccex,把它排在链上全栈之后没有理由(R8-1)。

**按评审 §5 把微秒级机制移到 optional,先做"够用"的 carry 引擎。** 用户裁决总目标仍是延迟优先(R8-2),这些机制保留为 core 设计;只是延迟验收推迟到 M7 的主机升级之后。

**删除链上规格。** 将来迁移 cdex 时要重做设计与评审;保留为 optional 的代价只是不维护,接口与编号段保持兼容即可。

**M3 开始前再定首批交易所。** 用户已定 Binance + OKX + Bitget(R8-6),规格按它们写金样与能力表。

## Acceptance criteria

- `check_hft.py` 通过:每个组件 README 的 `scope` 与 [11 章](../../../../design/11-实施路线图.md) 一致;链上组件全部 `optional`;`launch.md` 存在、`migration.md` 标 optional。
- 首次上线时主机上不安装 `hft-chain.target` 下的任何单元,上线门槛 ①–⑨、⑪ 全部通过。
- 规格中不再出现按周排期与"迁移前门槛"的说法。

## Risks

- optional 规格长期不被实现会腐化:触发时先做一次针对性评审再实现。
- dev2 与 cdex 正式账户共机:由 dev2 护栏兜底([deploy](../../../../deploy/README.md));cdex 优先。
- 有意放弃:cdex 在新框架上获得的正确性改进(9/24 类事故的结构性修复)推迟到 cdex 迁移时;旧 cdex 的已知缺陷继续存在(用户知情)。
