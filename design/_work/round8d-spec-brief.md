# 第八轮第四次修订简报(M0-d;所有修订子代理先读)

依据:`round8-decisions.md` 的 **R8-10**(用户决定:汇总 §4 的 12 项除第 2 项外全部按推荐;累计门槛 ③④ 改为在服务器上运行)与五路评审(`review-2026-10-03-M0c-V1…V5*.md`,含出处行号)及其汇总 `review-2026-10-03-M0c五路汇总.md`。本简报是本次修订的**唯一权威命名与数值来源**;与旧规格、旧简报或评审原文冲突时以本简报为准。修订前副本:`E:\trading\_backup\round8d-pre\`。

## 0. 写作规则

- 沿用 `spec-brief.md`、`round8-spec-brief.md` §1:中文、标识符英文、README 模板、一件事实一个归属地(别处链接)、未核实标"待核实"、大文件用 Write 工具整体重写或用 Edit 精确替换、**只改自己清单里的文件**(可以读任何文件)。
- **写当前状态**:删除你文件里全部历史叙述("本轮新增""第八轮第 N 次修订后""round 6 B-18""改为""以前"等)。"开发备忘"只放待核实项与开放问题,**删除"需要其他组件同步/登记"一类的跨文档同步清单**(例:hft-exec "本轮新增的同步项"、hft-oms :744、hft-ledger :473、hft-sequencer :685);需要别人改的名字在本简报 §8 已给出,不需要再在正文里排队。
- 不得丢章节:改完用 `python E:\trading\architecture-v2\_work\heading_audit.py E:\trading\_backup\round8d-pre` 看自己文件里消失的标题,只允许有意的删除或改名。
- architecture-v2 是设计证据,允许写历史;hft 规格不允许。

## 1. 用户决定的落地(R8-10)

### D1 CI 与仓库(汇总 §4-1;评审 V4 P0-1、P1-9)

1. M1 时 hft 仓库建在一个**免费 GitHub Organization** 下(组织名 M1 时由负责人创建,文中写 `<org>/hft`);负责人 = Admin,老板 = Read 角色。该组织私有仓库有独立的 2,000 min/月、artifact 存储 500 MB、每仓库 cache 10 GB;Trading 仓库仍在个人账户,两边额度互不影响。Free 组织的私有仓库**没有受保护分支**:生产安全以"发布工作流 `preflight` 证实该 SHA 的 `ci.yml` 全绿且 `decoupling-gate` 通过"为准。`ubuntu-24.04-arm` 在个人私有仓库可用已由 Trading 8–9 月 20 次成功运行证实;在组织私有仓库的可用性与倍率 M1 第一周核实。
2. **构建剖面**(唯一归属 `docs/development.md` 构建剖面;hot-path-rules B1 只链接):`release` = `lto = "thin"`、`codegen-units = 16`、`panic = "abort"`、`overflow-checks = false`、`debug = "line-tables-only"`;新增 `release-hot` = 继承 `release` 并设 `lto = "fat"`、`codegen-units = 1`,**只用于正式版的 `hft-core` 与 `kill-switch` 两个二进制**;`ci` = 继承 `release` 并设 `lto = false`、`codegen-units = 16`、`incremental = false`、`debug-assertions = true`、`overflow-checks = true`;`bench` = 与 `release-hot` 相同。开发构建(`-dev.N`)全部用 `release`,只构建 dev2 需要的二进制,跳过 aarch64 一致性集。
3. **触发与作业**:`ci.yml` 只由 `pull_request`(opened/synchronize/reopened)与 `main` 推送触发;新增小工作流 `decoupling.yml`,由 `pull_request_review`、`labeled`、`unlabeled` 触发,只跑 `decoupling-gate`。质量门的 `python` 与 `operations` 合并为一个作业 `ops-py`,按路径决定步骤。`release-aarch64.yml` 增加 `workflow_dispatch` 输入 `sha` 与 `dev_n`:对一个 `ci.yml` 已全绿的 PR 头提交直接出开发构建 `vX.Y.Z-dev.N`(工作流创建该标签),不必先合入 `main`。发布 artifact `retention-days: 1`(Release 资产不占 artifact 配额);CI 不再保存累计账本 artifact。
4. **分钟预算**(唯一归属 development 免费额度):账户额度 2,000 min/月 hft 独享;每夜 x86 ≤ 15 min(≈ 450)+ arm ≤ 5 min(≈ 150);发布构建、开发构建与验证包构建预留 ≥ 500 min/月;其余约 900 min 给 PR 与 `main` 推送;PR 目标 ≤ 12 计费分钟,`timeout-minutes: 20`。用量 > 80% 先缩每夜,再推迟开发构建,预留不动。M1 第一周按实测重算并写进 ops-log。
5. CI 拉 S3 fixtures:只对需要的作业开 GitHub OIDC(`id-token: write`),扮演只读 fixture 桶的角色(`[需授权]` IAM)。

### D2 累计门槛 ③④ 在服务器上运行(R8-10 第 1 条)

1. **服务器仍不编译**。CI 在 `ubuntu-24.04-arm` 上构建**验证包** `hft-verify-<ver>-aarch64.tar.zst`(+ `.sha256`),作为同一标签的 Release 资产:`fuzz/<target>`(固定 nightly + ASan 的 libFuzzer 二进制,每个 core 解码器目标一个)、`bin/hft-dst`(`ci` 剖面的 DST 运行器,来自 `hft-testkit`)、`bin/hft-verify`(驱动)、初始语料 `corpus/`。工作流 `verify-bundle.yml`:正式版标签 `vX.Y.Z` 必跑;`workflow_dispatch(sha)` 可对任一全绿提交出包;开发构建不自动出包。
2. **`hft-verify`**(规格归 `tools/README.md`,cold tier):`hft-verify run --budget-min N`(按轮转表在预算内运行 fuzz 目标与 DST 新种子,写运行记录)、`hft-verify ledger show|rebuild`、`hft-verify crash list`。状态目录 `/var/lib/hft/verify/{ledger,runs,corpus,crashes}`;不联网;S3 备份由 journal-archiver 的新 source(`kind = files`,前缀 `hft/verify/`)上传。崩溃输入最小化后由人带回仓库做回归金样。
3. **部署**(归 deploy):安装到 `/opt/hft/verify/<ver>/`(`releasectl install-verify <ver>`,不影响生产版本激活);单元 `hft-verify.service`(oneshot,`hft-verify run --budget-min`)+ `hft-verify.timer`;用户 `hft-verify`;`Slice=hft-batch.slice`、`Nice=19`、`CPUSchedulingPolicy=idle`、`IOSchedulingClass=idle`、`OOMScoreAdjust=+950`(全部 hft 单元中最先被杀);`MemoryMax` 取 hft-core 内存表新行(dev2 384 MiB,升级主机 2 GiB);`ExecCondition=` 在 `hft-core@prod`(dev2 联调窗口)或 `hft-replay-verify` 正在运行时拒绝启动。dev2 护栏一级动作**最先**停它(先于 `hft-replay-verify`)。
4. **在哪台服务器跑**:dev2 常态运行(护栏内、最低优先级);升级主机只在 M7 期间、门槛 ⑩ 的 7 天测量窗口之外运行;正式实盘(M8)开始后升级主机不再运行,后续版本的累计在 dev2 上继续。
5. **账本规则**(唯一归属 testing §6;阈值仍在 testing §8):只有失败清零;漏跑不清零;运行记录与账本丢失时用 `hft-verify ledger rebuild` 从本机与 S3 的运行记录重建,无记录可证才清零。"≥ 7 次每夜运行"改为"≥ 7 次运行,分布在 ≥ 7 个不同 UTC 日"。**候选版本补跑**:开正式实盘所用的版本 X,其验证包本身必须补跑每个 core fuzz 目标 ≥ 15 min、DST 新种子 ≥ 20,000、零失败(计入累计)。
6. **CI 每夜**只做回归冒烟(DST ≤ 6 min 批量 + fuzz 每目标短跑轮转 ≤ 4 min),不计入 ③④。
7. 估算:dev2 上 batch slice 的 CPU 配额约 0.3 核,fuzz 12 CPU·h + DST 6–39 CPU·h ≈ 3–8 天;升级主机批处理核上约 1 天。
8. 删除所有"dev2 不跑 fuzz/DST""③④ 读 CI 账本 artifact""`cargo xtask ledger rebuild`"的说法。

### D3 延迟证据提前:M1.5 延迟探针(汇总 §4-3;V5 P1-1、§3.1、§3.7;V4 P1-4)

1. **里程碑 M1.5 延迟探针**(在 M1 与 M2 之间;roadmap 归 architecture-v2/11,hft 里由 development、testing、latency-budget 链接):按需按小时租 `c8g.4xlarge`(与 Binance 同 AZ,以 az-probe 为准;`[需授权]` AWS 操作由负责人执行;只放测试网 key),跑纵切片:hft-net + 一个 venue 的 BBO feed(主网公共行情)+ 简化 sequencer + gw(测试网下单)。产出:`kernel_rx`、`ingest`、`decide`(简化)、`egress` 直方图(HW/SW 时间戳分开);核实 c8g 的 HW RX 时间戳与 PTP(假设 A7);核实 ENA 队列数与 IRQ 布局。结论写 Agent Note,据此在 M3 结束前重定 latency-budget §3 的分配并选定主机类(D4)。
2. **参考机从 M1.5 起按需租用**:每个里程碑结束时在同型租用实例上跑 `bench-gate --reference`(相对回归 + 单 crate 上限),不是 CI。删除"参考机要到统一升级后才有""M7 之前没有回归门槛"的说法。
3. **延迟探针策略** `strategies/latency-probe`(`kind_id` 由 registry 分配;scope core;规格归该策略 README):
   - SDK 新增通用描述符位 `StrategyDesc.non_prod_only: bool`:为真时 hft-core 装配拒绝把它绑定到任何 `is_prod = true` 的账户(退出 2,`ProbeOnProdAccount`),sequencer 对试图绑定的命令返回 `ParamInvalid{NonProdOnly}`。latency-probe 设为真。
   - 模式 `quote`:每个 (venue, inst) 在 `Bbo` 触发时至多每 `probe_period_ms`(默认 1 000)挂一张最小名义的 PostOnly 单,价格离对手价 `far_bp`(默认 300 bp),Ack 后立即撤单;产生 `tick_to_trade`、`egress`、`order_rtt` 样本(≈ 8.6 万/天/inst)。
   - 模式 `pair`:每 `pair_period_ms`(默认 60 000)在 venue A 测试网以最小名义 IOC 穿价、经 LegLink 在 venue B 测试网下第二腿,成交后 `pair_flat_ms`(默认 5 000)内两边平掉;产生 `hedge_host`、`leg_gap` 样本(≈ 1,440/天)。
   - 参数 `enabled` 默认 false;只在测试网账户上运行;受风控与测试网限速约束。

### D4 主机档位(汇总 §4-4;V5 P0-3、P1-3、§5-5)

1. 主机类枚举追加 **`c32` = `c8g.8xlarge`(32 vCPU,64 GiB)**,作为 prod + sim 同机的**默认候选**;`c16` 改为"下限候选:首批下单 venue ≤ 4";`c8` 只能承载 prod(无 sim)。最终档位按 M1.5 实测在 M3 结束前选定,宁可上更大档位,不砍首批 venue。`HostClass` 编码只追加 `C32`(值由 hft-rt/hft-refdata 的现有码表顺延)。
2. **升级主机线程计划原则**(唯一归属 hft-core 线程计划;deploy 生成器读取):
   - 核 0:housekeeping + 系统服务(kworker、systemd、sshd、chronyd)+ ENI-A(管理网卡)IRQ。
   - 核 1:**ENI-B(交易网卡)全部队列的 IRQ/软中断独占**,不放任何线程。
   - persist 与段预创建/回收(从 housekeeping 移到 persist 线程)独占一个非隔离核,不在核 0/1,不与 advisor 同核。
   - warm 线程:`ctl-gateway`、`conn-builder`、`snap-fetch`(见 L-P0-3);advisor 另占一核。
   - 隔离热核(`Spin`):`seq`;`feed-bbo-*`(只承载 BBO 路由,可跨 venue 合并);`feed-md-*`(深度/成交/标记价路由);每个下单 venue 一个 `gw-*`。升级主机上 `feed-bbo:*` 任务不得与 `feed-md:*` 同线程(加载校验新增第 13 条,退出 2 `BboSharesWithMd`)。
   - sim:自己的核 + **独立 ENI-C**(IRQ 钉在 sim 的一个核上);批处理与冷进程另占核。
   - `c32` 默认表:0 hk/系统/ENI-A;1 ENI-B IRQ;2 persist(含段预创建);3 warm(`ctl-gateway`、`conn-builder`、`snap-fetch`);4 adv;5 seq(`nohz_full`);6/7/8 `feed-bbo-bn`/`-okx`/`-bg`;9/10/11 `feed-md-bn`/`-okx`/`-bg`;12–17 六个 `gw-*`;18 prod 冷进程;19–21 批处理(含 `hft-verify`、`hft-replay-verify`);22 ENI-C IRQ;23–27 sim(seq、feed、sim-gw、adv、hk/persist/warm 合并);28–31 预留(并入批处理)。隔离集合 = 5–17。
   - `c16` 表(≤ 4 个下单 venue):0 hk/系统/ENI-A;1 ENI-B IRQ;2 persist;3 warm + adv(c16 例外:persist 与 adv 仍不同核);4 seq;5 `feed-bbo`(全部 venue);6/7 `feed-md-a`/`-b`;8–11 四个 `gw-*`;12 prod 冷进程 + 批处理;13 ENI-C IRQ + sim 的 hk/persist/warm/adv;14–15 sim 的 seq/feed/sim-gw。隔离集合 = 4–11。
   - 内存表、slice 的 `AllowedCPUs`、大页按新表重算;`c32` 的 `hft-core.slice`/`hft-sim.slice`/`hft-batch.slice` 由 hft-core 给数。

### D5 正式实盘前的零成交冒烟 + Shadow(汇总 §4-5;V4 P1-1)

launch D 在切 Live 之前加两步(不开仓,不算小额实盘;删除"不设小额实盘后无补偿"的隐含缺口):
- **D-smoke 零成交冒烟**:每个(正式账户, venue)挂一张远离市价的最小名义 PostOnly 单再撤;验证交易所死手刷新与到期、kill-switch 对正式端点撤单、reconciler 连续两轮 `Ignore`、每把 key 的 UID 与 `ACCOUNT_CFG`。
- **D-shadow 正式实例 Shadow 3–7 天**:正式 `prod` 实例全部策略实例 `RunMode = Shadow`(决策照算、订单不发),打通真实保证金、资金费流水、用户流、对账与告警;与同参数的 sim 实例逐决策比对(工具 `replayer compare --a <prod journal> --b <sim journal>`,规格归 replayer),差异需解释。
- 初始 `RiskParams` 上限与"第一次放宽"的条件写成上线签字记录里的**必填数值**(不是门槛)。

### D6 延迟验收指标(汇总 §4-6;V5 §5-6;V4 P2-1;V3 #26)

1. p50、p99 是硬门槛;**p99.9 只记录**(固定节拍突发与周期快照下)。
2. 新分段 **`hedge_host`** = 第二腿 `sent_tsc` − 触发它的第一腿成交帧的 `hw_rx`(计算同 `tick_to_trade`:`kernel_rx + (sent_tsc − rx_tsc)`);样本 = `LegLink` 第二腿的 `LatencySample`。基准形态 p50 ≤ 25 µs、p99 ≤ 80 µs(分配,待 M1.5 实测);门槛 ⑩ 的硬项之一。
3. **同机干扰**统一为:`prod` 的 `tick_to_trade` 与 `hedge_host` p99 劣化 ≤ 5%;测法:7 天内每小时交替开/关 sim 实例,按相邻小时配对比较,取配对比值的中位数。全仓只此一个数(testing ⑩ 为归属,其他链接)。
4. **因收包模型改为中断驱动(L-P0-1),§3 基准形态重定**(分配,待 M1.5 实测;latency-budget 为唯一归属,architecture-v2/00 记录这一调整):`kernel_rx` p50 ≤ 8 µs、p99 ≤ 20 µs;`ingest`(bookTicker)≤ 3 / ≤ 10;`ingest`(depth)≤ 4 / ≤ 12;`decide` ≤ 5 / ≤ 15;`egress` ≤ 5 / ≤ 15;**`tick_to_trade` p50 ≤ 22 µs、p99 ≤ 75 µs**(p99.9 只记录);验收下限仍 ≤ 30 / ≤ 100;`feed_host` p50 ≤ 12 µs;`leg_gap` ≤ 20 µs 不变。85% 规则校核:20 + 10 + 15 + 15 = 60 ≤ 0.85 × 75。
5. 新增 `sw_rx` 常态记录:追加进 `MarketHdr` 与 `LatencySample`(尾部追加字段,布局归 hft-event),诊断分段 `nic_to_kernel`/`kernel_to_user` 进生产直方图(仍不作门槛)。

### D7 门槛 ⑦(模拟 ≥ 14 天)计时规则(汇总 §4-7;V4 P1-7、P2-2;V2 P2-7)

1. 证据绑定 `(发布版本, StrategyParams SHA, RiskParams SHA)`。
2. 停机累计 ≤ 24 h 视为暂停,超过重新计时;策略、执行层、风控代码变化,或参数改动超出"小改白名单"(白名单归该策略 README:只含阈值类字段 ±20% 以内与名单增删),重新计时;压力用例用最终参数重跑。
3. 压力用例:门槛只保留"零强平";另加一个必过的 **L3 路径变体**(按 `max_perp_leverage` 上限杠杆或 1 h +45%,要求依次出现 L1/L2/L3 且无强平);非门槛变体:基差扩大、15 min 内 +30%、上涨中 gw 断开 10 min、盘口量 ×0.2、上涨中 hft-core 重启;交易 inst 的分档 MMR 必须在 refdata 中(缺失 → 用例拒绝运行)。
4. 每日 `--advice recompute` 从当日归档的 Model 检查点起算(见 S-P1-8);当日无检查点 → 跳过当日 recompute 并 P2,不使 ⑦ 重新计时。

### D8 机外看门狗与人工兜底(汇总 §4-8;V4 P1-8)

1. **`hft-offbox-watchdog.service`** 在 dev2 运行(只用 Python 标准库,`system.slice`,独立 Telegram token + 电话类告警通道,通道供应商**待选型**,列入 ⑪):每 15 s `GET http://<升级主机 ENI-A 私有 IP>:<healthz_port>/healthz`;连续 60 s 不可达或报告不健康 → 告警。
2. watchtower 新增 `healthz` 监听(`healthz_bind` = ENI-A 私有 IP,安全组只放行 dev2;与只绑 127.0.0.1 的 `/metrics` 分开):返回 hft-core 心跳年龄与状态、kill-switch 存活、最近一次对账时刻。
3. incident-response 新增一节"**交易主机失联**":授权表(两名人员,**名字待负责人指定**,持有各所网页权限与划转二次验证)、各所网页手工撤单/减仓步骤、从 AMI + EBS/S3 重建主机并以 `--recover-from-snapshot` 恢复的顺序。
4. 正式实盘前置(⑪):看门狗与电话告警演练通过。

