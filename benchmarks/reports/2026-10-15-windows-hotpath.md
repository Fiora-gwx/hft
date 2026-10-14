# Windows 开发机热点测量 · 2026-10-15

此报告用于定位与同夹具比较,不是参考机验收。预算全部保留。机器为 Windows x86_64、i7-13700KF(16 核/24 线程),与其他构建任务共用;未绑核或固定频率。Rust 1.97.1、LLVM 22.1.6,bench 剖面、fat LTO、codegen-units=1、默认 x86_64 CPU。每次 cargo 均经 memguard(6 GB),构建 jobs=1,串行运行,开始前要求可用 commit 内存至少 14 GB。

Criterion 预热 0.2 s、测量 0.5 s、10 个样本。下面是 Criterion slope 点估计(无 slope 时用 mean),单位 ns;不是逐事件 p50/p99。原始迭代次数、采样时间、均值、样本中位数和 95% 置信区间在 [JSON](2026-10-15-windows-hotpath.json)。短样本、系统调度和其他任务可能造成波动;未修改组件的波动不算优化收益。预算来源逐项存于 JSON,主机资格与正式验收归 [基准规则](../README.md#参考机与-ci)。

“原始”使用修改前代码与原测试框架;“关日志”仅关闭 sequencer 测试策略的 Debug 格式化/字符串日志,仍使用修改前生产代码;“优化后”使用同一安静框架与本补丁。“变化”以关日志后的数值为 sequencer 对照,其余以原始为对照。不得把关日志收益计作生产优化。

## 证据与修改

- 分配计数在重复的 70 条同品种 BBO 场景测到每批 160 次引擎分配。复用合并槽、feed staging 和环境查询后剩 117 次;临时阶段计数定位为每条实际交付 BBO 的三个订阅者各一次 CSR Vec 复制。借用 CSR 行后四批均为零。最终更新号、CONFLATED 与回报顺序由测试检查。
- feed 重试的引擎分配从 3 次变为零;合成 RingFull 回报按值排队,不再创建并转移 InputSlot 的 Vec。测试 journal 每记录一次 body 分配单独扣除,不声称整个测试进程零分配。
- 执行层原来对 Live、Sim 各扫描完整 cell 表。现在按待处理位图枚举置位项,保留 Live 优先、cix 顺序和 Continue 剩余工作。1024-cell 基准在修改前后均为同一个双腿在途目标,只 touch 一个 cell;跨 word 0/63/64/129 的回归检查有限与充足容量的决策/通知一致。
- 增加 Bitget UTA spot/perpetual books1 解码基准与分类基准,复用现有合成验收报文。构造报文、加载 refdata、reserve sink 均不在计时内。
- 没有资金、风控、订单或恢复语义变更;没有放宽预算。

## 测量边界

sequencer 使用 B0/TestIo/MemJournal:HashMap 队列、逐记录分配、三个测试策略、处理后的空闲轮与测试日志容器仍在计时路径内。它不是生产 SPSC/RAM journal 的组件预算边界。head_check 测整轮 idle,表中的 50/300 ns 仅是 R=10 的预算展开;conn_down 的 B0 品种数也不是预算中的 1024。notify_continue 当前夹具排入的是 64 个 AdvisorTick,没有制造 64 条积压通知;l2batch_20 的 B0 未配置 Live L2,不能用于真实 L2 应用验收。这些已有夹具缺口列作后续工作,保留原样比较。

执行层的 slice/force 基准包含 cell 状态恢复与输出清理,force_step_8 不是单个 cell 的成本。book_snapshot_sections 使用 400 个 L2 簿(各一档双侧)与 2000 个 Top。journal_end_event 测 append + end_event,需与 append 基准比较,不能将整行直接判为 end_event 超预算;publish/tailing_reader 使用轮询读者。解码器和策略使用现有合成夹具与测试视图;小于预算的 Criterion 均值也不证明参考机 p99 达标。

## 全量对照

预算标注 p50/p99 或单值上限;“—”表示没有同阶段数据或没有对应预算。吞吐单独用 M events/min 表示。

| 基准 | 预算(ns) | 原始(ns) | 关日志(ns) | 优化后(ns) | 变化 |
|---|---:|---:|---:|---:|---:|
| `bench_classify_bitget` | p50 30 / p99 60 | 2.30 | — | 2.84 | +23.5% |
| `bench_classify_usdm` | p50 20 / p99 50 | 4.88 | — | 4.71 | -3.5% |
| `bench_decode_bitget_books1` | p50 400 / p99 800 | 360.91 | — | 331.90 | -8.0% |
| `bench_decode_bitget_books1/spot` | p50 400 / p99 800 | 348.40 | — | 381.83 | +9.6% |
| `bench_decode_bookticker` | p50 400 / p99 800 | 208.11 | — | 221.55 | +6.5% |
| `bench_decode_spot_bookticker` | p50 400 / p99 800 | 286.00 | — | 178.81 | -37.5% |
| `bench_peek_bookticker` | p50 80 / p99 150 | 155.20 | — | 155.10 | -0.1% |
| `book_batch_20` | p50 250 / p99 600 | 84.68 | — | 90.44 | +6.8% |
| `book_best` | 3 | 4.13 | — | 1.75 | -57.7% |
| `book_delta_best_remove` | p99 100 | 32.68 | — | 34.87 | +6.7% |
| `book_delta_inwin` | p50 15 / p99 40 | 15.51 | — | 15.23 | -1.8% |
| `book_delta_ovf` | p99 150 | 52.77 | — | 22.40 | -57.6% |
| `book_export_200` | 3000 | 3,658.90 | — | 2,497.05 | -31.8% |
| `book_iter_20` | 100 | 60.99 | — | 52.11 | -14.6% |
| `book_recenter_2048` | p99 10,000 | 8,412.67 | — | 5,253.29 | -37.6% |
| `book_snapshot_1000` | p99 30,000 | 17,268.80 | — | 8,075.33 | -53.2% |
| `book_snapshot_sections` | p99 40,000 | 10,387.94 | — | 8,152.30 | -21.5% |
| `book_snapshot_topn20` | p50 1,000 / p99 3,000 | 462.04 | — | 255.09 | -44.8% |
| `book_top_apply` | p50 8 | 4.28 | — | 2.22 | -48.1% |
| `exec/drain_ready_64` | p50 200 / p99 600 | 380.19 | — | 199.97 | -47.4% |
| `exec/force_step_8` | p50 1,000 / p99 3,000 | 12,334.86 | — | 5,624.79 | -54.4% |
| `exec/force_ungrouped_512` | p50 5,000 / p99 15,000 | 6,663.11 | — | 3,191.27 | -52.1% |
| `exec/on_external` | p50 60 / p99 150 | 74.77 | — | 29.02 | -61.2% |
| `exec/on_link` | p50 40 / p99 100 | 9.65 | — | 4.65 | -51.9% |
| `exec/on_risk_64` | p50 300 / p99 1,000 | 119.79 | — | 94.85 | -20.8% |
| `exec/pair_lot` | p50 30 / p99 80 | 91.82 | — | 47.12 | -48.7% |
| `exec/price_cap` | p50 10 / p99 25 | 65.11 | — | 31.07 | -52.3% |
| `exec/recovery_end_1024` | 200000 | 23,553.79 | — | 10,436.03 | -55.7% |
| `exec/resolve_broken` | p50 30 / p99 80 | 47.36 | — | 21.96 | -53.6% |
| `exec/set_target` | p50 80 / p99 200 | 241.77 | — | 114.27 | -52.7% |
| `exec_step/auto_fix` | p50 200 / p99 500 | 395.38 | — | 196.13 | -50.4% |
| `exec_step/hold` | p50 60 / p99 150 | 120.44 | — | 100.52 | -16.5% |
| `exec_step/hold/1024_cells` | p50 60 / p99 150 | 1,676.83 | — | 160.18 | -90.4% |
| `exec_step/idle` | p50 8 / p99 20 | 2.01 | — | 2.01 | -0.1% |
| `exec_step/slice_pair` | p50 300 / p99 800 | 733.03 | — | 737.46 | +0.6% |
| `journal_append_80b` | p50 40 / p99 100 | 21.09 | — | 24.62 | +16.7% |
| `journal_append_raw_512b` | p50 100 / p99 250 | 146.79 | — | 144.96 | -1.2% |
| `journal_append_raw_512b/reserve_var` | p50 100 / p99 250 | 173.46 | — | 183.30 | +5.7% |
| `journal_end_event` | 3 | 40.29 | — | 25.28 | -37.3% |
| `journal_publish` | 5 | 1.41 | — | 0.64 | -54.7% |
| `journal_publish/tailing_reader` | p99 150 | 1.42 | — | 0.64 | -55.1% |
| `oms/cancel_resting_8` | p50 300 / p99 1,000 | 1,009.93 | — | 603.85 | -40.2% |
| `oms/continue_16` | p50 2,000 / p99 5,000 | 981.23 | — | 374.88 | -61.8% |
| `oms/external_cash_guard` | p50 150 / p99 400 | 49.33 | — | 31.50 | -36.1% |
| `oms/foreign_cash_seen` | p50 150 / p99 400 | 34.15 | — | 32.09 | -6.0% |
| `oms/hedge_gate_open_64` | p50 3,000 / p99 10,000 | 2,348.64 | — | 1,341.92 | -42.9% |
| `oms/hedge_gated` | p50 30 / p99 80 | 21.71 | — | 10.01 | -53.9% |
| `oms/leg_adjust` | p50 30 / p99 80 | 46.66 | — | 22.63 | -51.5% |
| `oms/leglink_onfill` | p50 150 / p99 400 | 284.04 | — | 267.10 | -6.0% |
| `oms/on_event_ack` | p50 100 / p99 300 | 92.39 | — | 104.45 | +13.1% |
| `oms/on_event_fill` | p50 100 / p99 300 | 102.09 | — | 104.34 | +2.2% |
| `oms/place` | p50 150 / p99 400 | 47.56 | — | 49.29 | +3.6% |
| `oms/price_hedge` | p50 15 / p99 40 | 59.84 | — | 29.67 | -50.4% |
| `oms/reconcile_cash_guard` | p50 500 / p99 2,000 | 106.89 | — | 93.64 | -12.4% |
| `oms/recovery_end_4096` | 300000 | 847,169.87 | — | 903,796.36 | +6.7% |
| `oms/remember_external_cash` | p50 300 / p99 800 | 76.70 | — | 50.00 | -34.8% |
| `oms/simultaneous_preview` | p50 50 / p99 150 | 36.19 | — | 42.48 | +17.4% |
| `oms/smoke` | p50 300 / p99 1,000 | 367.18 | — | 202.65 | -44.8% |
| `oms/sweep_256` | p50 3,000 / p99 8,000 | 3,844.58 | — | 1,675.32 | -56.4% |
| `oms/tomb_late_fill` | p50 120 / p99 350 | 101.63 | — | 43.59 | -57.1% |
| `oms/unknown_query` | p50 80 / p99 200 | 34.72 | — | 15.55 | -55.2% |
| `oms/write_off` | p50 50 / p99 150 | 102.17 | — | 55.28 | -45.9% |
| `risk/check_mode` | p50 2,000 / p99 5,000 | 9.62 | — | 4.82 | -49.9% |
| `risk/check_pair` | p50 560 / p99 1,000 | 708.74 | — | 712.60 | +0.5% |
| `risk/check_pass_hedge` | p50 150 / p99 400 | 159.69 | — | 176.72 | +10.7% |
| `risk/check_pass_linked_open` | p50 280 / p99 500 | 184.04 | — | 198.72 | +8.0% |
| `risk/check_pass_open` | p50 200 / p99 500 | 116.57 | — | 125.61 | +7.8% |
| `risk/check_reject_kill` | p50 30 / p99 80 | 14.33 | — | 14.02 | -2.1% |
| `risk/classify` | p50 15 / p99 40 | 2.11 | — | 2.29 | +8.4% |
| `risk/effective` | p50 5 / p99 15 | 7.65 | — | 7.68 | +0.3% |
| `risk/hedge_px_limits` | p50 5 / p99 15 | 2.41 | — | 1.01 | -58.1% |
| `risk/leverage` | p50 5 / p99 15 | 7.89 | — | 5.30 | -32.8% |
| `risk/mode_apply` | p50 2,000 / p99 5,000 | 4,248.57 | — | 4,608.54 | +8.5% |
| `risk/on_account_cfg` | p50 5,000 / p99 15,000 | 1,651.55 | — | 820.45 | -50.3% |
| `risk/on_legs` | p50 15 / p99 40 | 10.76 | — | 5.66 | -47.4% |
| `risk/on_margin` | p50 20 / p99 60 | 21.29 | — | 12.88 | -39.5% |
| `risk/on_pnl` | p50 20 / p99 60 | 9.92 | — | 9.52 | -4.0% |
| `risk/view_for` | p50 5 / p99 15 | 3.54 | — | 1.92 | -45.9% |
| `seq/advice_route` | p50 300 / p99 800 | 15,487.26 | 5,410.01 | 4,076.72 | -24.6% |
| `seq/bbo_exec_fire` | p50 2,000 / p99 5,000 | 9,607.19 | 3,702.93 | 2,963.70 | -20.0% |
| `seq/bbo_fire` | p50 2,000 / p99 5,000 | 17,520.45 | 6,973.04 | 5,121.02 | -26.6% |
| `seq/bbo_miss` | p50 250 / p99 600 | 10,592.81 | 4,897.15 | 4,513.30 | -7.8% |
| `seq/bbo_target_idle` | p50 350 / p99 800 | 11,070.63 | 5,571.54 | 4,091.88 | -26.6% |
| `seq/bbo_view` | p50 15 / p99 40 | 12.89 | 7.88 | 6.45 | -18.1% |
| `seq/conn_alive` | p50 80 / p99 250 | 6,862.77 | 4,158.30 | 4,584.49 | +10.2% |
| `seq/conn_down` | p50 3,000 / p99 8,000 | 5,198.21 | 2,850.68 | 2,232.86 | -21.7% |
| `seq/external_fill` | p50 2,000 / p99 5,000 | 18,149.32 | 8,955.86 | 7,991.96 | -10.8% |
| `seq/fill_to_hedge` | p50 1,500 / p99 4,000 | 17,583.38 | 8,097.45 | 6,462.09 | -20.2% |
| `seq/head_check` | p50 50 / p99 300 | 672.45 | 484.10 | 500.92 | +3.5% |
| `seq/inject_warmup` | p50 2,000 / p99 5,000 | 10,873.77 | 5,888.44 | 7,152.60 | +21.5% |
| `seq/l2batch_20` | p50 1,000 / p99 3,000 | 8,206.36 | 5,795.46 | 4,200.20 | -27.5% |
| `seq/margin_report` | p50 2,000 / p99 5,000 | 6,463.77 | 4,964.65 | 4,128.47 | -16.8% |
| `seq/notify_continue` | p50 10,000 / p99 30,000 | 34,713.97 | 22,077.33 | 20,301.87 | -8.0% |
| `seq/overhead_per_event` | p50 150 / p99 400 | 3,969.57 | 4,031.91 | 3,474.67 | -13.8% |
| `seq/recon_funding_item` | p50 600 / p99 1,500 | 7,250.26 | 4,283.45 | 3,325.06 | -22.4% |
| `seq/recon_group` | p50 15,000 / p99 40,000 | 143,112.27 | 84,754.67 | 69,100.79 | -18.5% |
| `seq/snapshot_keyframe` | p50 150,000 / p99 300,000 | 57,559.07 | 27,646.97 | 26,895.65 | -2.7% |
| `seq/strategy_cmd` | p50 500 / p99 1,200 | 10,415.07 | 5,894.30 | 5,356.82 | -9.1% |
| `strategies/cross-taker/exec_update` | p50 80 / p99 250 | 26.28 | — | 25.49 | -3.0% |
| `strategies/cross-taker/risk_event` | p50 150 / p99 400 | 48.82 | — | 27.43 | -43.8% |
| `strategies/cross-taker/tick_fire` | p50 300 / p99 1,000 | 279.35 | — | 290.15 | +3.9% |
| `strategies/cross-taker/tick_miss` | p50 15 / p99 40 | 186.40 | — | 192.11 | +3.1% |
| `strategies/portfolio-carry/exec_update` | p50 80 / p99 250 | 200.69 | — | 110.15 | -45.1% |
| `strategies/portfolio-carry/hold` | p50 60 / p99 150 | 215.34 | — | 116.40 | -45.9% |
| `strategies/portfolio-carry/risk_event` | p50 150 / p99 400 | 45.01 | — | 22.81 | -49.3% |
| `strategies/portfolio-carry/target_set` | p50 250 / p99 800 | 247.90 | — | 254.49 | +2.7% |
| `strategies/portfolio-carry/tick_fire` | p50 300 / p99 1,000 | 239.18 | — | 248.14 | +3.7% |
| `strategies/portfolio-carry/tick_miss` | p50 15 / p99 40 | 109.25 | — | 122.19 | +11.8% |

| 吞吐 | 最低目标(M/min) | 原始 | 关日志 | 优化后 |
|---|---:|---:|---:|---:|
| `seq/replay_throughput` | 20 | 9.38 | 70.76 | 91.74 |
| `seq/mirror_throughput` | 20 | 9.42 | 64.47 | 102.60 |

## 同核重点复测

主表中未改代码的函数也出现接近两倍的差异,因此重点对照另固定到逻辑 CPU 0(affinity mask=1),正常优先级,预热 0.5 s、测量 2 s、20 个样本(回放组代码固定 10 个)。只回退本工作树的生产源码补丁,保留完全相同的安静测试框架;测完恢复补丁。此组比较仍不是参考机验收。

| 基准 | 修改前(ns) | 修改后(ns) | 变化 |
|---|---:|---:|---:|
| `exec/force_step_8` | 5,839.95 | 6,166.04 | +5.6% |
| `exec_step/hold/1024_cells` | 1,672.92 | 171.40 | -89.8% |
| `exec_step/slice_pair` | 814.75 | 764.13 | -6.2% |
| `seq/bbo_miss` | 4,226.43 | 4,116.00 | -2.6% |
| `seq/head_check` | 503.07 | 452.47 | -10.1% |
| `seq/replay_throughput` | 15,629,632.03 | 12,802,290.39 | -18.1% |

## 验证与后续

本地 hft-exec 行为测试 279 项、hft-sequencer 行为测试 474 项及分配测试、venue-bitget 行为测试 72 项及零分配测试通过。回放比较包含在 sequencer 真实引擎测试中。独立 DST 的 core、modes、storage 三个套件通过,每个场景 2 个 seed、每个 seed 2,000 步;未重建 HashTrail golden。PR CI 结果见 PR 描述。

负责人仍需选择/提供统一 aarch64 参考主机及实盘等价负载,按既定直方图口径验收。force_step、OMS 的批量撤单/恢复、风险模式切换等剩余高成本路径需要参考机 CPU 采样继续分解;本报告未把开发机均值当作预算放宽依据。M3 全量混合 Input 零分配、生产环/RAM journal 的精确分段、真实 L2/通知积压夹具仍待补齐。策略源代码不在此引擎补丁内。

复现时在可用 commit 内存至少 14 GB 后,用仓库外 memguard 包装单个 `cargo bench -p <package> --bench <target> -- --warm-up-time 0.2 --measurement-time 0.5 --sample-size 10 --save-baseline <name>`;journal append 需 `--features writer,mem-backend`。所有目标串行,LimitGB=6。参考主机使用仓库规定的 CI 制品流程。


## BBO 逐阶段诊断

本节是 `perf-bbo` 的独立复测,基准代码基于 `a2247f944ac10b664bf6c2e7925f5e18f01aca3f`。机器与工具链同上,本节所有测量进程固定到逻辑 CPU 0,仍与其他任务共机,不固定频率。逐次数据、二进制及源码摘要、Criterion 原始样本见 [BBO JSON](2026-10-15-bbo-stages.json)。预算未修改,这些数值不构成参考机验收。

原来的约 4 µs 有明确的测试框架成分。`MemJournal::publish` 每次从头遍历已经发布的历史以求字节和,单次成本随历史长度增长;Criterion 的迭代数会改变测量负载。20,000 次固定样本中,该阶段 p50 为 4,284 ns、p99 为 8,421 ns。真实 RAM `JournalWriter` 对应轮末发布为 23 / 25 ns。修复在 append 和事件边界维护累计字节,发布 O(1);截尾后重算保留前缀。不能把此收益算成生产 journal 优化。

生产代码的改动是参考报价只重估受影响的权益数值(零持仓仍更新汇率过期标志与时间),连续 BBO 复用没有失效的 Feeds 检查,按 inst 刷新 group 视图,按当前接收者集合投递策略。后两项针对规模增长;B0 三个订阅者都命中同一 inst,其阶段成本没有显著改善。连接/参数变化和恢复使派生缓存失效;不更改 journal、资金、风控、订单与回放契约。

### 计时边界

`bbo_stages` 通过 opt-in `bench-stages` feature 在真实处理函数之间插入 `hft_rt::tsc_ordered`。普通构建完全去掉计时状态和调用。硬件计数器频率 3,417,600,000 Hz;连续两次读取 p50 / p99 为 8 / 9 ns,表内不扣此成本。每个阶段记录原始间隔,各阶段分位数不可相加。开启与关闭计时分别采样,最终预算比较看关闭计时的整段值;该诊断二进制关闭采样时仍有 feature 内的未激活分支,另外的 Criterion 二进制未启用此 feature。

miss 热身 2,000 次、采样 20,000 次;hit 热身 100 次、采样 1,000 次,每次使用新的可接纳订单状态,初始化和析构不计时。使用 std 分配器。数据为现有 SEQ B0 合成常量,三实例脚本策略保持安静模式,策略钩子仍有 TLS/动态调用框架成本。无外部网络、账户或生产 journal。

三个边界分开报告。TestIo 使用原 HashMap 队列与 MemJournal;RAM 轮次使用真实 SPSC、LiveDriver 和预备好的 16 MiB RAM JournalWriter,包含一次处理轮与其后的 idle 轮;RAM 直接入口从已填好的 InputSlot 开始,走同一个 process 与发布函数。交易所报文解码属于 `feed-ingest`,这些 BBO 已归一化,不能从该表推算 JSON 解码时间。样本不触发滚段、持久化或定时器到期,也没有真实网关和遥测消费者。决策时间取固定的已记录夹具时间,测量计数器不反馈到引擎。

### 未插入阶段读取的整段结果

单位 ns,单元格为 p50 / p99。

| 边界 | 修改前 | 修改后 |
|---|---:|---:|
| TestIo `seq/bbo_miss` | 6,054 / 10,093 | 1,473 / 1,657 |
| TestIo `seq/bbo_hit` | 4,929 / 10,989 | 4,790 / 6,940 |
| RAM 完整轮次 `seq/bbo_miss_ram` | 900 / 1,107 | 764 / 1,275 |
| RAM 已解码 Input → 发布 | 502 / 678 | 365 / 638 |

RAM 直接入口 p50 降低 27.3%,为 250 ns 预算的 1.46 倍;p99 638 ns 仍高于 600 ns。完整轮次 p50 降低 15.1%,但 p99 从 1,107 升到 1,275 ns,不能声称尾延迟全面改善。测试框架下 hit 的改善很小,未达到其 2 µs / 5 µs 预算。共享 Windows 核仍有明显长尾,最终 JSON 保留 p99.9 与 max,不裁掉中断样本。

### 阶段表

按实际执行顺序,journal 必须先于状态更新。单位 ns,单元格为 p50 / p99;下面均开启阶段读取。

| 内部阶段 | TestIo 前 | TestIo 后 | RAM 直接入口前 | RAM 直接入口后 |
|---|---:|---:|---:|---:|
| 取环/轮调度及 idle(直接入口仅计外层) | 1,135 / 1,382 | 1,097 / 1,200 | 21 / 21 | 21 / 22 |
| 输入 journal append / 编码 | 74 / 238 | 64 / 230 | 40 / 214 | 39 / 213 |
| kind 分派与引擎时间 | 34 / 40 | 34 / 38 | 40 / 42 | 40 / 42 |
| 读取 Bbo / Top 更新 | 22 / 26 | 18 / 22 | 16 / 24 | 17 / 20 |
| ledger 参考价、PnL 与相关 risk | 234 / 299 | 122 / 141 | 206 / 234 | 112 / 143 |
| Feeds 就绪及模式检查 | 93 / 106 | 22 / 33 | 74 / 94 | 22 / 26 |
| group 视图刷新 | 56 / 62 | 52 / 61 | 45 / 55 | 49 / 67 |
| 策略 fan-out / on_event / Ctx 应用 | 168 / 206 | 165 / 189 | 134 / 157 | 146 / 183 |
| exec.advance(含其 OMS 交互) | 12 / 14 | 11 / 16 | 12 / 13 | 12 / 14 |
| 通知后检查、订阅/Drain | 26 / 36 | 26 / 38 | 26 / 37 | 26 / 34 |
| 输出、结束事件与 kill 发布 | 14 / 18 | 14 / 16 | 20 / 23 | 20 / 25 |
| 轮末 journal publish | 4,284 / 8,421 | 26 / 30 | 12 / 13 | 12 / 13 |

RiskView/ModeView 是借用适配器,没有独立的全状态重建步骤。miss 不下单,意图检查链不会被调用;hit 的 evaluate/commit、OMS 分配和订单决策计入 Ctx 应用阶段。hit 的完整阶段分位数与 RAM 完整轮次分位数保存在 JSON。RAM 输入 append 的 p50 约 40 ns,不是主要瓶颈;seq journal 的普通 BBO 按原有 CRC 策略写入,没有关闭校验、哈希或提交步骤。定时器无到期样本只衡量轮调度的检查,不代表堆操作的峰值成本。

### Criterion 与回放对照

相同的原 `seq` 基准,未启用阶段 feature;预热 0.5 s、测量 2 s,miss/hit 20 个样本,回放 10 个样本。表中为 slope 点估计,不是逐事件分位数;`seq/bbo_hit` 是本诊断对原 `seq/bbo_fire` 的称呼。

| 基准 | 修改前 | 修改后 | 变化 |
|---|---:|---:|---:|
| `seq/bbo_miss` | 3.928 µs | 1.591 µs | -59.5% |
| `seq/bbo_hit` | 5.814 µs | 5.024 µs | -13.6% |
| `seq/replay_throughput` | 89.147 M events/min | 136.461 M events/min | +53.1% |

回放每轮含 20,000 个 BBO 的日志克隆、从关键帧恢复和逐字节验证;修改后超过 20M events/min 预算。miss 原测量有历史长度偏差,其 59.5% 改善是测试框架与生产改动的合计,生产贡献以 RAM 对照为准。

当前剩余成本集中在 ledger 与脚本策略投递,其次是完整轮次的取环/idle。尚未证明 250 / 600 ns 门槛达标,也未证明 365 ns 是物理下限。下一步是在合格 aarch64 参考机上运行 CI 产物,使用生产策略、生产装配和同机 sim 负载验收;不据本报告放宽预算。该任务不含资金或风控语义变更,没有待评审语义条目。

本地验证通过 sequencer 476 项、独立分配检查 3 项、ledger 207 项,以及两 crate 全 targets 的 Clippy(含 `bench-stages`,warnings 为错误)。测试覆盖填单后视图与 ledger 相等、冷恢复、连接成员变化、字节发布边界、汇率过期和零/非零库存。回放由已有 VerifyDriver 逐字节比对,PR fast lane 继续执行 DST 与质量门。构建诊断目标使用 `cargo bench -p hft-sequencer --bench bbo_stages --features bench-stages --no-run -j 1`,在本机须按 agent 规则经 6 GB memguard 串行执行;编译后单独运行生成的 exe,并设进程 affinity mask 为 1。原基准目标为 `seq`,过滤表达式 `seq/(bbo_miss|bbo_fire|replay_throughput)$`,参数 `--bench --warm-up-time 0.5 --measurement-time 2 --sample-size 20`。
