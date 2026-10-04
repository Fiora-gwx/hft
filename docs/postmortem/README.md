# 事故索引

每起事故一行:日期(UTC)、名称、根因一句、对应的防御规则、原始复盘。这些事故都发生在现有系统(`Trading` 采集仓库与 `cdex-arb` 引擎)上;hft 的设计把每一类变成"结构上不可能再发生"的约束([architecture-v2/12 §1](../../design/12-风险与未决问题.md))。规则只在 [defensive-patterns.md](../defensive-patterns.md) 维护;本目录是全仓唯一允许写事故经过的地方。链上类事故(公共 RPC 403、幻影成交、nonce)的规则落在 optional 的链上组件里,cdex 迁到新框架时生效;在此之前 cdex 在旧框架运行,这些风险仍在旧系统中。新框架自身的事故(含 dev2 护栏因 hft 自身用量触发的停机(同一周第二次另须写明是否启用独立主机的退路;只告警的不算)、Broken 自动平仓失败 `ExecAutoFixFailed` 或同一周 ≥ 3 次自动平仓、冷启动续接对冲超时、L3/flatten 退回串行减仓、老板 `/stop` 失败或经 kill-switch 兜底(只撤单或 `stop_core`)、某把 key 的 UID 与 `venue_uid` 不符、MarginStress L2 及以上、交易主机失联、sim 实例干扰 prod(隔离核闯入、网卡或出口地址错用)、模拟账户暴露的引擎缺陷;处置见 [incident-response](../runbooks/incident-response.md))按下文流程新增。dev2 上测试网联调的异常不涉及真实资金,只在暴露引擎缺陷时写。portfolio-arb 雏形的已知缺陷不是事故,记录在[主策略适配评审](../../design/_work/review-2026-10-02-主策略适配与执行正确性.md) §6,新框架不照搬。

## 索引