### D9 引擎兜底时的风险取舍(汇总 §4-9;V1 §4;V4 P2-9)

| 项 | 规则 | 归属 |
|---|---|---|
| a | L3/flatten 时另一腿不可用 → 先减永续,接受短时现货裸多 + P0(见 M-P0-1) | hft-exec |
| b | RESUME_HOLD 只等对冲腿 venue 的 `IDENTITY` + `ACCOUNT_CFG` + `ORDER_CONNS` 闸门子集且该链接无 `Unknown`;`resume_hold_max_ms`(默认 30 000)超时 → Broken + P0 `Note{OmsResumeTimeout}` | hft-oms、hft-risk |
| c | `ExternalFill{Manual}`(人工在网页操作)**不做引擎默认处理**(不在 5 s 后自动平),只 P1 `Note{ExecManualExternal}`;策略按自己的规则处理(portfolio-carry 见 D10) | hft-exec |
| d | 解除 `HedgeFailure` 要求该账户已无 Broken cell;否则 `CmdResult{Rejected, detail = BrokenRemaining}` | hft-risk、control-plane |
| e | ctl-gateway 不可达 10 s 但 hft-core 心跳仍在(或心跳过期)时,ops_bot 经 kill-switch 本地触发套接字发 `stop_core`:kill-switch 撤单后经 polkit **只允许** `systemctl stop hft-core@prod`(SIGTERM → Drain,不平仓);ops_bot 把这次停止记为"待补 Kill"(`/var/lib/hft/ops_bot/pending_kill.json`),hft-core 恢复、ctl 可达后先自动补发 `Kill`,再允许交易员 `/resume` | kill-switch、ops_bot、deploy(polkit) |
| f | 单个 inst 静默但连接有 pong 仍视为有效;watchtower 新规则 `MarkDivergence`:同一资产跨 venue 的 mark/mid 偏离 > `mark_div_bp` 持续 > 30 s → P1 | watchtower |
| g | Broken 组合内仍"永续先"(R8-8 现规格不变) | hft-exec |
| h | 允许策略在带 LegLink 的腿上挂 GTC/PostOnly:OMS 把 in_flight 拆成 `inflight_immediate`(IOC/FOK/Pending*/Unknown)与 `inflight_resting`(已 Ack 的 GTC/PostOnly);裸量与补腿抑制只用前者(见 M-P1-8) | hft-oms |

