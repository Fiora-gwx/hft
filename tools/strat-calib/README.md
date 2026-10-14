---
description: "离线读取历史 BBO 与资金费,比较两策略的费用、机会、容量及参数情景,生成中文报告"
kind: python-package
tier: cold
scope: optional
status: impl
---

# strat-calib

## 概要

负责人需要用已有采样行情筛选研究参数时使用。Python 命令读取本地 Parquet/CSV/JSON,输出中文 Markdown、JSON 与成交/资金费序列 CSV。Rust crate 只承载共享夹具对照测试,不提供生产接口。工具没有联网、账户访问、发单或配置下发能力,不替代模拟账户、journal 确定性回放与上线门槛。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

安装 `python -m pip install ./tools/strat-calib`。CLI 签名为 `strat-calib --quotes <parquet> --config <json> --output <directory> [--funding-daily <csv>] [--funding-events <csv>] [--report <markdown>]`。输入错误退出 2,成功退出 0。`--report` 可将报告放在仓库外;原始数据与结果不入库。无自动联网补数。

`example.json` 是明确的研究假设,费率不是交易所当前费率声明。费率按 `exchange:market` 配置,可用完整 `exchange:market:symbol` 覆盖;单位 bps,负费率按零计收入。策略参数名称、整数单位与实际参数一致,权威定义归策略 README。`taker` 的研究初值启用、min_edge=2 bps、每腿 slip=1 bps、单笔 100/持仓 500;与未配置时禁用的生产默认不同。carry 使用启用的暂定参数。配置缺少费率、参数越界、未知字段会拒绝加载。

`metadata` 按完整 instrument key 映射 `px_exp/qty_exp/tick/lot/min_qty/max_qty/min_notional`,单位与 InstHot 相同。缺失时必须显式提供 `assumed_metadata`,报告标记假设;不得将推测步长用于生产。`capital_quote` 与 `leverage_x100` 按交易所配置,缺失资金/杠杆阻止 carry 开仓。`initial_position_qty` 是按完整 instrument key 的人类单位库存,默认零,现货不可裸卖。`symbols` 是 canonical_id 列表,`pair_allowlist` 是 `leg0|leg1` 完整键列表,不配置则枚举跨所组合;有永续时第 0 腿为永续。相同交易所不配对。

Parquet 必需列为 `exchange,market,symbol,canonical_id,sec,last_ns,bid,ask,bid_base,ask_base`。market 仅为 `spot/linear_perpetual`,sec 是 UTC Unix 秒,last_ns 是该秒内最后观察的 UTC 纳秒,深度为基础资产数量而非合约张数。重复 instrument/second、空列、错位时间戳拒绝处理。只 join 同一秒的双方记录,不填充。`max_price_ratio`（示例 1.1）排除中位数倍率异常组合与单行异常;这只是数据质量检查,不是合约等价性证明。

`--funding-daily` 接受旧系统的 `exchange,symbol,settlements,day_bps,per_period_bps` CSV,只使用明确的日费率合计,不从 settlements 推算结算周期。`funding.json` 的同类汇总不需要重复输入。每日净差和年化仅是后验筛选,没有可伪装为预测的时间序列或入退场。`carry_horizons_days` 给出恒定费率情景;示例为 1/3/7 天。

`--funding-events` 是真实历史预测/结算记录,CSV 列如下。Rate 为 1e-9,时间为 UTC 纳秒;不自动从日汇总生成记录。

| 列 | prediction | settlement |
|---|---|---|
| inst | 完整 instrument key | 完整 instrument key |
| kind | prediction | settlement |
| ts_ns | 预测实际可见时刻 | 实际结算时刻 |
| rate | 当时预测费率 | 实际结算费率 |
| interval_s | 正的结算周期 | 可为 0 |
| next_settle_ns | 大于观察时刻 | 可为 0 |
| mark_price | 空 | 实际结算标记价格,人类单位 |

同 inst/kind/ts_ns 重复则拒绝。预测按可见时刻更新,实际结算只记账不参与信号,同刻先结算再下单。资金费序列记录预测更新时的净年化和新鲜度。缺少任一永续腿预测时不模拟 carry,不会把缺失当成零。缺失实际结算记录不能证明零成本,输出 `funding_complete=false`。