| 日期 | 名称 | 根因 | 防御规则 | 原始复盘 |
|---|---|---|---|---|
| 2026-08(多日) | 四小时发布/捕获同步 | 数据传输、CI、发布、清理几条流程叠在一次维护里,CI 与 ARM64 构建重复运行,S3 逐对象删除 | [development:CI 不重复跑](../development.md#ci-概要) | Trading/AGENTS.md(本仓库之外:`Trading/AGENTS.md`) |
| 2026-08-30 | v0.2.4 发布资产缺失 | GitHub Actions 作业条件里隐式 `success()` 遍历整条 `needs` 链,有意跳过的上游让下游全部静默跳过,工作流仍显示 success | [CI 条件显式判定上游结果](../defensive-patterns.md#ci-条件显式判定上游结果) | Trading/AGENTS.md(本仓库之外:`Trading/AGENTS.md`) |
| 2026-08-31 | 丢失一天采集 | `install -d -o USER deep/path` 只给最深一级属主,父目录属 root → EACCES;验证全以 root 运行;`Requires=` 拉起坏服务 crash-loop 57,915 次 | [文件系统所有权](../defensive-patterns.md#文件系统所有权逐级创建单一属主以服务用户演练) | Trading/AGENTS.md(本仓库之外:`Trading/AGENTS.md`) |
| 2026-09-06/07 | RuntimeDirectory 互抢 | 两个单元都声明 `RuntimeDirectory=trading`,systemd 每个 `Exec*` 递归改属主,opsd 的 socket 被夺 → Telegram 运维面断开 | [文件系统所有权](../defensive-patterns.md#文件系统所有权逐级创建单一属主以服务用户演练) | analysis/PR-fix-runtime-dir-ownership.md(本仓库之外:`analysis/PR-fix-runtime-dir-ownership.md`)(修复 v0.3.4) |
| 2026-09-07 | CXMT 分片 crash-loop | 目录刷新定时器从未运行,OKX `price_tick` 已从 0.01 改为 0.001 而本地仍是旧值 | [参考数据有版本](../defensive-patterns.md#参考数据有版本刷新不能静默丢标的) | analysis/PR-fix-runtime-dir-ownership.md(本仓库之外:`analysis/PR-fix-runtime-dir-ownership.md`)("Not in this PR" 一节) |
| 2026-09-07/08 | 84% L2 段隔离 | 写者在 500 MiB 自行滚动,新段以增量开头、无快照,无法独立回放(104 段中 87 段隔离,约 250 个包待多段 seed 恢复) | [段滚动必须带快照](../defensive-patterns.md#段滚动必须带快照) | analysis/PR-size-rotation.md(本仓库之外:`analysis/PR-size-rotation.md`)(修复 v0.3.5);[research/06 §2.5](../../design/research/06-data-plane-replay-observability.md) |
| 2026-09-10 | cdex 权限模型缺失 | Telegram 只按 `chat.id` 白名单、无只读角色;启动器 `dry_run = false` 写死;缺 Ed25519 key 时 Rust 引擎退出而 Python 侧看似正常 | [权限按人与角色](../defensive-patterns.md#权限按人与角色运行模式是被记录的命令) | analysis/cdex-arb-deploy-plan.md(本仓库之外:`analysis/cdex-arb-deploy-plan.md`) |
| 2026-09-15 | BTCUSDC 采集 5 分钟后丢失 | 周期刷新只接受 USDT 计价,`live = refreshed ∩ planned` 把计划内的 USDC 标的永久剔除 | [参考数据有版本](../defensive-patterns.md#参考数据有版本刷新不能静默丢标的) | analysis/btc-24h-20260915/PR-non-usdt-quote.md(本仓库之外:`analysis/btc-24h-20260915/PR-non-usdt-quote.md`)(修复 v0.3.6) |
| 2026-09-20 | AIN 插针平仓 | 1 s 单点采样触发,revert 后盲重试不复核;研究用 1 h 收盘价看不见秒级尖峰 | [采样粒度匹配执行粒度](../defensive-patterns.md#采样粒度匹配执行粒度重试是新意图) | analysis/DEX加速与AIN插针复盘.md(本仓库之外:`analysis/DEX加速与AIN插针复盘.md`) |
| 2026-09(日期待核实,方法:查 cdex-arb 引擎日志与 Telegram 告警记录中 JCT 那次重试的时刻) | JCT 重试用旧价 | 重试沿用上一次意图的价格 | [采样粒度匹配执行粒度](../defensive-patterns.md#采样粒度匹配执行粒度重试是新意图) | [architecture-v2/06 §6](../../design/06-链上执行层.md)(未单独成文) |
| 2026-09-23 | `recommend()` 烧 2 核 7.5 h | 从 0.5% 按 0.1% 步长扫到序列最大值,同名假币 10⁵ % 价差 → 几十亿次循环;流水线与扫描器 8 h 无进展无人发现 | [所有循环有界](../defensive-patterns.md#所有循环有界看门狗看的是进展) | analysis/cdex-自动化流程核验-2026-09-23.md(本仓库之外:`analysis/cdex-自动化流程核验-2026-09-23.md`) |
| 2026-09-23 | ARC 幻影成交(Solana) | 只看确认不看 `meta.err`,余额差为 0 时退回报价当成交,随即平掉币安腿 → 82 ARC 裸多 | [成功要有正面证据](../defensive-patterns.md#成功要有正面证据报价不是成交) | analysis/cdex-自动化流程核验-2026-09-23.md(本仓库之外:`analysis/cdex-自动化流程核验-2026-09-23.md`) |
| 2026-09-23/24 | 公共 RPC 全部 403 五小时 | 两台引擎 × 4 个公共节点并行 multicall,每秒几百次 swap 模拟,无速率治理 | [外部端点有速率治理](../defensive-patterns.md#外部端点有速率治理权威源自建) | analysis/cdex-价差引擎影子试跑-2026-09-24.md(本仓库之外:`analysis/cdex-价差引擎影子试跑-2026-09-24.md`) |
| 2026-09-24 | 交易节点 403 → 裸多 | 发送节点返回 403,已上链的交易被当作失败,引擎按失败处理 | [传输错误 ≠ 业务失败](../defensive-patterns.md#传输错误--业务失败) | 未单独成文:[research/00 §5.3](../../design/research/00-existing-codebase-inventory.md)、[architecture-v2/12 §1](../../design/12-风险与未决问题.md) |
| 2026-09-25 | NAORIS 滑点保护失效 | `min_out` 用发送时新报价而非信号价,滑点保护从不检查触发价;carry 与价差通道混仓 | [价格只有一个来源](../defensive-patterns.md#价格只有一个来源信号事件) | analysis/cdex-正式账户复盘-2026-09-25.md(本仓库之外:`analysis/cdex-正式账户复盘-2026-09-25.md`) |
| 持续性 | 多进程各持仓位副本 | 测试/正式双实例 + 多个 Python 进程各持一份仓位,补丁靠 `sync-code.sh` 同步,对账无法收敛 | [状态只有一个写者](../defensive-patterns.md#状态只有一个写者不复制仓位) | [research/00 §2、§6.2](../../design/research/00-existing-codebase-inventory.md) |

## 新增一起事故

1. 在本目录写 `yyyy-mm-dd-<slug>.md`:时间线(UTC)、影响(金额/数据/时长)、根因链(每个促成因素一条)、检测为何滞后、修复、防止复发的规则。
2. 在上表按日期插入一行。
3. 防止复发的用例写进归属组件的 `TESTS.md` 或 [docs/tests/scenarios.md](../tests/scenarios.md)(编号与流程见 [docs/tests](../tests/README.md#7-修改流程设计基线-v1-之后))。
4. 若根因是新的缺陷类别,在 [defensive-patterns.md](../defensive-patterns.md) 增一节(事故一句 + 规则 + 落点 + 验证);若已有类别,在该节的"事故"里补一句。
5. 规则需要常驻时,在根 [AGENTS.md](../../AGENTS.md#常驻规则) 加一行并链接到 defensive-patterns。