### D10 portfolio-carry 规格修正(汇总 §4-10;V2 §4;V5 §5-3)

| 项 | 规则 | 归属 |
|---|---|---|
| a | 平仓方向成本 `close_cost = fee(perp).taker + fee(spot).taker + (spread(perp) + spread(spot)) / 2`(只算平仓两腿);倍数条件 `cost_multiplier` 只用于开仓;`gain_daily` 一律用 `FundingSummary.realized_daily`(不把窗口合计当每日) | pc |
| a' | **默认生效的负资金费退出**:`neg_funding_exit_periods`(u16,默认 3;0 = 关闭):连续 N 次结算的实收资金费率 < 0 → 该 group 目标 0/0、`reason = Exit`(不经成本门),`reentry_block_ms` 照常;状态 `neg_streak`;N 的取值在模拟阶段研究 | pc |
| b | CloseOnly 下**持有**(不开新仓,不强制退出);新增实例级 StrategyCmd `exit_all`(`reduce_only = true`)= 对全部 group 执行 `exit`;超配部分(`actual > cap_i × (1 + overweight_tol)`,`overweight_tol` 默认 0.10)按减仓处理,只受 `close_cost` 回本门约束、不受倍数条件约束 | pc |
| b' | `External{Manual}` → 该 group 冻结(不设新目标,阶段 `Fix`),`Note{ManualExternal}` P1,直到 `clear_block` 命令或两腿重新对齐;`External{Unknown \| Delivery}` 仍对齐到较小一腿 | pc |
| c | 与杠杆无关的硬上限:每个永续 venue `Σ|永续名义| + 新增 ≤ margin_balance / perp_min_equity_ratio`(`perp_min_equity_ratio` 默认 0.40,待标定);重写"`perp_margin_frac` 与交易所杠杆必须一致"一段的论证(开仓按 avail_margin 增量判断时 `perp_margin_frac` 不决定安全) | pc |
| c' | SDK 暴露实际杠杆 `Views::leverage(a, i) -> Option<u32>`(百分之一倍,来自 `AccountConfigReport`);hft-risk `RiskParams.max_perp_leverage_x100`(默认 300):任一交易中的永续 inst 实际杠杆超过 → `ACCOUNT_CFG` 不符(禁开)+ P1 | SDK、hft-risk |
| d | 检查点只存原始样本(两侧基差样本环、资金费环、`kline_until_ns`),派生状态恢复后重算 | pc、hft-strat-lib |
| e | 结构参数允许热更:SDK `FieldDesc.structural: bool`;`Model::on_params` 返回 `ModelParamEffect { None, ResetGroups(GroupMask), ResetAll }`;被重置的 group 写 `Note{ModelGroupReset}`,pipeline 对它们自动预热;hft-strat-lib 的分位快照写入 `PctConfig`,不一致按 group 重置 | SDK、strat-lib、pc、pipeline |
| f | SDK `Model::mem_bytes(p, n_groups) -> u64` 与 `Model::snapshot_bytes(p, n_groups) -> u64`(上界);hft-core 配置 `advisor.mem_budget_mib`(按主机类与实例,在内存表)取代"advisor 一行按首个策略定";装配时求和超预算 → 退出 2;ctl-gateway 在 SetParams/绑定校验时超预算 → `ParamInvalid{ModelMemBudget}`;**新增策略零改动包括 hft-core 规格**(只可能需要运维调大 `advisor.mem_budget_mib`) | SDK、hft-core、hft-ctl、hft-sequencer |
| f' | Model 快照分块:SDK `Model::snapshot_chunk(&self, from_gix: u16, w: &mut SnapshotWriter, max_bytes: u32) -> SnapshotProgress { More(next_gix), Done }`;advisor 每次 `poll_once` 只写一块(≤ 1 ms 分片) | SDK、hft-sequencer advisor、hft-core 检查点表 |
| g | journal-archiver 每天把每个策略实例在 UTC 00:00 之后写出的第一份检查点归档到 S3 `…/advisor/<instance>/<date>/`;`replayer verify --model-checkpoint <path>` | journal-archiver、replayer、testing |
| h | 深度不在 BBO 热路径:深度只经 `feed-md` 线程与可被抢占的 md 环进入(L-P0-2/3);pc 快速部分不订阅 L2 事件(切片的可见深度由执行层读簿),冲击估算在 Model(`Part::Model` 订阅)。pc 的 `depth_levels` 范围改为 0–10 | pc、hft-exec |

