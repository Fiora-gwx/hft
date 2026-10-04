# Agent Note: journal 自足:配置内容作为事件写入

Status: proposed

## Problem

早期设计的段头只记 `config_hashes[]`([13 R6](../../../../design/13-设计评审与优化.md),原 08 §2)。回放需要的是配置**内容**:只有哈希,就依赖外部文件还在、没被改、能按哈希找到。这违背"状态 = f(journal)"——三个月后回放某天,refdata 已经改了几十个版本,流水线的参数文件早被覆盖。现有系统的"三参数源"与热加载的 JSON 文件正是这类问题的来源([research/00 §6.1](../../../../design/research/00-existing-codebase-inventory.md))。

## Proposal

[08 §1、§2、§5](../../../../design/08-数据面与回放.md):

1. 启动时与每次变更,把 refdata、`StrategyParams`、`RiskParams` 的**完整 blob** 作为 `ConfigBlob` 事件写进 seq journal(变长/多段记录,[journal-format](../../../../docs/subsystems/journal-format.md))。
2. 每个段首的 `StateSnapshot` 带当前生效的全部 blob(或引用同段内的 `ConfigBlob` 记录),使任一段可独立回放。
3. 段头的 `config_hashes[]` 只作索引;内容以 journal 里的记录为准。
4. 首条记录是 `HostProfile`(主机实际生效参数的摘要与比较结果),完整内容由 [hostprobe](../../../../tools/README.md#hostprobe) 采集并以 `ConfigBlob{HostManifest}` 写入,使延迟数据可按主机剖面分组。
5. 回放只读 journal;需要任何外部文件才能回放 = 缺陷(`replay-verify` 报"journal 不自足")。
6. 秘密不入 journal:blob 里不含 key,凭据只在启动期从 Secrets Manager/KMS 载入。

7. **记录分类影响自足的含义**:journal 记录分 Input / Output / Meta([hot-path-rules D5](../../../../docs/hot-path-rules.md#确定性d))。`HostProfile`、`ClockStatus`、`LatencySample`、`Note{Measure}` 是 Meta:它们让 journal 在**取证与测量**上自足,但不进入状态、不参与验证比对。何时快照、何时滚动这类依赖运行环境的决定由 live driver 写成 Input(`Timer{Snapshot|Roll|Keyframe}`),回放照读。
8. **对 `RecoverDriver` 的影响**:冷启动恢复只用 journal 里的 `ConfigBlob` 与快照,不读当前配置文件;因此段首快照必须引用当时生效的全部 blob。版本变化时 `RecoverDriver` 从最近快照起步、不重放尾部([recover-driver](2026-10-01-recover-driver.md))。refdata 变更 = 写 `ConfigBlob` 后 Drain + 重启([refdata-change-is-restart](2026-10-01-refdata-change-is-restart.md)),新进程的第一段即带新 blob。

## Alternatives considered

**只记哈希 + 内容寻址的配置仓库(S3)。** 多一个必须永久可用的外部依赖;离线回放、事故取证时要先保证两边一致,而 journal 本身已经是永久保存的东西(Parquet/S3)。

**只在启动时记一次完整配置。** 运行期经 `SetParams`/`ReplaceRefdata` 的变更会丢;从中间段开始回放需要从头读。

**记录配置文件的 diff。** 省空间,但从任意段开始回放要追溯全部 diff 链;refdata blob 本身不大(每次变更 KB 到 MB 级,相对 5–20 GB/天可忽略)。

## Acceptance criteria

- `replay-verify` 在一个没有任何配置文件的空目录里对生产 journal 逐字节一致。
- 从任意段开头单独回放,得到与连续回放相同的状态哈希。
- `fixtures check` 与一条专门测试断言 `ConfigBlob` 里没有凭据形态的字段。

## Risks

- 大 blob 写入在 sequencer 线程:`ConfigBlob` 由 ctl 环送达、在两个行情事件之间写入;多段写入的耗时计入 `decide` 分段,基准覆盖"变更配置时"的尾部。
- 旧 blob 的 schema 演进:blob 带版本,`hft-refdata` 保留旧版本读取能力。
