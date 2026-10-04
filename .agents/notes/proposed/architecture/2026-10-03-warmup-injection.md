# Agent Note: Model 检查点不随参数与发版失效;历史样本经 InjectWarmup 作为 Input 注入

Status: proposed

> **第八轮第四次修订(2026-10-03)**:补充——结构参数热更时 Model 按 group 重置并写 `Note{ModelGroupReset}`,pipeline 对这些 group 与新增的 group 自动预热;预热载荷是插件给出的块列表(每块 ≤ 4096 B、自成一体),客户端逐块发送、不切字节;见 [pipeline Model 预热](../../../../python/pipeline/README.md#model-预热)。

> **第八轮第五次修订(2026-10-03)**:收窄——Model 检查点分为原始样本节与带 `PATH_VERSION` 的路径状态节,见 [model-path-section](2026-10-03-model-path-section.md);pipeline 的预热回看窗口从当前生效的策略参数读取、不在 policy 里重复,按 (GroupId, 参数版本) 去重。依据 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) E6、E14(b)。

## Problem

策略的慢速部分 `Model`(168 h 分位、资金费统计等)在 advisor 线程运行,重启后靠检查点免于重新积累样本([strategy-sdk-decoupling](2026-10-02-strategy-sdk-decoupling.md))。修订后的规格里检查点格式在 SDK、hft-sequencer、hft-core 三处不一致,而且 hft-core 按参数版本、refdata 版本、二进制 SHA 丢弃检查点——日常的换参数、加 inst、发版都会让首个策略约 42 h 不开仓;也没有任何办法把历史数据交给 Model([独立评审](../../../../design/_work/review-2026-10-03-M0独立评审.md) P1-2)。用户批准通用的预热注入通道([round8-decisions](../../../../design/_work/round8-decisions.md) R8-8 第 3 条)。

## Proposal

检查点归 [hft-core](../../../../apps/hft-core/README.md);命令归 [control-plane](../../../../docs/subsystems/control-plane.md);Model 接口 `EventRef::Warmup` 归 [hft-strategy-sdk](../../../../crates/hft-strategy-sdk/README.md);生产者归 [pipeline](../../../../python/pipeline/README.md#model-预热);步骤归 [launch Model 预热](../../../../docs/runbooks/launch.md#model-预热)。

1. **检查点键**只含 `kind_id`、`Model::LAYOUT_VERSION`(策略声明的状态布局版本)、`schema.id`、`strategy_instance`、`last_seq`;参数版本、refdata 版本、二进制 SHA 不使检查点失效——参数变化随重放经 `model_on_params` 重新应用。配置 `advisor.model_checkpoint_ms`(默认 600 000);丢弃时写 `Note{ModelCheckpointDiscarded}`。检查点放冷盘,dev2 上两代合计 ≤ 1 GiB。
2. **注入通道**:控制命令 `InjectWarmup{instance, local_kind: u16, chunk_no: u32, last: bool, payload ≤ 4096 B}`(主体:Trader 或 pipeline;中性),作为 Input 记 journal,路由到该实例的 Model(可选也投递给快速部分);载荷格式由策略定义;回放确定。
3. **生产者**:pipeline 的策略插件从 collector 数据或交易所公共 REST(K 线、资金费历史)生成样本,`hftctl.inject_warmup` 分块注入,gateway 按 `warmup_rate_per_s` 限速;portfolio-carry 分 `FundingWarmup` 与 `BasisWarmup` 两种载荷。触发:新实例,或检查点缺失/被丢弃。

## Alternatives considered

**接受约 42 h 的冷启动。** 新实例上线可以接受,但每次策略状态布局变化、检查点损坏或换主机后都要停两天开仓;评审 P1-2。

**检查点按二进制 SHA 失效(修订前)。** 最保守,但每次发版都清空 Model,等于把 42 h 冷启动变成常态;状态布局是否兼容由策略的 `LAYOUT_VERSION` 明确声明更准确。

**advisor 直接从 S3/Parquet 读历史。** advisor 里出现 I/O 与外部数据,回放时读到的数据不一定相同,破坏"journal 即真相"与确定性回放。

**Python 计算 Model 状态后整体注入。** 要在 Python 里重写一遍 Model 逻辑,违背解耦与"一份实现";注入的是原始样本,由同一份 Rust Model 处理。

**只从自己的 journal 归档回填。** 新实例没有自己的历史;collector 的数据正是我们自己采集的行情,作为首选来源。

## Acceptance criteria

- 同一 `LAYOUT_VERSION` 的发版、换参数、refdata 变更后重启:检查点被载入,无 `ModelCheckpointDiscarded`,Advice 在重启后第一个周期即有效。
- `LAYOUT_VERSION` 变化 → 丢弃并写 Note;一次预热注入完成后,策略报告的样本数达到策略要求。
- 含 `InjectWarmup` 的 journal `replayer verify` 与 `--advice recompute` 逐字节一致。
- 600 块的注入在限速下全部 `Applied`,期间实盘事件的处理延迟不超出 ctl 批量上限的约束。

## Risks

- 预热样本与实时流口径不同(K 线收盘 vs BBO):由策略决定样本权重与是否区分来源。
- 载荷由 Python 插件写、Rust 策略读,两侧一致只靠 `fixtures/strategies/<name>/warmup/` 金样;格式变化提升载荷首字节的版本号(不换 `local_kind`,规则归 [hft-strategy-sdk](../../../../crates/hft-strategy-sdk/README.md))。
- 布局版本声明错误(状态变了而版本没变)会让旧检查点被错误载入:SDK 测试要求布局变化同时改版本,检查点头带校验。