### D11 dev2 退路(汇总 §4-11)

先按 O-P1-2 收紧护栏;若护栏一周触发 ≥ 2 次,或旧系统的延迟/成交可测劣化 → 另开一台 `c7g.large`(约 $66/月,`[需授权]`)专给 hft,主机类仍按 `dev2` 配置,护栏阈值放宽(那台主机上没有旧系统)。写进 hft-core dev2 护栏与 architecture-v2/11。

### D12 范围与研究手段(汇总 §4-12;V5 §3.2)

1. **冻结可选组件**:`venue-gate`、`venue-bybit`、`venue-kucoin`、`venue-hyperliquid`、`hft-chain-evm`、`hft-ext-proto`、`hft-amm`、`hft-chain-solana`、`apps/chain-core`、`apps/chain-grpc-relay`、`apps/mock-exchange`、`contracts/`。frontmatter `status: frozen`(README 模板新增该取值);概要第一行写"冻结:不随 core 规格同步;解冻(触发条件同原文)时先按 core 规格复核"。评审中只涉及冻结组件的不一致不再修正文,在冻结说明里列出编号(如"已知待复核:V3 #59、#60、#62、#63、#64")。
2. 快照节字节布局、行版本号、其余 `#[repr(C)]` 布局在章节标题旁注"草案,实现时以代码与编译期断言为准";码值与字段名仍须全仓一致。
3. `replayer sim` 增加 `--ab <paramsA> <paramsB>`:同一段已录 journal、同一模拟撮合器,只输出两组参数的**差值**;默认关闭(需显式参数),报告首行强制标注"相对比较,不是收益估计,不得用于定仓"。

## 2. P0 规范解法

### L-P0-1 收包模型唯一化(中断驱动)

- deploy 运行期调优:`net.core.busy_poll = 0`、`net.core.busy_read = 0`;ENI-B 的 `napi_defer_hard_irqs = 0`、`gro_flush_timeout = 0`;保留 `adaptive-rx off rx-usecs 0`。ENI-B 全部队列 IRQ → **核 1(独占)**,ENI-A → 核 0;`irqaffinity` = 核 0 与核 1。删除 GRUB 段的 "`busy_poll=25`" 说明。
- hft-net:conn-builder 建连时对每个 socket **显式** `setsockopt(SO_BUSY_POLL, 0)`(不再"默认不设");`busy_poll_us` 配置项删除。hostprobe 把四项 sysctl/网卡参数与 IRQ 落点列为 hard 校验(不符 → hft-core 退出 3)。
- T12 改写:IRQ 在专用 IRQ 核(升级主机类核 1),热线程只做非阻塞 `recvmsg`,不做 NAPI 轮询;`IRQ_ON_HOT` 告警仍为真。
- sim 用独立 **ENI-C**(主机清单 `[eni_c]`,其队列 IRQ 钉在 sim 的一个核上;sim 出口 IP = ENI-C 的私有 IP + EIP);hostprobe 核对 sim 连接的 `SO_INCOMING_NAPI_ID` 不属于 ENI-B。dev2 只有一块网卡,不适用(dev2 无延迟验收)。
- `kernel_rx` 预算按 D6 第 4 条重定;M7 用 sim 开/关对比 softnet 计数实证。

### L-P0-2 sequencer 主循环可抢占

- `step()`:驱动生成的 Input、ctl/hk、实盘 rep、bbo、advice 按原优先级取并处理;**md 改为逐条处理**,每处理完一条 md 消息(`L2Batch`、`Trade` 等)即查看 ctl、实盘 rep、bbo 环头(每个一次 Acquire 读),非空则先取并处理它们(本轮 ctl ≤ 4、rep ≤ 64、bbo ≤ 128 的上限照旧计),再继续 md。取出顺序照常写进 journal,确定性不变。
- md 每轮上限改为 `md_levels_per_round`(默认 4,096 档)与 `md_round_ns`(默认 50 000 ns)先到者;`rep-sim` 仍在最后、`sim_max_per_round`。两个新参数在 ECF1 中 sequencer 的段(与 `sim_max_per_round` 同段)。
- latency-budget §2.3:BBO 在 sequencer 侧最多等**正在应用的那一条** md 消息(≤ `snapshot_part_levels` × 簿更新上限)+ 一次环头检查。
- 基准 `seq/l2burst` 改为不依赖 mock 的单进程基准(生产者线程注入深度突发与 BBO),放进 M2;断言"BBO 等待 ≤ 一条 md 消息的应用时间 + 检查开销"。

### L-P0-3 feed 线程拆分与快照移出热线程

- hft-venue:同一 venue 拆成两个任务 `feed-bbo:<venue>`(只含 `RouteKind::Bbo` 路由)与 `feed-md:<venue>`(深度/成交/标记价路由),都是 `FeedRuntime<P>`,配置 `routes = Bbo | Md | All`;dev2/c4 可配 `All`(同一任务)。`routes = All` 时 `FeedRuntime` 每处理完一条非 Bbo 帧即 poll 一次全部 BBO 连接(T13 补充)。
- **REST 快照移到 warm 任务 `snap-fetch`**(hft-venue `SnapFetchTask<P>`;每实例一个,在 warm 线程):拥有快照用的 REST 连接,负责请求、读取、解密、解析为 `L2SnapshotPart`;环 `snapreq-<venue>`(feed-md → snap-fetch,`SnapReq`)与 `snap-<venue>`(snap-fetch → feed-md,`ByteSpsc`,携带原始响应体与解析后的分片);feed-md 收到后写 `RawRx`(标 REST)并驱动重同步状态机。`feed.resync_parallel` 的"同时在途快照"上限移到 snap-fetch。T7 中 feed 线程的 REST 快照系统调用与"单次解析 ≤ 100 µs"条款删除,改为 snap-fetch 的 warm 预算。
- 升级主机线程计划按 D4。