输出 `summary.json` 包含参数、数据/源文件 SHA-256、缺陷/排除计数、方向分位数、阈值次数、费率敏感性及两策略结果;`trades.csv` 是假设成交,`funding_series.csv` 是预测事件序列,`report.md` 是中文说明及每标的研究范围。每个 pair 为独立钱包情景,资本份额 shares=1,不能把重叠 pair 的收益相加。`threshold_grid_bps` 给出严格大于阈值的采样次数;实际信号的等于阈值也可触发。`fee_scenarios_bps` 是两腿相同费率的展示敏感性,不是实际账户档位。

## 实现说明

`core.py` 使用整数复制真实策略的净边、限价、共同 lot 和 sizing。`tests/fixtures/signals.json` 同时由 Python 与 Rust 测试读取;Rust 对照直接调用 cross-taker 信号、portfolio-carry 年化,并驱动真实 carry Strategy 验证资本/杠杆与目标数量。真实参数与进程模型的归属见延伸阅读。

`data.py` 按标的一次只读一个过滤后的 Arrow 表,使用单线程解码。`simulation.py` 在同秒记录的较晚 last_ns 计算机会,只在保留观察点评估,不制造秒内事件。套利选可行方向中净边最高者,平手买 leg 0;保留冷却和实际模拟持仓,遇 cap 停止,双腿减仓不穿零。现货库存来自显式初始库存及模拟成交。

carry 保留已发目标并按顶档深度切片,预测过期时持有已对冲仓位,收益衰减或禁用时退出,结算前窗口与 retry 生效。数量受静态资金、杠杆事实假设、lot 与最小订单约束。跨事件变化的保证金、引擎风险价格带和市场状态不在模型中;使用 ask 代替未知 mark 计算资金上限。模拟成交固定为两腿 IOC 在本次限价同时全部成交,费用按计价币保守向上取整。

开仓现金流不是利润。结果单列成交现金、尚存仓位和期末立即平仓估值,期末再扣两腿费用和 slip。套利持仓资金费未计;carry 只计显式实际结算与 borrow_apr 持有成本。每日 carry 年化扣往返两腿费用、四次 max_cross 和借贷/机会成本,分母为单腿名义金额;双钱包全额备资的资本收益通常约减半。方向、最小订单等决策用整数;分位数/敏感性展示用 float64,候选订单再过整数信号。

## 延迟与资源契约

离线 cold 工具,无热路径延迟约束。按 canonical_id 过滤读取、不并行解码、不下载数据;内存上限由单标的样本与输出成交数决定,不承诺任意大数据恒定内存。原始大数据保留在仓库外。Rust 对照测试只构建本工具包,不运行 workspace 构建;本地遵守操作者的 memguard 与可用提交内存规则。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

Python 3.12+、NumPy、pandas、PyArrow;测试 pytest。Rust 测试壳无运行依赖,dev 依赖两策略、SDK testing、基础类型/refdata/event 和 serde_json。它不在 hot crate 或生产制品依赖中。

## 延伸阅读

- [套利策略规格](../../strategies/cross-taker/README.md)
- [资金费策略规格](../../strategies/portfolio-carry/README.md)
- [journal 回放](../../apps/replayer/README.md)与[模拟账户](../../crates/hft-sim/README.md)

## 已知限制与后续工作

旧系统逐秒行情不是 hft journal,没有账户、连接、时钟可信状态、订单回报、参数事件及完整 refdata,不能送入确定性验证路径并声称重现了当时决策。Python 模型不模拟 Unknown、断腿/Reverse、失败 halt、清算、公告和风险干预。`max_hedge_failures` 被记录但无拒单事件时不触发,不提供真实双腿成功率。

同秒内顺序与未来 IOC 的成交可得性未知;健康安静报价不等于失效,但本数据无法恢复连接健康。期末平仓估值忽略深度不足;现货按计价币扣费不能复现基础币收费 dust。真实 tick/lot、合约倍率、实际账户费用/资金/杠杆、逐次预测和结算必须补齐,结果才可用于更细的模拟账户参数验证。参数推荐是单段样本内筛选,无样本外验证,不授权上线。
