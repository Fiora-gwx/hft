# Agent Note: 回放是研究与参数筛选工具(上线门槛不变)

Status: proposed

> **重构修订(2026-10-29,负责人决定)**:本 Note 收窄 [sim-accounts-not-backtest](2026-10-02-sim-accounts-not-backtest.md) 的第 6 条("重演只排障")与其"历史回测 / 假设回放 / 参数扫描"的否决;第 7 条(门槛 ⑦)与模拟账户的其余规则不变。设计依据:重构设计 2026-10-29 §1.2 D2/D3、§5、§9 第 1 条(仓外 `E:\trade\handoff\hft-refactor-design-2026-10-29.md`)。

## Problem

基线把回放限定为确定性验证与排障([08 §5](../../../../design/08-数据面与回放.md)、[07 §9](../../../../design/07-策略层.md#9-运行模式与模拟账户))。实际研究需要在录下的 journal 上比较参数变体:Polymarket 二元做市的每个新规则都先在仓外写一份 Python 整数移植(`bm_port.py`)与引擎级回放(`bm_sim.py`),做到 95–99.9 % 决策一致后再写 Rust,一致性本身成了持续成本,也出现过 Python 与 Rust 语义漂移使结论反转的情况。仓内的 `replayer` 只有 `verify`;`sim --ab` 只有规格、且只许两组参数、只出差值。

## Proposal

1. **用途**:`replayer research`(新增,M1)把 seq journal 经同一 `Sequencer` 与同一 Rust 策略、进程内 `VenueSim` 撮合,输出决策、订单、成交、特征与窗口 Parquet,支持 N 个参数变体共享一次解析;`replayer parity` 用日志里的真实回报重算决策并与日志决策比对(替代 `bm_port.py follow`)。它们是**研究与参数筛选工具**。
2. **输出必须带口径**:每次运行写 `run.json`,含代码 SHA、各变体 `params_sha`、校准表 `calib_sha`、journal 范围与确定性 `state_hash`;报告给出与实盘的对账误差(重构设计 §5.2 口径),以及 ≥ 5 个延迟随机种子的均值、标准差与变体间配对差的 t 值;同一窗口在各变体间用相同随机数(common random numbers)。
3. **上线门槛不变**:回放结论不能代替模拟账户 / Shadow / 门槛 ⑦([testing §8](../../../../docs/testing.md#8-上线门槛));策略版本仍按原规则在模拟账户与 Shadow 上计时。
4. **确定性**:研究回放不读系统时钟;模拟回报与日志 Input 按引擎时间全序交织,`(due, sim_seq)` 稳定排序;随机数只来自 `(seed, variant, account, window)`。`replayer verify` 不变,仍是 CI 金标准。

## Alternatives considered

**维持"不做回放评估"。** 研究照样会做,只是在仓外用第二份实现做,一致性无法机检,结论随移植漂移。

**只扩展 `sim --ab`(两组参数、只出差值)。** 不够:研究一次比较十几到几十组变体,还需要逐单与逐窗的明细与对账。

**仓外 Python 回放继续,仓内只提供解码器。** 策略仍要写两遍;速度受 CPython/PyPy 限制。

## Acceptance criteria

- `replayer parity` 对固定回归 journal 集:binary-maker 当前代码决策一致率 ≥ 99.9 %,差异全部可归因(M1)。
- `replayer research` 与 `bm_sim.py` 在同参数、同固定延迟下逐窗相关 ≥ 0.95、合计差 ≤ 5 %(M1);与实盘对账每窗 |Δ| ≤ 0.3 USDC、t < 1(M2)。
- 同一 journal、同一种子两次运行的 Parquet 与 `state_hash` 相同。

## Risks

- 回放再精确也不是实盘:校准表可能过拟合某段时期。缓解:报告强制列出校准表日期范围与哈希;门槛不变。
- 一次延迟抽样的噪声与要分辨的效应同量级(研究侧:同一策略 832 窗合计 +$163 vs −$195):缓解:多种子 + 配对比较是默认输出,不是可选项。