### M-P0-1 L3/flatten 不依赖另一腿报价

hft-exec:`reason ∈ {MarginStress, Hard}`(含 `Kill{flatten}`)的非 Broken cell 预检只要求永续腿(第一腿)`quote_live`;第二腿报价不健康或被门控时退回 Broken 同款的串行路径(永续 reduce-only 先减,现货腿在其报价恢复后再减),称 **`SerialFallback`**:写 `ExecDecision{ForceStep, detail = SerialFallback}` 与 `Note{ExecSerialFallback}`(P0);进入后直到该 cell 到达引擎目标不切回 PairOnFill。L3 与 flatten 同样作用于该账户 `GROUP_NONE` 的永续持仓(单腿 TakerSlice reduce-only,P0 Note)。DST 必过用例 **MS-3**:"L3 + 另一所 BBO 断 30 s → 永续仍按步减"(testing §4.4)。

### M-P0-2 引擎单的限价与名义对齐风控

- hft-types(与 `HedgePxLimits` 同处)新增纯函数 `cap_cross_px(side: Side, touch: Px, mid: Px, max_cross: Rate, lim: &HedgePxLimits) -> Px` = 买:min(touch × (1 + max_cross), mid × (1 + lim.rr_max_cross)),卖对称,按 side 取整到 tick。OMS `price_hedge` 与 hft-exec 的全部 Engine 订单(HedgeFix、AutoFix、ForceStep、Flatten、补腿)共用它。
- SDK `RiskStateView` 增加 `rr_max_cross`;hft-risk 检查链 #6 定义 `IntentView.notional = qty × 限价`;切片数量按限价折算;Open 切片也按 `max_order_notional` 截量。
- 校验:`ExecConfig.force_slice.max_cross ≤ RiskParams.rr_max_cross` 且 `force_slice.max_chunk_notional ≤ RiskParams.max_order_notional`,hft-core 装配不符退出 2(`ExecRiskMismatch`),`SetRisk`/引擎配置变更不符 `InvalidArgs{ExecRiskMismatch}`;策略 `PairParams.slice` 的 `max_cross`/`max_chunk_notional` 在执行时按上述两项**截取**(不拒绝)。
- Flatten/ForceStep 被 #5/#6 拒 → 按执行失败处理:`ExecDecision{ForceStep, detail = RiskReject}` + `Note{ExecEngineOrderRejected}` P0。

### D-P0-1 CI

见 D1、D2。

### C-P0-1…11(V3 #1–#11)

| # | 规范 | 归属(改的一方) |
|---|---|---|
| 1 | SPB1 body 的 `#[repr(C)]` 布局(`ParamsBodyHdr`、`ParamOverride`、`body_view`)写在 SDK `params`;hft-event 只说"body 不透明,编码归 SDK";hft-ctl `parse_spb1` 链接 SDK | J2 写;J6 改 hft-event/hft-ctl |
| 2 | `SnapshotHdr.purpose`:`Boot=1 Keyframe=2 OnDemand=3 Recovery=4 Drain=5` | J6(journal-format) |
| 3 | `StrategyRowHdr` 增加 `desc_version: u16`(布局与断言重算) | J5;J6 链接 |
| 4 | ReconcileReport Summary 条目追加 `sflags: u16`(`USERSTREAM_DOWN=1`、`TRUNCATED=2`、`HISTORY_UNCHECKED=4`)与 `skipped_inflight: u16`,条目版本 v3 | J6 写布局;J9 reconciler 写语义 |
| 5 | `NoteLevel`:`P0=1 P1=2 P2=3 Info=4`(hft-event);SDK、ops_bot、hft-telemetry 用同名 | J6;J2、J9、J10 对齐 |
| 6 | `ExecDecision.detail` 的全部子码数值与 u32 打包规则由 hft-exec 登记(TargetReject、Stuck、Broken、AutoFix、ForceStep 各子码,含新增 `SerialFallback`、`RiskReject`) | J4;J2 SDK 链接 |
| 7 | CID bit 15 恒为 0(以 hft-types 为准);删除 hft-oms 的 `instance << 15` 与 hft-venue 的"实例位" | J4、J7 |
| 8 | ConnRole:`Md=1 Order=2 UserStream=3 Rest=4`(event-model) | J7 改 hft-net |
| 9 | `MarginMode` 追加 `Mixed=4`(hft-refdata 与 hft-event) | J6;J7 venue 文档引用 |
| 10 | 预热:pipeline 插件产出块列表,每块 ≤ 4096 B 且自身完整;hft-ctl/hft-ctl-proto 客户端逐块发送,不切字节 | J6;J10 pipeline |
| 11 | `index.json` 为数组(以 registry 为准);tools 与 hft-py 对齐 | J6、J11 |

## 3. P1 规范解法

