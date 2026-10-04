# Agent Note: 交付方式是建成 → 测试 → 一次性迁移;范围按 core / optional 划分

Status: proposed

> **第八轮(2026-10-02)**:部分取代——交付模型(一次性迁移 cdex、Shadow ≥ 14 天 + 与旧引擎离线对照、core 含链上与策略 D/E)由 [ccex-first-scope](../architecture/2026-10-02-ccex-first-scope.md)、[sim-accounts-not-backtest](../architecture/2026-10-02-sim-accounts-not-backtest.md) 取代:core = 首次上线(新子账户上线 ccex 策略)必须具备的组件,门槛见 [testing §8 上线门槛](../../../../docs/testing.md#8-上线门槛)。第 2 条(无并行期、无过渡机制)与第 3 条的 `scope` 机制继续有效。

> **第八轮第四次修订(2026-10-03)**:第 5 条与风险中的"小额实盘"不再适用(没有小额实盘阶段);上线前的验证见 [no-small-live](../architecture/2026-10-03-no-small-live.md) 与 [shadow-before-live](2026-10-03-shadow-before-live.md)。

## Problem

此前的规格按阶段编号(从 0 到 4)组织交付:先在现有系统上做 AZ 搬迁与链上加固,新系统按阶段逐个上线,与旧 Python 引擎并行交易,靠第二实例(灰度)、共享账户的 ownership 表、过渡接口与"影子/实盘意图一致率"完成切换。这带来大量只为过渡期存在的机制,也让每份规格都要回答"这个排在哪个阶段"。用户决定(第七轮):现有系统照常运行、不做适配;新系统做好、测试完就直接迁移;cdex 免费版直接做在新框架上。

## Proposal

第七轮裁决(2026-10-01 用户决定,主控记录)R7-1–R7-4:

1. **交付模型**:现有系统(Python cdex、collector、现有 Telegram bot)照常运行到迁移。本系统 `scope: core` 的组件全部建成并通过[迁移前门槛](../../../../docs/testing.md#8-上线门槛)后,一次性切换:旧引擎停止开仓并收尾 → 停旧引擎 → 新系统启动、对账,剩余仓位用 `AdoptExchange` 入账 → 解除 `Recovery` → 逐策略 `SetMode Live`。回滚 = 新系统 Drain → 重启旧引擎。步骤见 [runbooks/migration](../../../../docs/runbooks/migration.md)。
2. **没有**新旧并行交易期、共享账户与 ownership 机制、过渡接口、灰度第二实例;同一交易所账户同一时刻只由一个系统交易([one-exchange-account-one-system](../architecture/2026-10-01-one-exchange-account-one-system.md))。新版本验证 = 回放 verify + Shadow 运行。
3. **范围**:README frontmatter `scope: core | optional` 是唯一来源,`doc-check` 据此生成根 README 组件表;正文不写"阶段 N",optional 写成"可选扩展:……(触发条件)"。
   - core:Binance USDT-M、Gate;BSC 经 chain-core free profile([chain-core-free-profile](../architecture/2026-10-01-chain-core-free-profile.md));策略 C、D、E,A 以 Shadow 运行直到运营决定打开;pipeline、ops_bot(覆盖现有 Telegram 工作流)、reconciler(两所 + BSC 钱包)、kill-switch、watchtower、journal-archiver、replayer verify、hft-py(reader + ledger);hft-testkit + 交易所测试网。
   - optional:其余 CEX、Hyperliquid、Binance Spot SBE、策略 B;builder profile、块流、自建节点、L0 + revm、hft-amm、chain-grpc-relay;Solana;metal/x86 与 Onload;完整 hft-sim 与假设回放;wire 级 mock-exchange;RSS 源端口、TX 时间戳、TLS 预热、L2 双路合并;treasury、research。
4. **沿用的依赖闭合决定**(第六轮 A2):refdata / `EngineConfig` / `RiskParams` 由 [`hft-cfg`](../../../../tools/README.md#hft-cfg) 编码,每种 blob 只有一个 Rust 编码器;`StrategyParams` 由 pipeline 产出,人工经 `/pl set` 策略文件或 CLI 确认下发;DST 用 `hft-testkit` 的最小确定性撮合器;`hft-py` 按 feature 拆分。
5. **测试即迁移门槛**:单元/属性/金样/fuzz/DST、交易所测试网、现有服务器上的长时间 Shadow 运行、Shadow 信号与旧引擎信号的**离线**对照、小额实盘([testing §8](../../../../docs/testing.md#8-上线门槛))。

## Alternatives considered

**分阶段交付、新旧并行(第三至六轮的路线图)。** 原理由是"按测得收益排投入":先在旧系统上搬 AZ、先修链上腿,让收益最早落地;A 类先影子积累 LeadLag 证据。代价是一整套只为过渡期存在的机制(共享账户隔离、灰度第二实例、过渡接口、旧系统加固),且所有规格都被阶段号切碎。用户选择让旧系统照常运行到迁移,新系统一次做完;"A 类先 Shadow"保留为运行配置,链上腿的修复直接在新框架里做。

**在旧 Python cdex 内先加固链上规则(第六轮 A3)。** 规则写两遍,旧系统迁移时整体停用;已否决([cdex-hardening-in-python-first](../../rejected/architecture/2026-10-01-cdex-hardening-in-python-first.md))。

**灰度第二实例做版本验证。** 与共享账户同一类风险;回放 verify + Shadow 运行覆盖同样的问题。

**保留 `phase` 字段但只作排序。** 会继续诱导"阶段 N 再做"的写法;core/optional 加触发条件更直接地回答"迁移时要不要有它"。

## Acceptance criteria

- 规格仓库中除设计历史外不再出现"阶段 N"式措辞;每个组件 README 的 frontmatter 有 `scope`,根 README 组件表与之一致(`doc-check`)。
- [testing §8](../../../../docs/testing.md#8-上线门槛) 的全部门槛通过后才执行迁移;迁移演练与回滚演练各一次。
- 迁移后旧引擎不再持有任何交易 key 或连接。

## Risks

- 一次性迁移的风险集中在切换当天:由迁移前门槛(长时间 Shadow、离线信号对照、小额实盘)与回滚步骤兜底。
- 没有分阶段的早期收益:AZ 搬迁、链上修复都要等迁移才生效;现有系统的在线风险作为待决定事项保留(用户知情)。
- core 范围较大(含 chain-core free profile、pipeline、ops_bot 完整):构建顺序按依赖排列(设计书的交付与迁移计划),不按日历承诺。
