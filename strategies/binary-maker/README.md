---
description: "binary-maker 策略(kind_id 7):二元结果代币(每个时间窗口一对 Up/Down)的挂单策略。参考市场的实现波动足够高、窗口离结束还远时,在当前窗口 Up 与 Down 两个代币的买一价各挂一张 post-only 买单;价位吃空时留守,被抢先、参考价把公允价推向不利一侧或超过对价上限时撤单"
kind: crate
tier: hot
scope: core
status: impl
---

# binary-maker

## 概要

`binary-maker` 是二元结果代币的挂单策略(kind_id 7,登记在 [registry](../registry/README.md))。市场按固定长度的时间窗口滚动,每个窗口有一对结果代币(Up、Down),结算时赢的代币每单位兑付 1、输的兑付 0;代币 inst 是运行期绑定的槽位(绑定与交割由发现进程经 `BindSlot`/`Settle` 完成,[control-plane](../../docs/subsystems/control-plane.md#命令集))。规则来自对 2026-08 盘口与成交的事件级挂单回放(仓库外研究报告,2026-10-20):吃单没有正边际;排在买一的挂单只在参考市场的波动率 σ ≥ 0.5–0.7 bp/√s 时每份有 +0.1–0.2 美分,窗口最后一分钟、剩余标准差 < 1 bp 时为负;参考市场 ≥ 1σ 的跳动后 0.15 s 内的成交是有信息的(对挂单不利);凑成的 Up+Down 对成本超过 1 是持仓到结算的主要亏损(2026-10-21 模拟盘诊断)。本策略只挂 post-only 买单、不吃单、不对冲,持仓到窗口结算。

## 目录

- [使用本组件](#使用本组件)
  - [group 形态](#group-形态)
  - [参数](#参数)
  - [订阅](#订阅)
  - [波动率](#波动率)
  - [特征](#特征)
  - [规则](#规则)
  - [Note 码表与报表字段](#note-码表与报表字段)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### group 形态

全部 group 都是单腿:前 `2 × pairs` 个是槽位(按顺序为第 0 对的 Up、Down,第 1 对的 Up、Down,……),都在同一个 venue;其后是一到两个参考市场(不在槽位 venue 上,只读,不下单):第一个是公允价参考,可选的第二个是 v37 快参考保护(`cb_*`)读的参考——有它时保护读它的 BBO(中间价 × 2,按该 inst 的价格单位;规则只用相对变化),并忽略 InjectWarmup local_kind 3 的旁路样本;没有时照旧读旁路样本。第二参考还可用于研究参数 `ref_cb_w`(schema v38,缺省 0 = 关):公允价及其锚点读的参考中间价改为第一参考中间价 + `ref_cb_w` × (第二参考中间价折成第一参考单位 − 第一参考中间价),单位比值取两者中间价之比的滑动均值(每个第一参考报价 1/64);第二参考 5 s 未更新或尚无比值时退回第一参考自己的中间价;结算模块、波动率与方向分数仍只读第一参考。参考组数 = 表尾不在槽位 venue 上的连续 group 数(至少按 1 计)。`check` 的 `GroupShape` 码:1 = 参考组多于 2,或槽位数不是 ≥ 2 的偶数(只有一个参考时即 group 总数不是 ≥ 3 的奇数);2 = 某 group 不是单腿;3 = 槽位腿不在第一个槽位的 venue 上,或参考腿在;4 = 腿的 inst 不在 refdata。开始时刻为 S 的窗口用第 (S / `window_s`) mod `pairs` 对槽位,与发现进程的槽位规则一致(`bind_slots.py --slots pairs`)。

**辅助组(schema v41 `ref_aux`)**:参考组之后可再有至多三个辅助组,按 `ref_aux` 的位依次排列(位 2 = Coinbase 永续参考,schema v45:只订阅 `BBO`,必须在永续 venue 上,是规则 15 的触发源 `lead_src` 位 8,其余规则不读):位 0 = 永续参考(订阅 `BBO | BOOK | TRADE`,只为把永续行情记进 journal,规则不读),位 1 = Chainlink 组(订阅 `INDEX | MARK`:`Index` = Chainlink 价格样本,`Mark` = 官方 60 s TWAP,样本时间取事件的 `ex_ts_ns`)。辅助组的事件只读取上一事件的下单结果、写入 Chainlink 环,不跑特征也不跑规则:有无辅助组,其余事件序列相同则决定逐位相同(BMK-051.3)。参考组数改为表尾、辅助组之前不在槽位 venue 上的连续 group 数;`check`:`ref_aux` 的辅助组数从表尾扣除后照原规则核对,辅助组也须不在槽位 venue 上,位 0 的组所在 venue 的 `VenueCaps.market` 须是永续(`GroupShape` 码 5 = `shape::AUX`)。有 Chainlink 组时,`InjectWarmup` local_kind 1 的旁路样本被忽略;价格换算:1e-8 USD = 价格 × 10^(8 + 该 inst 的 px_exp)(px_exp 不在 −8..=0 内时不用原生样本)。`ref_aux` = 0(缺省)时与 v38 相同。

### 参数

| 字段 | 单位 | 默认 | 说明 | relax |
|---|---|---|---|---|
| `enabled` | Bool | 0 | 关闭时不挂单,已挂的撤掉;人工设定 | Up |
| `window_s` | Count | 300 | 窗口长度(秒) | None |
| `clip_shares` | Count | 10 | 每张挂单的份数 | Up |
| `inv_max_shares` | Count | 100 | 当前窗口 Up 减 Down 的净份数加上一张挂单(`clip_shares`)会超过它时,多的一侧不再挂单 | Up |
| `p_min` / `p_max` | Rate | 0.10 / 0.90 | 只在买一价落在 [p_min, p_max](占 1 的比例)内时挂单 | Down / Up |
| `sigma_min` | Rate | 0.5 bp | 隐含波动率(每 √秒)下限 | Down |
| `sigma_scale` | Rate | 1.39 | 隐含波动率 / 参考市场 1 秒实现波动率(研究里 5 分钟方差比 1.6–2.2 的平方根量级,隐含 σ ≈ 1.39 × 参考市场 30 分钟实现 σ) | None |
| `min_rem_sd` | Rate | 1 bp | 剩余标准差(隐含 σ × √剩余秒数)下限 | Down |
| `fv_cancel` | Rate | 0.005 | 挂单后参考价变动换算成的公允价变化(占 1 的比例)达到它时撤掉该腿(规则 5);0 = 关闭 | Up |
| `fv_cool_ms` | DurationMs | 2000 | 公允价撤单之后该腿不挂单的时长 | Down |
| `stop_before_ms` | DurationMs | 60 000 | 窗口最后这段时间不挂单 | Down |
| `vol_window_s` | Count | 1800 | 实现波动率用的 1 秒收益个数(30 分钟) | None |
| `vol_min_samples` | Count | 300 | 至少这么多个样本才挂单 | Down |
| `requote_ms` | DurationMs | 200 | 同一腿两次改价(撤单或挂单)的最短间隔(引擎时间) | Down |
| `max_rest_ms` | DurationMs | 360 000 | 每张挂单的最长挂单时长(OMS 到期发 `Aged`) | Up |
| `pair_margin` | Rate | 0.01 | 会凑成对的一侧买单的价格上限:1 − 另一侧平均成本 − `pair_margin`(规则 4);范围 −0.10 – 1,负值允许以每对至多 −`pair_margin` 的亏损凑成对 | Down |
| `nochase_ms` | DurationMs | 0(关) | 因被抢先撤单后,该腿这段时间内不再挂单(不追价,规则 6) | Down |
| `streak_k` | Count | 0(关) | 连续成交暂停的成交笔数;≥ 2 时生效(规则 7) | Down |
| `streak_window_ms` | DurationMs | 10 000 | 最近 `streak_k` 笔成交须落在这段时间内 | Down |
| `streak_pause_ms` | DurationMs | 10 000 | 暂停持续到该腿最后一笔成交之后这么久 | Down |
| `max_window_cost` | Notional | 0(无上限) | 当前窗口两腿已买入成本(份数 × 平均成本之和)达到它时不再新挂单(规则 7) | Up |
| `stuck_reset_ms` | DurationMs | 5 000(暂定) | 撤单发出后这么久仍没有终态回报:再撤一次并视为没有挂单(规则 8);范围 1 000 – 600 000;schema v4 加入(`since` 4,v3 参数体取默认值) | Down |
| `fv_limit` | Bool | 0(关) | 开启时买单价再受本策略自己的公允价上限约束(规则 9);关闭时与 v5 相同 | Down |
| `fv_anchor_ms` | DurationMs | 1 000 | 公允价锚定 Up 中间价的半衰期(规则 9);范围 100 – 600 000 | None |
| `fv_edge` | Rate | 0 | 买单价至多为公允价 − `fv_edge`;范围 −0.10 – 0.10 | Down |
| `fv_hyst` | Rate | 0.005 | 挂单价高于公允价上限超过它时撤单;范围 0 – 0.10 | Up |
| `inv_skew` | Rate | 0.03 | 公允价上限按 (U − D) / `inv_max_shares` 每单位平移的量:多的一侧降低、少的一侧提高;范围 0 – 0.5 | None |
| `pred_k` | Rate | 0.2 | 方向预测折成公允价漂移的倍数:漂移 = `pred_k` × 预测 × 参考 σ × √30 秒;0 = 不用预测;范围 0 – 2 | None |
| `pred_w_imb` | Rate | 1.37249 | 预测里参考市场买一卖一数量失衡的权重;范围 −10 – 10 | None |
| `pred_w_ret` | Rate | 0.12129 | 预测里最近 5 个 1 秒收益之和 / (σ √5) 的权重;范围 −10 – 10 | None |
| `pred_w_ser` | Rate | 0.18720 | 预测里最近 15 个 1 秒收益之和 / 其绝对值之和(带方向的效率比)的权重;范围 −10 – 10 | None |
| `comp_skip` | Rate | 0(关) | 方向分数(规则 10)低于 −`comp_skip`(标准差)的代币不挂单、已挂的撤掉;v7 取 0.5;负值:只挂分数高于 −`comp_skip` 的代币(方向 maker,dm10 取 −1);范围 −10 – 10 | None |
| `comp_dir` | Rate | 0(关) | 方向分数高于 `comp_dir` 的代币的净份数上限改为 `dir_cap_shares`;v7 取 0.5;范围 0 – 10 | None |
| `dir_cap_shares` | Count | 20 | 方向分数高于 `comp_dir` 时该侧的净份数上限(代替 `inv_max_shares`,规则 4 的"加上一张挂单"照算);范围 1 – 1 000 000 | None |
| `comp_every_ms` | DurationMs | 250 | 方向分数至多每这么久(UTC 时间)重算一次;0 = 每次判定都算;范围 0 – 60 000 | None |
| `comp_flow_min_shares` | Count | 50 | 计入流向特征的成交至少这么多份 | None |
| `comp_excl_own` | Bool | 1 | 1:失衡特征里扣除本实例在簿上的挂单(实盘:挂单在交易所公开簿里);0:不扣(模拟账户:挂单不在公开簿里) | None |
| `comp_imb1_mu` / `comp_imb1_sd` | Rate | 6.034656e-6 / 0.674104 | 方向分数里 买一数量失衡特征的均值 / 标准差(标准差 ≥ 1e-9);默认值为 2026-10-28 研究拟合(`comp_norm.json`) | None |
| `comp_imb5_mu` / `comp_imb5_sd` | Rate | 3.815088e-6 / 0.382700 | 方向分数里 5 美分深度失衡特征的均值 / 标准差(标准差 ≥ 1e-9);默认值为 2026-10-28 研究拟合(`comp_norm.json`) | None |
| `comp_mom30_mu` / `comp_mom30_sd` | Rate | −1.798897e-6 / 0.165942 | 方向分数里 30 秒中间价变化特征的均值 / 标准差(标准差 ≥ 1e-9);默认值为 2026-10-28 研究拟合(`comp_norm.json`) | None |
| `comp_mom60_mu` / `comp_mom60_sd` | Rate | −5.916940e-6 / 0.224953 | 方向分数里 60 秒中间价变化特征的均值 / 标准差(标准差 ≥ 1e-9);默认值为 2026-10-28 研究拟合(`comp_norm.json`) | None |
| `comp_bret15_mu` / `comp_bret15_sd` | Rate | −2.810269e-5 / 0.919830 | 方向分数里 15 秒参考收益(σ 单位)特征的均值 / 标准差(标准差 ≥ 1e-9);默认值为 2026-10-28 研究拟合(`comp_norm.json`) | None |
| `comp_bret60_mu` / `comp_bret60_sd` | Rate | −2.486879e-5 / 0.968756 | 方向分数里 60 秒参考收益(σ 单位)特征的均值 / 标准差(标准差 ≥ 1e-9);默认值为 2026-10-28 研究拟合(`comp_norm.json`) | None |
| `comp_flow30_mu` / `comp_flow30_sd` | Rate | −4.960950e-5 / 6.260787 | 方向分数里 30 秒大单净流向特征的均值 / 标准差(标准差 ≥ 1e-9);默认值为 2026-10-28 研究拟合(`comp_norm.json`) | None |
| `comp_bdrop1_mu` / `comp_bdrop1_sd` | Rate | 0.001550071 / 3.031530 | 方向分数里 1 秒买一变化(美分)特征的均值 / 标准差(标准差 ≥ 1e-9);默认值为 2026-10-28 研究拟合(`comp_norm.json`) | None |
| `load_gate_ns` | Count | 0(关) | 上一个 UTC 分钟的行情延迟每秒 90 分位的中位数 ≥ 它(纳秒)时不挂单(规则 11);v7 取 12 688 500(12.6885 ms,研究的繁忙档) | None |
| `mdslow_gate_ns` | Count | 0(关) | 上一个 UTC 分钟的行情延迟每秒中位数的中位数 > 它(纳秒)时不挂单;v7 取 40 000 000 | None |
| `dir_pair_free` | Bool | 0(关) | 1:方向分数高于 `comp_dir` 的代币凑对时不受对价上限约束(两模式规则:被看好的一侧把另一侧留下的库存配掉;公允价上限照常);schema v7 | None |
| `tilt_z` | Rate | 0(关) | 方向分数高于它的代币每单挂 `tilt_clip_shares` 份(倾斜:分数偏向的一侧挂大单);schema v8 | None |
| `tilt_clip_shares` | Count | 0(关) | 上一行的单笔份数;0 = 关 | None |
| `late_hedge_ms` | DurationMs | 0(关) | 最后一分钟止损:从 `stop_before_ms` 到窗口结束前 `late_hedge_ms`,只挂凑对的买单,由分数高于 `late_hedge_z` 的腿(信号与净持仓相反)在买一挂、不受对价上限;0 = 关 | None |
| `late_hedge_z` | Rate | 0 | 止损凑对的腿需要的分数 | None |
| `ref_aux` | Count | 0 | 辅助组(见 [group 形态](#group-形态)):位 0 = 永续参考,位 1 = Chainlink 组,位 2 = Coinbase 永续参考(schema v45);范围 0–7;schema v41(v39/v40 属于另一条研究线,本线没有) | None |
| `fc_on` | Bool | 0(关) | 规则 14 开:成交条件 EV 模型独自决定当前窗口的买单;需要 Chainlink 组;schema v41 | Down |
| `fc_s` | Rate | 1 | 规则 14:σ 的厚尾系数 s | None |
| `fc_lam` | Rate | 0.1 | 规则 14:每次成交的不利移动 λ(bp) | None |
| `fc_c0` / `fc_c1` | Rate | 1 / 0 | 规则 14:每次成交的固定成本与价格相关成本(美分;成本 = c0 + c1 · \|p − 50\| / 50) | None |
| `fc_w` | Rate | 0.5 | 规则 14:模型概率的权重(其余为代币中间价) | None |
| `fc_m_post` / `fc_m_cancel` | Rate | 0 / 0 | 规则 14:挂单与撤单的分数阈值(美分) | None |
| `fc_m_pair` | Rate | −1(不配对) | 规则 14:配对差额(美分);< 0 不配对 | None |
| `fc_iv_n` | Count | 60 | 规则 14:隐含 σ 的样本数(每秒一个,5–600) | None |
| `fc_ver` | Count | 0 | 规则 14 参数版本(重拟合的编号,只用于审计) | None |
| `lead_w_ms` | DurationMs | 0(关) | 规则 15:永续中间价的回看时长;0 = 关;0–2 000;schema v42 | None |
| `lead_x_bp` | Rate | 0(关) | 规则 15:触发阈值,\|ret\| 至少这么多(占 1 的比例;1 bp = 100 000) | None |
| `lead_life_ms` | DurationMs | 3 000 | 规则 15:领先挂单的存活时长,到时未全部成交即撤 | None |
| `lead_cool_ms` | DurationMs | 2 000 | 规则 15:同方向两次触发的最短间隔 | None |
| `lead_shares` | Count | 5 | 规则 15:每张领先挂单的份数 | None |
| `lead_pc_w_ms` / `lead_pc_x_bp` | DurationMs / Rate | 0 / 0(关) | 规则 15 永续撤单:回看时长与阈值 | None |
| `lead_pc_block_ms` / `lead_pc_cool_ms` | DurationMs | 1 000 / 1 000 | 规则 15 永续撤单:规则 14 在被不利代币上的封锁时长;同方向两次的最短间隔 | None |
| `lead_only` | Bool | 0 | 规则 15 独占:其他开仓规则都不运行,持仓到结算 | Down |
| `lead_src` | Count | 1 | 规则 15 触发源(位):1 = 永续,2 = 公允价参考(现货),4 = 第二参考(Coinbase 现货),8 = Coinbase 永续(`ref_aux` 位 2);范围 0–15;schema v43(位 4、8 为 v45) | None |
| `lead_cf_ms` / `lead_cf_x_bp` | DurationMs / Rate | 0(关)/ 0.5 bp | 规则 15 的确认:由 Coinbase 触发源(`lead_src` 位 4、8)挂出的领先单,挂出后 `lead_cf_ms` 仍没有被币安参考确认就撤;确认 = 永续或现货参考相对"挂出前 `lead_w_ms`"的中间价、朝该单方向的变动 ≥ `lead_cf_x_bp`;schema v45 | None |
| `lead_price` | Count | 0 | 规则 15 挂单价:0 = 买一 + 1 美分;1 = 模型价 | None |
| `lead_m` | Rate | 0.5 | 规则 15 模型价的扣减(美分) | None |
| `lead_maxtick` | Count | 3 | 规则 15 模型价至多高出买一的美分数 | None |
| `lead_k` | Rate | 1 | 规则 15 模型价的重定价系数 k | None |

全部字段只有默认值(不按 group 覆盖);`minor_change` 均为 `None`。`fv_limit` 至 `pred_w_ser` 为 schema v5 加入(`since` 5,v4 参数体取默认值,即 `fv_limit` 关闭)。`comp_skip` 至 `mdslow_gate_ns` 为 schema v6 加入(`since` 6,v5 参数体取默认值:`comp_skip`、`comp_dir`、`load_gate_ns`、`mdslow_gate_ns` 为 0,规则 10、11 关闭,决定与 v6 相同)。均值与标准差都是 1e-9 定点:`comp_imb1_mu` 6 035、`comp_imb1_sd` 674 103 869、`comp_imb5` 3 815 / 382 699 882、`comp_mom30` −1 799 / 165 941 588、`comp_mom60` −5 917 / 224 952 723、`comp_bret15` −28 103 / 919 829 667、`comp_bret60` −24 869 / 968 755 915、`comp_flow30` −49 610 / 6 260 786 978、`comp_bdrop1` 1 550 071 / 3 031 529 770。

v7 的实盘参数 = v6 实盘参数(`clip_shares` 5、`inv_max_shares` 10、`sigma_min` 0.4 bp、`fv_cool_ms` 1 000、`requote_ms` 1 000、`nochase_ms` 1 000、连续成交暂停 2/10 s/10 s、`max_window_cost` 20 USDC、`fv_limit` 1、`fv_hyst` 0.01,其余默认)加 `comp_skip` = `comp_dir` = 0.5、`dir_cap_shares` 20、`load_gate_ns` 12 688 500、`mdslow_gate_ns` 40 000 000、`pair_margin` −0.02;模拟账户另设 `comp_excl_own` 0。

### 订阅

槽位腿订阅 `BOOK` 与 `TRADE`,参考腿订阅 `BBO`,辅助组按 [group 形态](#group-形态)订阅,都投递给快速部分(`Part::Fast`);`InjectWarmup` 也投递给快速部分(`Part::Both`,只用 local_kind 1 的 Chainlink 样本,见规则 12)。成交事件不触发判定(只读取上一事件的下单结果,方向分数开启时另记下大单,规则 10);订阅它也是为了让行情流带上成交:模拟撮合的排队成交模型(`[sim] maker_fill = "queue"`)按公开成交消耗挂单前方的数量,没有成交时只有价格穿过挂单才成交。没有 Model。

### 波动率

参考腿每条 `Bbo` 取 2 × 中间价(买一 + 卖一)。UTC 秒内的第一条报价记一个样本:与上一个样本的相对变化 r(1e-9,截到 ±1),进入长度 `vol_window_s` 的环,同时维护 Σr²。σ = ⌊√(Σr² / 样本数)⌋(每秒,1e-9);样本数不足 `vol_min_samples` 时没有 σ(不挂单)。没有报价的秒不记样本。`clock_ok = false` 时不记样本。

### 特征

规则读取的特征是 [hft-feature](../../crates/hft-feature/README.md) 的节点(`RealizedVol`、`BboTop`、`SecHist`、`SignedFlow`、`OwnExcludedSide`、`LagQuantiles`、`zsum`、Φ/Φ⁻¹ 与定点对数),由本策略的 `MakerFeatures`(`src/features.rs`)组合;规则 9 的方向预测与规则 10 的八个特征是本策略自己的组合。每个事件先跑特征的处理器,再跑规则,处理器顺序固定(执行计划,`clock_ok = false` 时只跑参考报价的 `BboTop`):

| 事件 | 处理器(按序) |
|---|---|
| 参考腿 `Bbo` | 参考逐秒环 → 槽位逐秒环 → 延迟统计 → `BboTop`(两侧都有价时)→ `RealizedVol` |
| 槽位 `Trade` | 大单环 → 槽位逐秒环 → 延迟统计 |
| 其他事件 | 槽位逐秒环 → 延迟统计 |

参考逐秒环、大单环与槽位逐秒环只在方向分数开启(`comp_skip` 或 `comp_dir` 非 0)时运行,延迟统计只在延迟闸门开启(`load_gate_ns` 或 `mdslow_gate_ns` 非 0)时运行。`MakerFeatures::export` 按 `features::COLUMNS` 写出一行定点列:`ref_mid_x2`、`ref_imb`、`ref_sigma`(至少 1 个样本即有)、`ref_samples`、`lag_minute`、`lag_p90`、`lag_p50`(上一结束分钟,×100 ns;无则 `NA`)。`Strategy::FEATURES` = `features::COLUMNS`,`export_features` 写出同一行([hft-strategy-sdk 特征导出](../../crates/hft-strategy-sdk/README.md#特征导出)),`replayer features` 由此导出。

### 规则

每条参考腿 `Bbo`、槽位腿 `Book` 与本实例的 `Order` 事件之后判一次(参考腿 `Bbo` 先更新参考中间价与波动率样本)。每个事件(含 `Trade` 等不判定的事件,`clock_ok` 时)先做规则 10 的逐秒记录与规则 11 的延迟采样:

1. **当前窗口**:S = ⌊UTC 秒 / `window_s`⌋ × `window_s`,当前对 = (S / `window_s`) mod `pairs`,剩余 = S + `window_s` − 现在。
2. **能否挂单**:`enabled`、`clock_ok`、剩余 > `stop_before_ms`、有 σ、隐含 σ = σ × `sigma_scale` ≥ `sigma_min`、隐含 σ × √剩余秒数 ≥ `min_rem_sd`(平方根取整数下界)、规则 11 的两个闸门都没有关闭。
3. **仓位**:当前对 Up、Down 两腿的份数 U、D 与平均成本 cu、cd 取 `group_pos` 第 0 腿的 `qty` 与 `entry_px`。
4. **每条槽位腿的目标价**:只有当前对的两条腿、且第 2 步成立时才有目标:该腿簿 `Live`、有买一 b 与卖一 a 且 b < a、b 在 [p_min, p_max] 内、该侧净份数(Up 腿 U − D,Down 腿 D − U)加上 `clip_shares` ≤ 上限(`inv_max_shares`;规则 10 的方向分数高于 `comp_dir` 时为 `dir_cap_shares`)(按本单全部成交计;成交量略少于委托量时,如 9.99 / 10 份,不再放行整张新单)、该腿不被规则 10 跳过;目标 = b。**对价上限**:这一侧的份数少于另一侧(这张买单成交会凑成对)时,上限 = 1 − 另一侧平均成本 − `pair_margin`,向下取整到 1 美分(0.01);`pair_margin` 为负时上限高于 1 − 另一侧平均成本,即以每对至多 −`pair_margin` 的亏损减仓;目标 = min(b, 上限),低于 p_min 则没有目标。开启 `fv_limit` 时目标再与第 9 步的公允价上限(向下取整到 1 美分)取较小者;没有公允价时没有目标。
5. **公允价撤单**:挂单时记下参考中间价 B₀、d₀ = Φ⁻¹(Up 代币中间价,截到 [0.001, 0.999])与 den = 隐含 σ × √(剩余秒数 − 40)(结算取窗口最后 60 s 的时间平均,剩余 t 秒时尚未确定的方差相当于 t − 40 秒的现价)。之后 Δ = (B − B₀) / B₀,dfv = Φ(d₀ + Δ / den) − Φ(d₀)(Up 的概率变化)。Up 腿的挂单在 dfv ≤ −`fv_cancel`、Down 腿在 dfv ≥ +`fv_cancel` 时撤单,该腿 `fv_cool_ms` 内不再挂单。Φ 与 Φ⁻¹ 用 z = k/64(k = 0..320)的整数表线性插值。
6. **挂单的去留**:有挂单时,下列任一成立则撤单(撤单不受 `requote_ms` 限制):不再满足第 2 步、不是当前对、簿不是 `Live`;第 5 步触发;买一高于挂单价、簿没有锁住或交叉(买一 < 卖一)且目标为空或高于挂单价(被抢先,改挂更高;一笔吃单分成几条价位更新到达时簿会短暂锁住,不算被抢先);有对价上限且挂单价高于它;开启 `fv_limit` 时挂单价高于第 9 步的公允价上限 + `fv_hyst`;该腿被规则 10 跳过(方向分数低于 −`comp_skip`;不设冷却)。**买一低于挂单价(本价位已被吃空)时留守**,不跟着降价。因被抢先而撤单时,若 `nochase_ms` > 0,该腿冷却到撤单时刻 + `nochase_ms`(与第 5 步的冷却取较晚者)。
7. **新挂单**:没有挂单而有目标、距上次改价 ≥ `requote_ms`、过了冷却、该腿不在连续成交暂停中(`streak_k` ≥ 2 且已有 ≥ `streak_k` 笔成交时,倒数第 `streak_k` 笔距今 < `streak_window_ms` 且最后一笔距今 < `streak_pause_ms` 即为暂停;每次 `Filled` 的 d > 0 计一笔)、当前窗口已买入成本 < `max_window_cost`(为 0 时不限)→ `place` 一张 `OrderIntent{Buy, PostOnly, class Open, px = 目标, qty = clip_shares 份, tag = gix + 1, max_rest_ms}`(不带链接)。一条腿同时至多一张挂单;撤单在途时不再挂。
8. **订单状态**:`place` 后下一个事件读 `Ctx::outcome`:`Placed(r)` → 记下 `r` 与挂单所在窗口的起点,其余(`Rejected`、`Shadowed`、无结果)→ 视为没有挂单;`Order` 事件按 `r` 匹配,`Filled{done}`、`Cancelled`、`Rejected` → 没有挂单;`Aged`(挂单超过 `max_rest_ms`,OMS 不替策略撤)→ 撤单。**过期状态**(每次判定时先做):① 撤单发出后 `stuck_reset_ms` 内没有上述终态回报(如交易所报的成交量略少于委托量、撤单与成交竞争);② 槽位对重新成为当前对时,挂单或撤单在途的状态属于更早的窗口 → 对该订单再发一次撤单(不等待结果)并视为没有挂单,随即可按第 7 步新挂单;此后该订单的回报不再匹配(`r` 已清除),不改变新挂单的状态。

9. **公允价上限**(`fv_limit` 开启,只在第 2 步成立时计算,每次判定一次):
   - 窗口起点之后第一次判定时记下参考 2 × 中间价 B_open;Δ = (B − B_open) / B_open。den 同第 5 步(隐含 σ × √(剩余秒数 − 40))。
   - **锚定**:Up 腿的簿有买一与卖一、买一 < 卖一、中间价 m 在 [0.03, 0.97] 内时,g = Δ − Φ⁻¹(m) × den(盘口隐含的参考涨跌与实际涨跌之差:慢的是行权价与基差,快的是盘口还没跟上的参考变动);偏移 off 第一次取 g,之后 off ← off + (g − off) × dt / (`fv_anchor_ms` + dt),dt 为距上次更新的引擎时间。没有 off 时没有公允价。
   - **预测**(以下一 30 秒参考收益的标准差为单位,截到 ±6):`pred_w_imb` × 参考最新报价的 (买一数量 − 卖一数量) / (买一数量 + 卖一数量) + `pred_w_ret` × 最近 5 个 1 秒收益之和 / (σ √5)(截到 ±6)+ `pred_w_ser` × 最近 15 个 1 秒收益之和 / 其绝对值之和(为 0 时取 0);σ、1 秒收益同"波动率"。漂移 = `pred_k` × 预测 × σ × √30。
   - Up 的公允概率 fu = Φ((Δ − off + 漂移) / den)(z 截到 ±6)。偏斜 = `inv_skew` × (U − D) / (`inv_max_shares` 份)。
   - 上限:Up 腿 fu − `fv_edge` − 偏斜,Down 腿 1 − fu − `fv_edge` + 偏斜(占 1 的比例)。目标低于买一时挂在上限价位(比买一更深)。

10. **方向分数**(`comp_skip` 或 `comp_dir` 不为 0 时开启;全部为整数,`Rate` = 1e-9 定点,除法向零截断,以下"⌊⌋"即此截断):
    - **逐秒记录**(每个事件之前,UTC 秒 s = ⌊UTC / 1 s⌋):参考腿 `Bbo`(买一、卖一 > 0)记 R[s] = 2 × 中间价;每条槽位腿的簿 `Live`、有买一 b 与卖一 a 且 b < a 时记 M[s] = b + a、B[s] = b(簿含本实例的挂单);同一秒后写覆盖先写。槽位腿的 `Trade` 数量 ≥ `comp_flow_min_shares` 份时记 (UTC, inst, ±数量)(主动买为 +),最近 256 笔。读某秒的值时依次取 s、s − 1、s − 2 中第一个有记录的秒(都没有即缺失)。
    - **何时计算**:第 2 步成立、当前对的两条槽位腿的簿都不是 `Uninit` 时,两条腿各算一次分数,结果保留到 UTC 前进 `comp_every_ms` 或窗口变化;第 2 步不成立时清除(恢复后立即重算)。
    - **特征**(代币 X、其互补代币 O,unit = 价格单位 10^|px_exp|,cent = unit / 100,数量单位 10^|qty_exp|):扣除本实例挂单(`comp_excl_own` 且该腿挂单属于当前窗口、`Views::order` 为 `Live`/`PendingCancel`/`PendingAmend`:在其价位扣 委托量 − 已成交,不低于 0)后,X、O 的买盘从买一起最多读 64 档;"买一" = 扣除后数量 > 0 的最高价。① imb1 = ⌊(qX − qO) × 1e9 / (qX + qO)⌋(q = 各自买一的数量,和为 0 时取 0);② imb5 同式,q 换成买一所在美分 ⌊b / cent⌋ × cent 起向下 5 个整美分价位(k = 0..4)的数量之和;③④ mom_h = ⌊(M_X[s] − M_X[s − h]) × 1e9 / (2 × unit)⌋,h = 30、60,任一缺失则该特征缺失;⑤⑥ bret_h = ⌊sgn × L × 10^15 / (隐含 σ × √h_micro)⌋,h = 15、60,L = ln(R[s] / R[s − h]),隐含 σ = ⌊σ × `sigma_scale` / 1e9⌋(σ 同"波动率"),sgn = X 为 Up 时 +1、Down 时 −1,√15_micro = 3 872 983、√60_micro = 7 745 967,R 缺失或没有 σ 时该特征缺失;⑦ flow30 = sign(v) × ln(1 + |v| 份):v = UTC − 30 s 及之后记下的 X 的带号数量之和减去 O 的,ln 的参数为 1e9 + ⌊|v| × 1e9 / 数量单位⌋;⑧ bdrop1 = ⌊(B_X[s] − B_X[s − 1]) × 1e9 / cent⌋(美分),任一缺失时取 0。①②⑦⑧ 总有值,③–⑥ 可缺失。
    - **对数**:atanh2(u) = 2 × Σ_{j=0}^{11} ⌊p_j / (2j + 1)⌋,p_0 = u,p_{j+1} = ⌊p_j × u2 / 1e9⌋,u2 = ⌊u × u / 1e9⌋;ln(a / b) = atanh2(⌊(a − b) × 1e9 / (a + b)⌋);参数 x ≥ 1e9 的 ln(x / 1e9):x 反复 ⌊x / 2⌋ 共 k 次直到 x < 2e9,结果 = k × 693 147 181 + ln(x / 1e9)(上式)。
    - **分数** z = ⌊Σ_i ⌊sign_i × (x_i − mu_i) × 1e9 / sd_i⌋ × 1e9 / 2 828 427 125⌋(即除以 √8;缺失的特征贡献 0),sign = +1,flow30 为 −1(反向);mu、sd 为 `comp_<特征>_mu`、`comp_<特征>_sd`。
    - **用法**:z < −`comp_skip`(`comp_skip` ≠ 0;负值即 z 不高于 |`comp_skip`| 时)的腿不挂单、已挂的撤掉(分数尚未算出时不跳过);z > `comp_dir`(`comp_dir` > 0)的腿第 4 步的上限用 `dir_cap_shares`;`dir_pair_free` 时这条腿第 3 步没有对价上限。
11. **行情延迟闸门**(`load_gate_ns` 或 `mdslow_gate_ns` 不为 0 时采样):每个事件读出各槽位 inst 自上次以来的新行情戳(`Views::md_stamp`:L2 批、交易所整簿与成交,每 inst 至多最近 16 条),跳过 ⌊`ex_ts_ns` / 1 ms⌋ = 0 或 `sw_rx_ns` ≤ 0 的,按 `sw_rx_ns` 排序后逐条采样:延迟 = `sw_rx_ns` − ⌊`ex_ts_ns` / 1 ms⌋ × 1 ms(纳秒),按 ⌊`sw_rx_ns` / 1 s⌋ 归入接收秒;早于正在收集的秒的样本丢弃,每秒至多 1 024 个。一秒在出现更晚秒的样本或 UTC(策略时间)进入更晚的秒时结束:排序,算 90 分位与中位数(线性插值:n 个值 v,h = q × (n − 1)(q 以十分之一计),lo = ⌊h / 10⌋,fr = h mod 10,值 × 10 = 10 × v[lo] + fr × (v[lo + 1] − v[lo]))。一分钟(⌊秒 / 60⌋)在出现更晚分钟的秒或 UTC 进入更晚的分钟时结束:对其各秒的 90 分位、中位数(× 10)各取中位数(同一公式,结果 × 100)。判定时若刚结束的分钟恰是 UTC 当前分钟的前一分钟:其 90 分位的中位数 ≥ `load_gate_ns` × 100 或中位数的中位数 > `mdslow_gate_ns` × 100(阈值为 0 的不判)→ 第 2 步不成立;前一分钟没有样本时两个闸门都不关。
12. **结算窗口流动性**(`settle_ms` > 0,schema v25,研究用,默认关闭;设计见仓库外 `E:\trade\reports\Polymarket_设计_结构性优势_2026-11-02.md` §3):剩余 ≤ `settle_ms`(≤ 60 s)且主规则没有目标(第 2 步不成立、不在 `late_hedge_ms` 止损段、不在 `pair_stop_ms` 凑对优先段)时,本模块接管当前对两条腿的挂单槽位。参考腿每条 `Bbo` 记逐秒 2 × 中间价 R[s](每秒最后一条,512 秒环,精确取秒)。窗口起点 S、当前 UTC 秒 s:行权价 K = mean(R[S−60 .. S−1])(每窗查一次,记录秒数 < 48 则本窗不运行);n_lock = s − (S + `window_s` − 60),锁定部分 = mean(R[S + `window_s` − 60 .. s−1])(缺 > 20% 不算),n_fut = S + `window_s` − s,R_now = 最新参考 2 × 中间价;μ = ((n_lock × 锁定均值 + n_fut × R_now) / 60) / K − 1;sd = σ × `settle_vol_mult` × √(n_fut(n_fut+1)(2n_fut+1) / (6 × 60²)),σ 为第 2 步的实现波动;p_up = Φ(μ / sd)(z 截到 ±6)。热门方 F = Up 若 p_up ≥ 0.5,否则 Down,p_F = max(p_up, 1 − p_up)。F 的簿 `Live`、有买一 b、不锁不交叉时目标价 px = b(`settle_improve` 且卖一 − b ≥ 2c 时 b + 1c);p_F ≥ `settle_p_min`、`settle_bid_lo` ≤ px ≤ `settle_bid_hi`、本窗模块已买 + `settle_clip_shares` ≤ `settle_max_shares`、n_lock ≥ `settle_min_lock_s`、|μ| ≥ `settle_mu_min`、距该腿上次改价 ≥ `settle_requote_ms` 时挂 post-only 买单 `settle_clip_shares` 份,tag = 0x10000 | (gix + 1)。模块的挂单在下列任一成立时撤:簿不 `Live`、p_up 算不出、该腿不再是热门方、p_F < `settle_cancel_p`、买一高于挂单价(簿不锁时;撤后按 `settle_requote_ms` 重挂)、挂单价 > `settle_bid_hi`;买一低于挂单价时留守。成交持有到结算。研究导出在 `COMP_COLUMNS` 之后加 `settle_p_up`、`settle_mu_bp`(μ,1e-9,1 bp = 100 000)、`settle_nlock` 三列(模块运行且算出时有值,否则 `NA`)。状态(`SettleState`、每腿的模块标记 `skind`、参考逐秒环)不进快照、不进 `Quote`。
    - **Chainlink**(schema v32):PM 按 Chainlink BTC/USD 60 s TWAP 结算(窗口末 60 s 均价 ≥ 起点前 60 s 均价 → Up)。外部进程 `tools/pm-md-probe/scripts/cl_feed.py` 订阅 PolyBolt `price.crypto`(btcusd),每个新样本发一条 `InjectWarmup`(local_kind 1,载荷 v1 共 17 字节小端:版本 u8 = 1、Chainlink 时间戳 ms i64、价格 1e-8 USD i64;其它版本、短载荷、价格 ≤ 0 忽略)。样本按 Chainlink 秒 ⌊ts / 1 s⌋ 记入 512 秒环(同秒后写覆盖;比最新样本早的只补自己的秒),最新样本的秒 ≥ s − 3 时为当前价。`settle_src` 1:K、锁定部分与 R_now 改用 Chainlink 秒与当前价(σ 仍取参考),没有当前价时 p_up 算不出(不挂、已挂的撤)。领先 L = μ × mean(K)(美分;Chainlink 为 1e-8 USD,参考为 2 × 中间价按其价格单位换算),Up 取 L、Down 取 −L:`settle_lead_usd` > 0 时挂单还需 L ≥ `settle_lead_usd` × 100,挂单在 2L < `settle_lead_usd` × 100 时撤;`settle_edge` > 0 时挂单价还需 ≤ p_F − `settle_edge`,挂单价 > p_F 时撤。Chainlink 环与当前价不进快照。**原生样本**(v41 `ref_aux` 位 1):Chainlink 组的 `Index` 事件按同一规则写入同一环(样本时间 = `ex_ts_ns`),`Mark` 事件(官方 60 s TWAP)按秒写入另一环(`ClFeed::twap_at`:不晚于该秒、至多往前 3 s 的最新 TWAP);此时旁路样本被忽略。
13. **止损配对**(`rescue_ms` > 0,schema v32,默认关闭):窗口年龄 ≥ `rescue_ms` 且剩余 > `stop_before_ms` 时,一条短腿(本腿份数 < 另一腿)若分数 z > `rescue_z`(规则 10;此段即使第 2 步不成立也计算分数)且另一腿(持仓腿)买一 ≤ `rescue_px`,按最后一分钟止损同样的方式挂单:不受第 2 步、对价上限与公允价上限约束,在买一挂 `clip_shares`(`rescue_cap` > 0 时价格不高于 1 − 持仓腿均价 + `rescue_cap`);挂出后本窗口不再开新仓(第 4 步的开仓腿全部不挂)。记号(窗口起点)不进快照。
    - **震荡开关预热**(v31 `chop_k` 的记录不进快照,重启后要攒够 `chop_k` / 2 个窗口才判断):`InjectWarmup` local_kind 2,载荷 v1 小端:版本 u8 = 1、条数 u8、条数 × (窗口起点秒 i64, 效率比 1e-9 i64)。只在环为空时接收;效率比不在 [0, 1e9]、窗口不递增、或不早于正在累计的窗口的条目跳过;保留最后 24 条,最新在最后。外部进程 `tools/pm-md-probe/scripts/chop_seed.py` 在新 run 启动后从其它 run 的 seq journal 按本节同一算法(每窗 30..240 s 每秒第一次判定时 Up 中间价,≥ 100 秒)算出最近的窗口并发送。
14. **压力挡开仓**(`ofi_win_ms` > 0,schema v33,默认关闭;Albers 等 2025 的 Reversal 情景):每条槽位腿在自己簿的每条 `Book` 事件上按 Cont-Kukanov-Stoikov 记买一 / 卖一的订单流增量 e = 1{b ≥ b₋}·q_b − 1{b ≤ b₋}·q_b₋ − 1{a ≤ a₋}·q_a + 1{a ≥ a₋}·q_a₋,按引擎时间 100 ms 分桶累计(256 桶,约 25.6 s;腿的合约换了就重来)。一条不短的腿(本腿份数 ≥ 另一腿)在本簿一档失衡 (买一量 − 卖一量) / (买一量 + 卖一量) < `ofi_obi_max` 且过去 `ofi_win_ms` 的订单流 ≤ 0 时不开仓(第 4 步没有目标,已挂的开仓单按第 6 步撤);历史不够 `ofi_win_ms` 时不挡。Up 与 Down 是同一本簿的两面,每条腿用自己的簿即是按腿定向的失衡与订单流。订单流环不进快照。

依据:2026-08 逐事件回放(1 985–2 221 个市场,`E:\trade\staging\pm5m\mm_event2.py` 的 `final` 变体):原规则 −0.11 c/份;加留守 +0.16、再以公允价撤单代替 1σ 跳动暂停 +0.25、再加对价上限 +0.52 c/份(t 3.6,7/7 天)。v3 三条(不追价 1 s、冷却 1 s、同腿 10 s 内 2 笔成交暂停 10 s,即回放的 `s2`)在同一回放中把净库存的亏损从每净份 −22.7 降到 −5.7 美分;82 美元账户的 `micro_s2_c5_cap10_mc20`(每张 5 份、净上限 10 份、每窗成本 ≤ 20 美元)+1.03 c/份。

v4(规则 6 的锁簿、规则 8 的过期状态与 `Aged` 撤单)来自 2026-10-23 实盘 journal:一笔吃单分几条价位更新到达时簿短暂锁住,对价上限的挂单在一个窗口里被当作被抢先撤掉重挂 33 次;一条腿的撤单没有终态回报(成交 4.997 / 5 份、撤单早于交易所回执)而停在撤单在途,三个窗口后槽位对复用时这条腿不再挂单(单边窗口)。

v5(规则 4 的库存上限按本单全部成交计)来自 2026-10-23/24 实盘:一张 5 份的单只成交 4.99 份后净份数 9.99 < 10,仍放行一张 5 份的新单,净持仓到 15 份。快照布局不变。

v6(规则 9,默认关闭)来自 2026-10-24/25 的方向因子研究与回放(仓库外 `E:\trade\reports\Polymarket_30s方向因子研究.md`):参考市场买一卖一失衡、5 秒收益与 15 秒效率比的线性组合,对下一 30 秒参考收益的样本外相关在 8 月为 0.14(σ 门槛开时,留一天交叉)、10-25 为 0.22(用 8 月拟合的系数),系数即上表默认值。按实盘延迟(挂单 37 ms、撤单 60 ms)并计入"对手盘越过挂单价即成交"的回放,扣返佣:现行 v5 8 月 +$10、10-25 −$11;开启规则 9(锚定 1 s、偏斜 0.03、`pred_k` 0.2、σ 门槛 0.4)8 月 +$93–136、10-25 +$6–19(3 因子即默认系数:+$93、+$16);不开 σ 门槛时门槛关闭时段在 10 月与慢延迟下亏损。

v7(规则 10、11,默认关闭)来自 2026-10-28 负责人要求的混合做市研究(仓库外整数移植 `bm_port.py` 与按引擎回放的 `bm_sim.py`,`E:\trade\staging\pm5m\v7spec`):8 个代币级特征的组合分数对下一段代币方向有预测力,做市与按分数持有方向性库存的混合优于 v6;繁忙(上一分钟行情延迟 90 分位中位数在研究的高档,阈值 11.2105 / 12.6885 ms 的第 2 档)与慢行情(中位数 > 40 ms)分钟的挂单亏损,停挂。研究的分数用浮点、行情延迟用 journal 的 `rx_ns`(即 `sw_rx_ns`);本实现的定点算法即上文,移植以此逐位对齐。

持仓不平:窗口结束后由发现进程的 `Settle` 交割(模拟账户经 sim-gw;实盘交割尚未实现)。


**规则 14:成交条件 EV(schema v41,`fc_on`,缺省关)**。研究 Spec "成交条件期望_数学模型"(仓外 `E:\trade\reports\`,Python 参照 `E:\trade\staging\fcev\sim2.py` 的 `Window.features`/`score_arr`/`Strat`)。开启时只有本规则决定当前窗口两腿的买单,规则 1–13(含规则 12 结算模块)不运行;需要 Chainlink 辅助组(`ref_aux` 位 1),没有时不挂、已挂的撤。全部定点 1e-9(`src/fcev.rs`):

- **特征**(每个判定时刻):K = 窗口起点的官方 60 s TWAP(不晚于起点、至多往前 3 s 的最新样本);C = 最新 Chainlink 价格(须在 3 s 内);S = C × 参考中间价(当前)/ 参考中间价(C 的样本秒开始时,即前一秒的最后一条报价)。剩余 τ ≥ 60 s:μ = S,sq = √(τ − 40),remf = 1;τ < 60 s:n = 最后 60 s 中已收到的 Chainlink 秒数,rem = 60 − n,μ = (已收到样本之和 + rem × S) / 60,sq = max(rem/60 · √(max(rem, 1)/3), 0.001),remf = rem/60。dist = ln(μ / K) × 1e4(bp,Down 取负)。
- **σ**:每个 UTC 秒至多取一个隐含 σ 样本(该秒第一次判定时):z = Φ⁻¹(clip(Up 中间价, 0.02, 0.98)),|z| > 0.25、|dist| > 0.5 bp 且同号时取 clip(dist / (z · sq), 0.03, 5);每窗口重新开始,至多 `fc_iv_n` 个;≥ 5 个时 σ = 中位数,否则取参考市场实现 σ(1 秒收益,`vol_window_s`)× 1e4,再没有取 0.3 bp。**与 Python 的差别**:Python 的回退 σ 是 300 s 子采样实现波动;Python 按样本时间戳使用 Chainlink 样本(研究数据有约 1.2 s 的超前),本规则只用已收到的样本。
- **分数**(美分):s' = max(`fc_s` · σ, 0.03),d = clip(dist / (s' · sq), ±8),Δ = 100 · φ(d) / (s' · sq) · remf,fair = `fc_w` · 100 · Φ(d) + (1 − `fc_w`) · 该代币中间价,score(p) = fair − p − `fc_lam` · Δ − (`fc_c0` + `fc_c1` · |p − 50| / 50)。Φ 用 hft-feature 的表,φ 用 1/64 步长到 8 的表,均线性插值;与 Python 参照的逐秒平价见 BMK-052。
- **挂单**:窗口年龄 ≥ 5 s、剩余 > `stop_before_ms`、买一在 [`p_min`, `p_max`] 内、净份数(本腿 − 另一腿)+ `clip_shares` ≤ `inv_max_shares`、窗口买入成本未到 `max_window_cost`、距上次改动 ≥ `requote_ms` 时,score(买一) > `fc_m_post` 则在买一挂 `clip_shares` 份 post-only 买单。
- **撤单**:挂着的单在 score(挂单价) < `fc_m_cancel`、买一高于挂单价、特征缺失、`enabled` 关或剩余 ≤ 2 s 时撤;陈旧单(上一窗口、撤单无终态)按规则 8 处理。
- **配对**(`fc_m_pair` ≥ 0):本腿份数少于另一腿时,本腿只挂配对单:价 = min(⌊100 − 另一腿平均成本 − `fc_m_pair`⌋ 美分,买一取整到美分),量 = 份数差(按 lot 向下取整,小于 `min_qty` 不挂);目标价变化即撤单重挂;本腿此时的普通挂单撤掉(Python 参照允许两张并存,这里一腿只有一张单)。
- 参数可由 hft-core 的参数文件热加载(`[strategy_params_file]`,只许 `fc_*` 字段);`fc_ver` 只用于审计。


**规则 15:领先挂单(schema v42,缺省关)**。研究:仓外 `sim2.py` 的 `perp_triggers`、`Strat.on_lead`、`Strat.on_perp_against`(严格排队 + 80 ms 下单/撤单延迟,4 个样本外日结算 +2.5–4.4 美分/份,1/5/30 s markout 为正)。需要永续辅助组(`ref_aux` 位 0);`lead_w_ms` > 0 且 `lead_x_bp` > 0 开领先挂单,`lead_pc_w_ms` > 0 且 `lead_pc_x_bp` > 0 开永续撤单。实现 `src/lead.rs` 与 `Fast::lead_perp`:

- 每条永续 `Bbo` 把 2 × 中间价按引擎时间记入环(8 192 条);ret = 中间价 / (不晚于 t − W 的最近一条) − 1。触发只在窗口年龄 [5 s, 结束前 2 s] 内计数,冷却按方向分开、每个窗口重新开始。
- **永续撤单**(先于领先触发判定):|ret(`lead_pc_w_ms`)| ≥ `lead_pc_x_bp`、同方向距上次 ≥ `lead_pc_cool_ms` → 被不利的代币(上涨时 Down,下跌时 Up)的领先挂单撤掉(规则 14 开启时它的普通挂单也撤),规则 14 在该腿 `lead_pc_block_ms` 内不新挂(与研究一致:封锁不作用于领先挂单)。
- **领先挂单**:|ret(`lead_w_ms`)| ≥ `lead_x_bp`、同方向距上次 ≥ `lead_cool_ms` → 看好的代币(上涨 Up、下跌 Down);该腿没有任何订单、`enabled`、窗口年龄 ≥ 5 s 且距结束 ≥ 10 s、该代币卖一 − 买一 ≥ 2 美分、买一 > 0 且卖一 < 1、本腿净份数 + `lead_shares` ≤ `inv_max_shares` 时,在(买一取整到美分)+ 1 美分挂 `lead_shares` 份 post-only 买单(低于卖一,不穿价)。
- **生命周期**:挂单后 `lead_life_ms`(引擎时间)未完全成交即撤;距结束 ≤ 2 s、`enabled` 关或时钟不可用时撤;陈旧单按规则 8。每个永续报价与每次判定都检查。
- 领先挂单占用该腿的订单槽:规则 1–14 不管理它(跳过该腿),它终结后该腿恢复正常。
- **`lead_only`**:只有规则 15 下单——规则 1–14(含配对、反转、规则 12)都不运行,其余挂着的单撤掉;持仓到结算。
- **Coinbase 触发源与确认(schema v45,缺省关)**:`lead_src` 位 2(值 4)= 第二参考(Coinbase 现货 BTC-USD 的 BBO 中间价),位 3(值 8)= Coinbase 永续(`ref_aux` 位 2 的辅助组,[venue-coinbase](../../crates/venue-coinbase/README.md) `coinbase-adv` 的 level2 最优报价);各有自己的环与(源, 方向)冷却,W/X、挂单条件、价格与寿命同其他源。研究(仓外,2026-11-10/11):到达实盘机时 Coinbase 现货在 25% 的实盘单之前约 100 ms 已经出现同向 1 bp 变动,Coinbase 永续 51%;但它们的触发有相当一部分币安并不跟(永续 37% 单独触发,最小变动 0.34 bp)。`lead_cf_ms` > 0 时,由这两个源挂出的领先单需要确认:自挂出起,只要永续或现货参考相对"挂出前 `lead_w_ms`"的中间价、朝该单方向的变动达到 `lead_cf_x_bp`,该单即记为已确认,此后与普通领先单相同;挂出满 `lead_cf_ms` 仍未确认就撤(撤单不等下单确认)。币安源挂出的单不受此约束。
- **触发源与模型价(schema v43)**:`lead_src` 的位 0 = 永续(缺省)、位 1 = 公允价参考(现货 BTCUSDT 的 BBO 中间价,自己的环);两个源用同样的 W/X,冷却按(源, 方向)各自独立,任一源触发都走同一个挂单判定(每个代币仍只有一张领先单);永续撤单只看永续。`lead_price` 1:挂单价 = max(买一 + 1 美分, min(买一 + `lead_maxtick` 美分, ⌊代币中间价 + E − `lead_m`⌋, 卖一 − 1 美分)),E = `lead_k` × Δ × |move bp|,Δ = 100·φ(d)/(max(σ, 0.03)·sq)·remf、d = dist/(max(σ, 0.03)·sq)(规则 14 的同一组特征,不乘 `fc_s`;dist 按代币方向);特征不可用(无 Chainlink 组、无起点 TWAP、无新鲜价格)时为买一 + 1 美分。`lead_price` 1 时每秒取一个隐含 σ 样本(与规则 14 同一个环)。`lead_src` 1、`lead_price` 0(缺省)与 v42 逐位相同。与 Python 参照 `Strat.on_lead`(`lead_src`/`lead_price`/`lead_m`/`lead_maxtick`/`lead_k`)的挂单价逐笔相同(BMK-055.2)。
- 时间常量(研究里的固定值):`LEAD_START_MS` 5 000、`LEAD_END_MS` 2 000、`LEAD_LAST_MS` 10 000。

### Note 码表与报表字段

无 Note、无报表字段、无策略命令(`desc.rs` 三表为空)。

## 实现说明

<details><summary>实现要点</summary>

- 状态:每个槽位 group 一条 `Quote{id, inst, venue, state, handle, r, px, last_ens, win, b0, d0, den, cool_until, fill_ens, n_fill}`(`state` ∈ None/Placing/Live/Cancelling;`last_ens` 是最近一次挂单或撤单的引擎时间,`win` 是挂单所在窗口的起点(UTC 秒),两者用于规则 8 的过期状态;`b0`–`cool_until` 是规则 5 的锚点与冷却;`fill_ens` 是该腿最近 8 笔成交的引擎时间环,`n_fill` 为累计笔数);参考 inst;特征 `MakerFeatures`(最新参考 2 × 中间价与买一卖一数量失衡 `BboTop`、波动率环 `RealizedVol`(`recent(k)` 给出最近 k 个收益之和与绝对值之和)与规则 10、11 的状态);当前窗口的公允价锚点 `FvAnchor{win, b_open, off, off_ok, off_ens}`(规则 9,窗口变化时清零)。容量在 `new` 时按 `GroupTable::capacity()` 预留,`on_params` 只在 `vol_window_s` 变化时重建环。`GroupId` 与腿都不变的槽位保留挂单状态。
- 全部整数运算:相对变化 ×1e9 用 i128,平方和 u128,整数平方根(牛顿法)。
- 规则 10、11 的状态:每个槽位一条 `SlotHist{inst, mid_x2, bid, md_seen}`(两个 128 秒的逐秒环与已读行情戳计数,约 4 KB)、参考 `ref_x2` 逐秒环、大单环 `Trades`(256 笔)、两条腿的分数缓存 `CompCache{ok, win, utc_ns, z}` 与延迟统计 `LagQuantiles`(当前秒的样本缓冲 1 024 × 8 B、当前分钟每秒的两个分位、上一结束分钟的两个中位数);都在 `new` 分配,`on_params` 只在槽位 inst 变化时重建逐秒环;都不进快照(恢复后重新积累:最长 62 s 的历史与一分钟的延迟)。打分与分位在快速部分做(有界的整数运算:每 `comp_every_ms` 一次 2 × 64 档与 256 笔的扫描、每秒一次 ≤ 1 024 个样本的排序),不经 Model:本策略的部署不运行 advisor 进程,且分数需要当前簿。
- 快照(`DESC_VERSION` 7;布局与 4、5、6 相同;`FvAnchor` 与规则 10、11 的状态不进快照,恢复后在下一次判定重新取 B_open 与锚点):`n: u16`,每个槽位 `id: u32, state: u8, venue: u8, account: u8, slot: u32, gen: u32, px: i64, b0: i64, d0: i64, den: i64, cool_until: i64, n_fill: u32, fill_ens: 8 × i64, last_ens: i64, win: i64`;`Placing` 存为 None。恢复按 `GroupId`;波动率环不进快照(重启后重新积累 `vol_min_samples` 个样本才挂单)。

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `StrategyDesc` 与 `impl Strategy`,转发到 `fast` |
| `src/fast/mod.rs` | 订阅、规则、订单状态、快照 |
| `src/features.rs` | `MakerFeatures`:hft-feature 节点的组合与执行计划;规则 9 的方向预测、规则 10 的八个特征与分数 |
| `src/params.rs`、`src/desc.rs` | 参数 schema 与 `check`;空描述符 |
| `tests/` | 场景测试与 refdata 夹具 |

</details>

## 延迟与资源契约

每个事件 O(槽位数) 的整数运算,不分配(场景测试以计数分配器核对);`Model = NoModel`。状态每个槽位 64 B 以内,波动率环 `vol_window_s` × 8 B(默认 14.4 KB);规则 10、11 另有每个槽位约 4 KB 的逐秒环、参考逐秒环 2 KB、大单环 5 KB 与延迟统计约 9 KB。开启规则 10 时每个事件多读一遍各槽位的买一卖一,每 `comp_every_ms` 一次打分(两条腿各读 64 档、扫描 256 笔大单);开启规则 11 时每个事件读各槽位的新行情戳,每秒一次排序。

保留 `decided_tsc − proc_tsc` 的基线预算:参考 BBO 未变更挂单(tick_miss)与产生挂单(tick_fire)p50 ≤ 1 µs/p99 ≤ 3 µs(每次重算环上 σ),Order 更新 p50 ≤ 1 µs/p99 ≤ 3 µs,Risk p50 ≤ 100 ns/p99 ≤ 300 ns。参考机实测尚未完成,这些不是性能声明。

`benches/on_event.rs` 经真实 BinaryMaker::on_event 测量 tick_miss(两腿已在最优买价挂单,参考 BBO 无动作)、tick_comp(同 tick_miss,规则 10、11 开启且 `comp_every_ms` 0:每个事件重算两条腿的分数并读行情戳;暂定上限 p50 ≤ 5 µs/p99 ≤ 10 µs,开发机约 2.4 µs、tick_miss 约 0.6 µs)、tick_fire(同一参考 BBO,无挂单 → 两笔 post-only 买单)、exec_update(Up 挂单被撤 → 重挂)、risk_event(只读取上一事件的下单结果)。夹具为 tests/common 与场景测试 BMK-003 的 400 s 预热;每个样本前在计时外恢复挂单表,并在计时外核对一次挂单数。Criterion 默认配置,仅作相对比较。

所有订阅都是 Part::Fast,无慢侧缓存更新、Advice 或 TargetIntent;`benches/not-applicable.toml` 为 slow_update、advice、target_set 登记理由。

## 测试与验收

用例见 [TESTS.md](TESTS.md)。上线前按模拟盘提案(`.agents/notes/proposed/architecture/2026-10-20-polymarket-sim-maker.md`)在 `sim` 实例连续运行两天。

## 依赖

依赖 `hft-strategy-sdk` 与 [`hft-feature`](../../crates/hft-feature/README.md)(特征节点);测试另依赖 hft-testkit、hft-types、hft-refdata、hft-event(只在 `[dev-dependencies]`)。

## 延伸阅读

- [cookbook:新增一个策略](../../docs/cookbook/adding-a-strategy.md)
- 模拟盘与挂单策略提案:`.agents/notes/proposed/architecture/2026-10-20-polymarket-sim-maker.md`

## 已知限制与后续工作

- 只挂买单:持有的代币不卖出、不合并,持仓到结算;库存只由 `inv_max_shares` 限制。
- 返佣不计入;实盘挂单的排队位置与撤单竞争在模拟盘只能近似(hft-sim `maker_fill = Queue`)。
- 实盘时本实例自己的挂单会出现在簿上,"买一"可能就是自己;目标价仍按簿上的买一(含自己),只有规则 10 的失衡特征扣除自己的挂单(逐秒中间价与买一不扣,与研究移植相同)。过期复位(规则 8)后仍在簿上的旧挂单不再扣除。
- 规则 10 的逐秒记录按事件采样:交易所整簿(不投递 `Book` 事件)改变的簿在下一个事件记入,研究移植在整簿当时记入;一秒内的最后一个值只在整簿之后、同一秒再无事件时不同。
- 特征状态(波动率环、逐秒环、大单环、延迟统计)不进快照,重启后重新积累(`vol_min_samples` 秒内不挂单);写进快照要改快照布局与 `DESC_VERSION`,作为单独的版本变更。
- 规则 11 的样本来自本实例订阅的槽位 inst;每 inst 两个事件之间超过 16 条行情消息时多出的丢失(槽位 venue 的每条 L2 批与成交都是事件,只有整簿不是)。

### 开发备忘

<details><summary>给实现者</summary>

参数默认值取研究报告的起点值;模拟两天后按成交与盈亏调整。

</details>