| 编号 | 解法 | 归属 |
|---|---|---|
| L-P1-2 | `BboView` 健康按成员聚合:每 venue 的 BBO 连接槽 `MAX_BBO_CONN_SLOTS = 8`(连接 id 即槽号,退役后复用);sequencer 每 (venue, slot) 维护 `ConnLive{up, resync, live_tsc}`(快照节 26),每 inst 维护 `bbo_member_mask: u8`(节 10;由 `ConnState{Up/Down, conn, inst}` 维护);SDK `Views::bbo(i) -> BboView` **按值返回**,`conn_up` = 任一成员 up,`conn_live_tsc` = up 成员的最大值,`resync` = 成员 resync 的或 ∨ inst 的 bbo_resync ∨ 簿 Resyncing;`BboView` 另带最近一条 Bbo 的 `rx_tsc` 与 `hw_rx_ns`(供 LeadLag 类策略);预算 `seq/conn_alive` 不变 | J2、J5、J7 |
| L-P1-3 | 见 D4 线程原则(IRQ 独占核、persist/段预创建移出核 0) | J8、J11(hft-journal 段预创建归属改 persist) |
| L-P1-4 | gw:UserStream 连接**每圈** poll(`gw.aux_every` 只管 standby 与 REST);每解一帧回报 `drain_out()` 一次;`hedge_host` 分段见 D6 | J7 |
| L-P1-5 | M3 之前必须核实:Bitget UTA V3 WS 下单频道、Binance USDⓈ-M 测试网 WS API、私有频道能否同时双订阅;HW RX 时间戳在 M1.5 核实 | J7、J1 |
| M-P1-1 | Broken 自动解除 = **已成交**裸量 × mark ≤ `dust_notional` 且无 Pending*/Unknown/在途;自动平仓成功判定同口径、每片终态后判断 | J4、J5 |
| M-P1-2 | `on_external` 只暂停/清除 `origin = Strategy` 的目标;Engine 来源、`force_active`、`flatten_lock` 账户不暂停不清除;Broken cell 保持 Broken | J4 |
| M-P1-3 | `recovery_end` 的 `now` 取本次运行 TscClock,写入 Recovery 快照头 | J5、J4 |
| M-P1-5 | `naked_groups` 按 `dust_notional` 过滤;设 Broken 后同一 Input 内调一次 `on_legs` | J5 |
| M-P1-6 | BalanceOnlyPush:按 (account, venue, settle_ts) 记已入钱包额;reconciler 对这类 venue 的 `engine_amount` 取已入钱包额,差额用 `AdjustDelta::Funding` 动钱包;FundingSettle 推送纳入去重 | J5、J9、J7 |
| M-P1-7 | Binance:窗口内存在不属于已知槽/外来单的 orderId 成交 → `history_checked = 0` + P0;grace 后按 client id 复查两次仍 -2013 且无未归属成交才给 1 | J9(reconciler)、J7(venue-binance)、J4 |
| M-P1-8 | 见 D9-h | J4 |
| S-P1-1 | 见 D10-f | J2、J8 |
| S-P1-3 | `Exec{Stuck}` 只对 `reason = Rebalance` 的目标 `clear_target`;Exit/Hard/MarginStress 保留,`fail_cooldown_ms` 后重发并写 P1 Note;`Exit{Cmd}` 保存命令字段以便重发 | J3 |
| S-P1-4 | hft-exec 对减仓方向、`Exit`/`Hard`/`MarginStress` 不做 `LotMismatch` 拒绝(按较粗 lot 取整,残余交"残余与尘埃");`SetParams` 与 `set_target` 的阈值统一为 `slice.max_chunk_notional` | J4、J5 |
| S-P1-5/6 | 见 D10-d、D10-e;检查点金样改为比对布局描述 `ModelLayoutDesc`(节标签、元素类型、长度规则;文件 `fixtures/strategies/<name>/ckpt/layout.json`),删除字节金样;布局描述变化而 `LAYOUT_VERSION` 未升 → CI 失败 | J2、J3 |
| S-P1-7 | `GroupId` 单调分配永不复用(pipeline 分配器,hft-types 写语义);SDK 规定 `on_params` 比对 `GroupSpec::fingerprint()`(asset + legs),不一致即重置该 group;sequencer 记每 group 的 `spec_seq`,丢弃 `basis_seq < spec_seq` 的 Advice | J6、J2、J5、J10 |
| S-P1-8 | 见 D10-g、D7-4 | J9、J8 |
| S-P1-9 | 见 D10-c、c' | J3、J2、J5 |
| O-P1-2 | dev2 护栏:干净 Drain(`HeartbeatBlock.state = Stopped` 且 persist 已刷完)后由 `ExecStopPost=+…/hft-shm-release %i` 删除该实例 tmpfs 中的热段文件(崩溃路径保留);停机信号增加 swap-in 速率(`/proc/vmstat` `pswpin` 增量 > 1,000 页/min 持续 2 min)与 `system.slice` 的 `memory.pressure some avg10 > 5%`(swap 已用量增长仍只告警);watchtower `HostPressure` 阈值与护栏一致(`MemAvailable < 600 MiB`);dev2 内存按"满载时 `MemAvailable` ≥ 1 GiB"重算(减 sim 订阅等),并为 `hft-verify` 留出 batch 内的份额 | J8、J10、J9 |
| O-P1-3 | 删除 dev2 的 `Conflicts=`;联调窗口脚本 `deploy/dev2/hft-window start|stop`:start = `systemctl mask --runtime` 批处理单元与定时器(`hft-replay-verify`、`hft-verify`、`hft-journal-archiver`、`hft-pipeline`、`hft-treasury`)→ 停止它们 → 启动 `hft-core@prod`;stop = 反向;批处理单元另带 `ExecCondition=` 检查 `hft-core@prod` 未运行(双保险) | J10、J8 |
| O-P1-4 | 门槛 ⑥ 用 `feed_env = "testnet"` 与测试网 refdata(测试网符号映射待核实,A21) | J9、J8、J10 |
| O-P1-5 | **IMDS 锁定**(只在 `formal_live = true` 的主机;dev2 不做,避免影响旧系统):nftables 只放行 uid 0 访问 169.254.169.254;root 的 `hft-cred-broker.service` 以实例角色 AssumeRole 出带 session policy 的短期凭证,按消费者写 `/run/hft/creds/<user>/credentials`(0400,45 min 刷新):archiver(S3 写 `hft/` 前缀)、ops_bot/pipeline/treasury(S3 只读各自前缀);preflight 检查"非 root uid 访问 IMDS 被拒";可 AssumeRole 的角色是 `[需授权]` IAM 变更 | J10 |
| O-P1-6 | 见 D7 | J9 |
| O-P1-7 | 见 D8 | J9、J10 |

**V4 P2 的规范**(其余 P2 由各归属者按评审原文处理):M7→M8 切换补三步(清 hugetlbfs 上的测试网热段;archiver source 前缀 M7 = `hft-testnet`;搬目录前 `journal-archiver check` 确认测试网小时全部提交);正式账户 kill 演练以 incident-response 为准(正式账户只在维护窗口、CloseOnly 下做零成交撤单冒烟,不 SIGSTOP);保留 SSM agent 作破窗通道(限非隔离核);启动时校验两实例账户名不相交;改动 `venue-*`/`hft-venue`/`hft-net`/`hft-oms` 的版本先在 dev2 测试网窗口冒烟;冷盘容量按"seq 7 天 + raw 3 天、raw ≈ 160 GB/天"重算;hugetlb 不计入 `memory.max` 写进 hft-core 内存表注释;hft-sim 概要写明适用范围(10–20 ms 窗口的跨所吃单类策略评估不了)。

## 4. V3 一致性表的处理

1. 每个代理逐行读 `review-2026-10-03-M0c-V3一致性.md` §2,找出点名到自己清单内文件的行,按"修法"列改**输的一方**;赢的一方需要补码表或取值的,由其归属者补。只涉及冻结组件(D12)的行不改正文,由 J12 写进冻结说明。
2. **下列行的修法被本简报覆盖**:#13/#13a(进快照的 TSC 只允许 OMS/exec/定时器堆的已登记行,恢复时按 `recovery_end` 平移,停机不计;删除"按 boot_id 重建"的全部说法;根 AGENTS 与 crates/AGENTS 补这条例外);#14(`SnapshotHdr` 的 `snap_tsc`、`boot_id`、`snap_utc_ns` 加入 `MEASURE_MASK`);#26(→ D6-3);#29(→ incident-response 为准);#32/#80/#85(→ L-P0-1:ENI-B IRQ 在**专用 IRQ 核**,不是 hk 核);#43(→ D4:`c32` 默认候选、`c16` 下限候选、`c8` 只承载 prod);#46(→ O-P1-2);#66(放行前 UID 不符:hft-core 启动检查退出 2;hft-risk 补"放行前"分支的说明链接 hft-core);#67(新 refdata 复核失败 → `CmdResult{Rejected}`,继续用旧 refdata,不退出;sequencer 补被拒分支);#70(按 O-P1-2 重算);#72(`FundingCorrected` 全仓 P1,watchtower 改为 P1);#73(缩容只经 Drain + 新 EngineConfig 重启,启动时当前占用 > 新容量 → 退出 2 `CapacityShrink`)。
3. 文档体系(V3 §3):每个代理清理自己文件的历史叙述与同步队列;J11 压缩篇幅;J12 加机检。

## 5. 文档篇幅与机检

- 篇幅计量(docs/AGENTS.md 写明;J12 的检查实现同一口径):去掉代码块与链接 URL 后,**CJK 字符数 + 英文/数字词数**(连续 `[A-Za-z0-9_.-]+` 计 1)。上限不变:根 AGENTS.md ≤ 1,800;architecture.md ≤ 3,000;子树 AGENTS.md ≤ 900。超出时先把不属于这里的内容搬到归属地,再压缩。
- `check_hft.py` 新增检查(J12):① 历史叙述关键词(`本轮`、`修订后`、`第[一二三四五六七八九十]+轮`、`round \d+`、`R\d-\d+` 出现在 hft/ 正文,`.agents/notes/` 与 `docs/postmortem/` 除外,"延伸阅读"节除外);② 同步队列标题或条目(`同步项`、`需要其他组件`、`需要.*登记`);③ 篇幅上限;④ `#[repr(C)]` 结构中名为 `*kind`/`*level`/`*purpose`/`*role`/`*mode`/`*status`/`*reason`/`*code` 的整型字段,其注释须含码值(`=`)或码表链接;⑤ 结构字段字节和与声明字节数一致;⑥ 正文出现的 `Note{X}` 与 `Timer{X}` 名须在 hft-event 登记表中;⑦ 冻结组件跳过跨文档一致性检查;⑧ 残留:`busy_poll=25`、`busy_read=25`、`napi_defer_hard_irqs=2`、`Conflicts=hft-replay-verify`、"c8 或更大"/"c8 或更高"、`kernel_rx` 的"≤ 4 µs"基准、`ledger rebuild`(xtask)、"dev2 不跑 fuzz"、`pull_request_review` 触发整套质量门。

