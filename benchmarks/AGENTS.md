# AGENTS.md — benchmarks

本子树拥有**跨 crate 的、按用户路径组织的**延迟门槛,以及单 crate 基准的上限表。单 crate 的 Criterion 基准留在各自的 `crates/<x>/benches/`。规格与门槛数字见 [README.md](README.md);分段名只用 [latency-budget.md](../docs/latency-budget.md#2-分段名全仓唯一) 的定义。

- **延迟门槛只在参考机上判定**。参考机是按需租用、与目标主机类同型的实例,从 M1.5 起每个里程碑结束时运行(规则见 [README](README.md#参考机与-ci));上线门槛 ⑩ 在升级主机上、`prod` 与 `sim` 同机运行时判定([testing §8](../docs/testing.md#8-上线门槛));不得把 dev2 或 CI runner 的数字写成门槛或基线。
- **按被测路径组织**,一条路径一个目录(`feed-ingest/`、`seq-decide/`、`gw-egress/`、`gw-ingest/`、`leg-link/`、`startup-cold/`、`journal-persist/`、`replay-throughput/`;可选扩展 `tick-to-trade/`);不镜像 crate 树。
- **走生产入口**。基准经公开 API 或完整的 `hft-core` 二进制驱动;不复制产品算法、不为测量新增生产导出、不在基准里重复语义断言(语义归测试)。需要策略时用 hft-testkit 的测试策略或已注册的生产策略,不为基准另写策略。
- **输入来自 `fixtures/`**:经脱敏、带哈希的真实报文与 journal 样本,或由已评审常量合成。不联网、不读生产 journal 的实时尾部、不读开发者本地文件。大样本按 S3 指针取并校验哈希。
- **测量边界写清**:每个基准在 README 表里写明起止时间点(用 latency-budget 的时间点名)、热身次数、样本数、绑核、分配器。
- **分位数用逐次采样**:路径基准用 `hft-testkit` 的 `HistBench`,报告 `n`、p50/p90/p99/p99.9/max;Criterion 只用于均值/中位数的相对比较。
- **参考机与 CI 分开**:CI 是共享的 GitHub 托管 runner,只跑 `bench-gate --smoke`(编译、短跑,时间与分配数值只报告),不判任何门槛、不做回归阻断;参考机按需开机、运行发布作业产出的基准制品,不是 CI runner。
- **基准名只登记一处**:每个基准名(Criterion id 或 `HistBench` 名)在 [README 基准名登记](README.md#基准名登记) 有一行,全仓唯一;新增、改名或删除基准时同一改动改登记表与对应 `TESTS.md` 的 `bench` 用例。
- **门槛是评审过的源常量**:路径门槛在 README 表与 `benchmarks/budgets.toml`,单 crate 预算在 `crates/<x>/benches/budgets.toml` 且不得宽于 README 的上限表。环境变量不得覆盖门槛。
- **报告足以解释裁决**:输出 JSON 带原始分位数、样本数、数据集维度(`HostProfile` 哈希、`host.class`、Git SHA、`target-cpu`、时间戳来源 HW/SW)与所用门槛类型(绝对/相对)。不同数据集不合并([latency-budget §5](../docs/latency-budget.md#5-分组与不同数据集不混算))。
- **进程级基准在干净环境里跑**:每个样本组新起进程、私有临时目录,结束后无论成败都清理;子进程有超时并等待退出。
- **共享支撑**:某路径专用的支撑代码放在该目录;至少两条路径需要同一行为时才移入 `benchmarks/support/`。
- **改门槛要有证据**:放宽任何门槛须在同一 PR 附参考机直方图,并在负责的 Agent Note 记录负载、计时边界、校准参考、备选方案与已知排除项。