## 6. 工作分配(只改自己清单里的文件)

| 代号 | 文件 | 重点 |
|---|---|---|
| J1 | `architecture-v2/README.md`、`00`–`13`、`现状测量…`、`待测量…`(不含 `research/`、`_work/`) | 01–05、08、09 开头加"实现以 hft 规格为准";改写 V3 #78–#85、#128、#129 与 V5 §3.3 的冲突;00 记录 D6-4 的预算调整;11 路线图加 M1.5、D2、D4、D5、D11;12 风险更新(A7 在 M1.5 核实、A21);13 末尾追加 §17"第八轮第四次修订"摘要;待测量 H2 销账(arm runner 在个人私有仓库已证实,组织仓库 M1 核实) |
| J2 | `crates/hft-strategy-sdk`、`crates/hft-strat-lib`、`strategies/AGENTS.md`、`strategies/registry/**`、`strategies/{cdex,cross-taker,maker-taker}/**`、新建 `strategies/latency-probe/README.md`、`docs/cookbook/adding-a-strategy.md` | C-P0-1;D3-3;D10-c'、e、f、f';L-P1-2(SDK 侧);S-P1-5/6/7;`NoteLevel` 对齐;`note()` 加 group(V3 #36);V2 P2-12、P2-15;registry 登记 latency-probe |
| J3 | `strategies/portfolio-carry/**`(README 与 `pipeline/`) | D10 全部;S-P1-3、S-P1-4(策略侧);V2 P2 全部;小改白名单(D7-2);V3 #37 |
| J4 | `crates/hft-exec`、`crates/hft-oms` | M-P0-1、M-P0-2;D9-a、b(OMS)、c、g、h;M-P1-1/2/3/7/8;S-P1-4;C-P0-6、7;V1 P2 全部;删除同步清单 |
| J5 | `crates/hft-risk`、`crates/hft-ledger`、`crates/hft-sequencer` | L-P0-2;L-P1-2(节 10/26);M-P0-2(风控侧);M-P1-1/3/5/6;D9-b、d;D10-c'、f(校验);C-P0-3;S-P1-7(`spec_seq`);V3 #13–#18、#48、#49、#50、#66、#67、#69 等 |
| J6 | `crates/{hft-types,hft-event,hft-refdata,hft-ctl-proto,hft-ctl}`、`docs/subsystems/*`、`docs/cookbook/{adding-an-event-kind,adding-a-control-command}.md`、`tools/README.md` | C-P0-1(链接)、2、4、5、9、10、11;`cap_cross_px`;`HostClass::C32`;新 Note/Timer/码登记(§8);`sw_rx` 追加;`hedge_host` 样本字段;`hft-verify` 规格;xtask 名字(V3 #42)与 tools 示例;GroupId 语义;`ParamInvalid{ModelMemBudget, NonProdOnly}`、`InvalidArgs{ExecRiskMismatch}`、`CmdResult` detail `BrokenRemaining` |
| J7 | `crates/{hft-venue-proto,hft-venue,hft-net,venue-binance,venue-okx,venue-bitget}`、`docs/cookbook/adding-a-venue.md`、`fixtures/README.md` | L-P0-1(hft-net)、L-P0-3、L-P1-2(成员槽)、L-P1-4、L-P1-5;C-P0-7、8;M-P1-6/7(venue 侧);V3 venue 行 |
| J8 | `apps/hft-core`、`apps/replayer`、`crates/hft-sim` | D4 线程计划与内存表(c32/c16/dev2 重算,`advisor.mem_budget_mib`、`hft-verify` 行、hugetlb 注释);加载校验 13、`ProbeOnProdAccount`、`ExecRiskMismatch`、`CapacityShrink`;D10-f、f';O-P1-2/3(hft-core 侧);D11;replayer `compare`、`--ab`、`--model-checkpoint`;hft-sim 压力用例变体与适用范围 |
| J9 | `apps/{reconciler,watchtower,kill-switch,journal-archiver,mock-exchange}`、`crates/{hft-testkit,hft-telemetry}`、`docs/testing.md`、`benchmarks/*` | D2(testing §6/§8 账本规则);D6;D7;D8(watchtower healthz);D9-e、f;O-P1-4、6;门槛 ⑥⑦⑩⑪ 改写;`seq/l2burst` 单进程;参考机 M1.5 起;journal-archiver 的 `files` source 与每日检查点归档;mock-exchange 冻结 |
| J10 | `python/**`、`deploy/*`、`docs/development.md`、`docs/runbooks/*`、`docs/postmortem/README.md`、`.agents/notes/**` | D1、D2(部署与工作流)、D5(launch)、D8(看门狗单元、IR 新节)、D9-e(polkit、ops_bot 待补 Kill)、D11、L-P0-1(调优与 ENI-C)、O-P1-2/3/5;M7→M8 步骤;ops_bot `NoteLevel`;新 ADR(§7) |
| J11 | `hft/{AGENTS.md,CLAUDE.md,README.md}`、`docs/{AGENTS.md,architecture.md,glossary.md,defensive-patterns.md,hot-path-rules.md,latency-budget.md}`、`crates/AGENTS.md`、`apps/AGENTS.md`、`docs/cookbook/adding-a-crate.md`、`crates/{hft-journal,hft-rt,hft-book,hft-py}` | 篇幅压缩;T7、T8、T12、T13、D6、B1 改写;latency-budget 按 D6 与 L-P0-2/3 重写 §2.3、§3、§6、§7;README 模板加 `status: frozen`;篇幅计量写进 docs/AGENTS;hft-journal 段预创建移到 persist;hft-rt `HostClass::C32`;glossary #28、#112 |
| J12 | `crates/{venue-gate,venue-bybit,venue-kucoin,venue-hyperliquid,hft-chain-evm,hft-ext-proto,hft-amm,hft-chain-solana}`、`apps/{chain-core,chain-grpc-relay}`、`contracts/README.md`、`architecture-v2/_work/check_hft.py` | D12-1 冻结说明(列出涉及它们的 V3 编号);§5 机检;最后对全仓跑一遍检查,把结果写进报告(不修改别人的文件) |

## 7. 新 ADR(J10 写,`hft/.agents/notes/proposed/`)

`architecture/2026-10-03-ci-org-and-profiles.md`、`process/2026-10-03-verify-on-server.md`、`architecture/2026-10-03-latency-probe-milestone.md`、`architecture/2026-10-03-rx-irq-driven.md`、`architecture/2026-10-03-host-class-c32.md`、`architecture/2026-10-03-offbox-watchdog.md`、`architecture/2026-10-03-imds-lockdown.md`、`process/2026-10-03-shadow-before-live.md`、`architecture/2026-10-03-feed-split-and-snap-fetch.md`、`architecture/2026-10-03-sequencer-md-preempt.md`;更新索引与受影响旧 ADR 的状态行(`ci-free-hosted`、`no-small-live`、`sim-colocated-with-prod`、`quote-freshness-conn-health`、`build-target-aarch64`)。

## 8. 命名总表(跨文件引用一律用这些名字)

| 名字 | 种类 | 定义归属 |
|---|---|---|
| `release-hot`、`ci`(新设置) | Cargo 剖面 | development |
| `decoupling.yml`、`verify-bundle.yml`、`ops-py` | 工作流/作业 | development |
| `hft-verify-<ver>-aarch64.tar.zst`、`hft-verify`、`hft-dst` | 验证包与二进制 | tools(hft-verify)、deploy(包与安装)、hft-testkit(hft-dst) |
| `hft-verify.service`/`.timer`、用户 `hft-verify`、`/var/lib/hft/verify/` | 单元 | deploy;内存数在 hft-core |
| `releasectl install-verify` | 命令 | deploy |
| `files` source、前缀 `hft/verify/` | archiver 配置 | journal-archiver |
| M1.5 | 里程碑 | architecture-v2/11;hft 链接 |
| `latency-probe`、`StrategyDesc.non_prod_only`、`ProbeOnProdAccount`、`ParamInvalid{NonProdOnly}`、参数 `probe_period_ms` 1000、`far_bp` 300、`pair_period_ms` 60000、`pair_flat_ms` 5000、`enabled` false | 策略与描述符 | 策略 README;SDK;hft-core;control-plane |
| `c32`、`HostClass::C32`、加载校验第 13 条 `BboSharesWithMd` | 主机类 | hft-core;hft-rt/hft-refdata 码表 |
| `feed-bbo:<venue>`、`feed-md:<venue>`、`routes = Bbo\|Md\|All`、`snap-fetch`、`SnapFetchTask<P>`、`snapreq-<venue>`、`snap-<venue>`、`SnapReq` | 任务与环 | hft-venue;线程计划在 hft-core |
| ENI-C、清单 `[eni_c]` | 网卡 | deploy;hft-core 资源隔离 |
| `md_levels_per_round` 4096、`md_round_ns` 50000 | sequencer 参数 | hft-sequencer |
| `MAX_BBO_CONN_SLOTS = 8`、`bbo_member_mask`、`Views::bbo(i) -> BboView`(按值)、`BboView.rx_tsc`/`hw_rx_ns` | 视图 | SDK;sequencer(节 10/26);hft-venue(槽号) |
| `hedge_host`、`sw_rx`(常态) | 分段/时间点 | latency-budget;布局 hft-event |
| `SerialFallback`、`RiskReject`(ExecDecision 子码)、`Note{ExecSerialFallback}`、`Note{ExecEngineOrderRejected}`、`Note{ExecManualExternal}`、MS-3 | 执行层 | hft-exec;Note 登记 hft-event;用例 testing |
| `cap_cross_px` | 纯函数 | hft-types |
| `RiskStateView.rr_max_cross`、`IntentView.notional = qty × 限价` | 视图/定义 | SDK;hft-risk |
| `ExecRiskMismatch`(退出 2 与 `InvalidArgs` detail) | 校验 | hft-core;control-plane |
| `resume_hold_max_ms` 30000、`Note{OmsResumeTimeout}` | OMS | hft-oms;Note 登记 hft-event |
| `BrokenRemaining` | `CmdResult` detail | control-plane;hft-risk |
| `inflight_immediate`、`inflight_resting` | OMS | hft-oms |
| `stop_core`(kill-switch 触发动作)、`pending_kill.json` | 兜底 | kill-switch;ops_bot;polkit 在 deploy |
| `MarkDivergence`、`mark_div_bp` | watchtower 规则 | watchtower |
| `healthz_bind`、`/healthz`、`hft-offbox-watchdog.service` | 机外监控 | watchtower;deploy |
| `hft-cred-broker.service`、`/run/hft/creds/<user>/credentials` | IMDS 锁定 | deploy |
| `hft-window start\|stop`、`hft-shm-release` | dev2 脚本 | deploy;规则在 hft-core |
| `neg_funding_exit_periods` 3、`neg_streak`、`exit_all`、`overweight_tol` 0.10、`perp_min_equity_ratio` 0.40、`close_cost`、`Note{ManualExternal}` | portfolio-carry | pc |
| `Views::leverage(a, i) -> Option<u32>`、`RiskParams.max_perp_leverage_x100` 300 | 杠杆 | SDK;hft-risk |
| `FieldDesc.structural`、`ModelParamEffect{None, ResetGroups, ResetAll}`、`Note{ModelGroupReset}`、`ModelLayoutDesc`、`Model::mem_bytes`、`Model::snapshot_bytes`、`Model::snapshot_chunk`、`SnapshotProgress`、`advisor.mem_budget_mib`、`ParamInvalid{ModelMemBudget}` | Model | SDK;hft-core;control-plane |
| `GroupSpec::fingerprint()`、`spec_seq` | group | SDK;sequencer;GroupId 语义 hft-types |
| `NoteLevel{P0=1,P1=2,P2=3,Info=4}` | 码表 | hft-event |
| `SnapshotHdr.purpose Drain=5`、`StrategyRowHdr.desc_version`、Summary `sflags`/`skipped_inflight`、`MarginMode::Mixed=4` | 码与布局 | journal-format/hft-sequencer/hft-event/hft-refdata |
| `replayer compare`、`replayer sim --ab`、`replayer verify --model-checkpoint` | 工具 | replayer |
| `status: frozen` | frontmatter | crates/AGENTS 模板 |

新 Note 码一律由 J6 在 hft-event 登记(source 与 level 照上表);新 Timer 无。

## 9. 最终报告(简短)

1. 改了哪些文件。2. D1–D12、§2、§3 中你负责的各项落在哪里(文件 + 小节)。3. V3 表中你处理了哪些行号、哪些留给别人或冻结。4. 本简报 §8 之外你新引入、别人需要用的名字(应很少)。5. 仍有风险的地方(≤ 5 条)。

## 10. 主控合并裁决(修订与合并过程中)

1. advisor 写检查点(分块)期间**整个 advisor** 不消费 journal(镜像为各实例共享),各块同一 `last_seq`,落后计入 `AdvisorLag`(SDK、hft-core、hft-sequencer 一致)。
2. SDK `StrategyDesc.version` 为 u16(= `StrategyRowHdr.desc_version`)。
3. `SimReset` 前置唯一定义在 control-plane:无 Live cell、无活动目标、无 `Unknown` 槽;其余非终态槽(挂单、在途)由 OMS `sim_reset` 按撤销终结,sim-gw 同时撤单。
4. 关键帧按节分片:hft-journal `JournalWriter::mark()/rollback()`;hft-book、hft-oms、hft-exec、hft-ledger、hft-risk 提供 `snapshot_section(sec, w)`;节 7 写出的 `Top.rx_tsc` 置 0。
5. 节 18 `GateRow` 增 `conns_ok_venues: u64`(48 B,行版本 3)。
6. `hedge_host` 样本来自 latency-probe `pair` 模式,需要 `feed_env = "testnet"`:门槛 ⑩ 在 7 天主网行情窗口之外另取 ≥ 7 天测试网行情时段采集(单独成数据集)。
7. BBO 拆成独立任务时的环与 journal 名加 `-bbo` 后缀(`feedctl-<v>-bbo`、`snapreq-<v>-bbo`、`snap-<v>-bbo`、`raw-feed-<v>-bbo`),命名与分发归 hft-venue §1;hft-core、hft-sequencer、hft-event、journal-format 已登记。
8. SDK `LegTrack` 拆为 `inflight_immediate`/`inflight_resting`(32 B;`LegView` 160 B);`ExternalFillView.deadline_tsc = 0` 表示引擎不做默认处理。
9. 门槛 ⑩ 硬线(J9 口径,待负责人确认):`tick_to_trade` 以验收下限 30/100 µs 为硬线,基准形态 22/75 µs 为目标,落在两者之间须 Admin 签字;`hedge_host` 每小时样本不足,同机干扰改为开/关 sim 小时合并比较。
10. `hft-verify` 或子进程被 `SIGKILL`(cgroup OOM、护栏、超时)= 中止,不算失败、不清零;libFuzzer 自报的 `-rss_limit_mb` 超限才是发现。
11. ctl-proto 新主体 `issuer = ops_bot`(uid `hft-opsbot`,M0-e 更正;不代表 Telegram 用户):只允许非 HARD、不带 flatten 的 `Kill`(恢复后补发待补 Kill)与 `Annotate{OpsNote}`。
12. watchtower `ExternalFill{cause = Manual}` 为 P1,其余 P0。
13. 两实例账户名不相交:sim 实例账户名以 `sim-` 开头,prod 不得以它开头(hft-core `AccountNamePrefix`)。
14. sequencer 恢复"心跳"一节(第八轮初次修订时丢失)。
