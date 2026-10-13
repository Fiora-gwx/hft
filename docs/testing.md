# 测试

本页定义测试金字塔:每一层测什么、放在哪、何时跑、门槛是多少,以及 CI 运行矩阵与服务器上的累计运行(§6)与**上线门槛**(§8)。用例的写法、编号与追溯检查归 [docs/tests](tests/README.md):组件自己的用例在各组件的 `TESTS.md`,跨组件场景在 [scenarios](tests/scenarios.md),全局不变量在 [invariants](tests/invariants.md),主机、测试网与运维演练在 [drills](tests/drills.md);本页只放全仓统一的层次、位置约定、CI 运行矩阵与门槛。来源:[architecture-v2/10 §5](../design/10-语言分工与代码组织.md)、[11 实施路线图](../design/11-实施路线图.md)(里程碑 M1.5–M8)、[research/04 §7、§9](../design/research/04-oss-frameworks-and-patterns.md)、[research/06 §4](../design/research/06-data-plane-replay-observability.md)、[13 O8](../design/13-设计评审与优化.md)、[负责人决定记录](../design/_work/round8-decisions.md)。

## 1. 原则

1. **测试描述行为**。断言写"输入什么、期望看到什么事件/状态",不写"调用了哪个内部函数"。测试名是一句行为陈述:`fill_before_ack_moves_slot_to_filled`,不是 `test_oms_3`。
2. **金样不对,修 fixture 来源,不修归一化逻辑去迁就**。金样的期望输出来自交易所文档与人工核对,不是来自当前代码的输出;禁止为了让金样变绿而在解码器里加特例。流程见 [fixtures/](../fixtures/README.md#更新流程)。
3. **失败路径与成功路径同等**。每个状态机的每条边至少一个用例;失败路径用例(拒绝、超时、断连、乱序、重复、截断)不少于成功路径。
4. **走真实入口**。测试经公开 API 或真实连接驱动,不为测试新增生产导出;venue 解码器在实盘、测试网、raw 重放、金样与 fuzz 中是同一份代码;撮合器在模拟账户、`replayer sim`、DST 中是同一份代码([hft-sim](../crates/hft-sim/README.md))。
5. **时间与随机数可注入**。测试中不读系统时钟(规则 [T4](hot-path-rules.md#线程t)、[D2](hot-path-rules.md#确定性d));时间来自事件,随机数来自种子。
6. **测试不联网**。所有输入来自 `fixtures/` 或种子生成;需要交易所真实行为的检查属于测试网/模拟盘联调、模拟账户长跑与正式实盘的运行监控,不是单元测试。
7. **不以重试换绿**。一个测试第一次失败就是失败;不稳定的测试在同一天内修复或以 Agent Note 记录后隔离,不加自动重试。
8. **CI 是唯一的构建环境**,只用 GitHub 托管 runner:PR 与每夜主批次在 `ubuntu-latest`(x86_64),每夜 aarch64 专项、发布构建与验证包构建在 `ubuntu-24.04-arm`(§6 与 [development](development.md))。本地只写代码,不要求能编译运行。**服务器不编译**:dev2(以及 M7 期间的升级主机)只安装 CI 产出的发布制品与验证包——前者用于测试网/模拟盘联调与模拟账户长跑,后者以 `hft-verify` 运行累计 fuzz 与 DST(门槛 ③④,§6.1);dev2 不放正式实盘账户,不做延迟验收。

## 2. 金字塔

| 层 | 目的 | 位置 | 何时运行 | 门槛 |
|---|---|---|---|---|
| 单元 / 属性 | 纯函数与状态机的不变量:定点运算、簿增量、OMS 状态机、执行层算法、风控检查链与 MarginStress 档位、ledger 记账(净量、库存、余额基线、资金费校正)、模拟撮合 | `crates/<x>/tests/`、`src/` 内 `#[cfg(test)]` | 每个 PR | 全绿;`proptest` 失败的最小化种子写进 `tests/regressions/` 永久保留 |
| 策略测试 | 单策略在 `MockViews` 上的场景:意图、目标、定时器、快照往返、零分配、Note 码与描述符一致 | `strategies/<x>/tests/`(用 [hft-testkit `StrategyHarness`](../crates/hft-testkit/README.md#4-策略测试桩)) | 每个 PR | 全绿 |
| 生产策略集成 | 已注册的生产策略经真实 sequencer 跑 DST 场景与压力用例(hft-testkit 不依赖 registry,这类测试放在依赖 registry 的一方) | `strategies/registry/tests/`、`apps/hft-core/tests/` | 改动 `strategies/` 或引擎 crate 的 PR | 全绿 |
| 解耦门槛 | `scaffold-gate`:用模板生成一个临时策略并注册、编译、跑场景与 DST 短集,引擎 crate 与 app 零改动;`decoupling-gate`:同一 PR 同时改策略与引擎即失败(豁免规则归 development) | `cargo xtask scaffold-gate`([hft-testkit §5](../crates/hft-testkit/README.md#5-解耦门槛夹具))、`cargo xtask decoupling-gate`([tools](../tools/README.md#cargo-xtask-decoupling-gate)) | `scaffold-gate`:改动 `crates/`、`apps/`、`strategies/`、`tools/` 的 PR;`decoupling-gate`:每个 PR,评审或标签变化时由 `decoupling.yml` 单独重跑 | 通过 |
| 解码金样 | 每 venue 每消息类型:真实 payload → 期望归一化事件逐字段相等 | `fixtures/<venue>/`;驱动在 `crates/venue-<x>/tests/golden.rs` | 每个 PR;交易所改协议时更新 | 全绿;覆盖要求见 [fixtures](../fixtures/README.md#测试与验收) |
| 模拟撮合金样 | 一段行情 + 指令脚本 → 期望回报序列 | `fixtures/sim/` | 每个 PR | 全绿 |
| Fuzz | 解码器与 journal 读者对任意输入不 panic、不越界、不死循环;十进制解析与参考实现一致 | `crates/<x>/fuzz/` | PR 冒烟只在改动解码器、journal 读者或十进制解析时跑改动的目标(各 60 s);CI 每夜按目标短跑轮转(回归冒烟,不计入 ③);上线门槛 ③ 由服务器上的验证包累计(§6.1) | 零失败(panic、崩溃、单输入超时,超时口径见 §3.3);崩溃输入最小化后进 `fixtures/<venue>/regressions/` |
| 微基准 | 单个热原语的延迟与分配 | `crates/<x>/benches/`(Criterion) | 每个 PR 编译;改动热 crate 或 `benchmarks/` 时短跑(数值**只报告、不阻断**);从 M1.5 起每个里程碑结束时在按需租用的参考机上判定上限与相对回归 | 见 [benchmarks/](../benchmarks/README.md#回归阈值) |
| 假交易所集成 | **VenueProtocol 级**假交易所(撮合器即 hft-sim,venue 行为层与故障注入取自 `hft-testkit`,不走 socket)驱动完整 sequencer + venue 运行时(`hft-venue`) | `apps/hft-core/tests/`(hft-testkit 不依赖 hft-venue,需要 venue 运行时的场景放在这里,如 [scenarios](tests/scenarios.md) 的 C12、FP-8) | 每个 PR | 场景断言全过 |
| 测试网/模拟盘联调 | Binance 测试网、OKX 模拟盘、Bitget 模拟盘上的真实连接:下单/撤单/成交/断连/重同步、账户配置与 UID 查询、回报解码、kill-switch 撤单 | `tests/testnet/`;在 dev2 上以指向测试网端点、持测试网 key 的 `prod` 角色实例运行:`feed_env = "testnet"`、`order_env = "testnet"`,refdata 只含测试网账户与测试网合约;只在联调窗口运行(`hft-window start\|stop`,[deploy](../deploy/README.md));用已安装的发布制品,护栏内 | 联调窗口内每日(非阻塞告警);上线门槛 ⑥ | 无未解释异常 |
| DST(确定性模拟) | 种子驱动的假交易所(hft-sim 撮合器 + venue 行为层)+ 故障注入 × sequencer/OMS/风控/执行层/ledger;单线程按种子决定全部交错,失败打印种子 | `crates/hft-testkit`(框架、场景套件与 `hft-dst` 运行器);`tests/dst/`(cargo test 入口) | 每个 PR 冒烟 200 种子 + 回归集;CI 每夜一批由日期派生的种子(回归冒烟,不计入 ④);服务器上 `hft-verify` 跑新种子并累计(④,§6.1);策略/执行层/风控/OMS 改动必跑 | 不变量零违反(§4);失败种子进 `fixtures/dst-seeds/` 回归集 |
| 验证回放 | journal → 状态哈希序列与意图流逐字节一致 | `apps/replayer`;`cargo xtask replay-verify` | 每日在运行该实例的主机上运行,补跑所有缺报告的 UTC 日(§6);PR:策略/执行层/风控/OMS/sequencer 改动跑 `fixtures/replay/` 样本 | 逐字节一致;不一致即红,附第一个分歧的 `seq` |
| 模拟账户 | 实时行情上的 Sim 账户长跑:策略与实盘同码,订单进 sim-gw;每日账户日报与策略日报 | dev2:`sim` 实例(只含 Sim 账户、不持任何 key,护栏内);升级主机:与 `prod` 同机的 `sim` 实例;watchtower 日报 | 持续;上线门槛 ⑦ | 见 §8 |
| 恢复与混沌 | 重连伴随重同步;kill 在每个线程生效;控制命令乱序/重复/过期;RAM 段崩溃后补完复制;`kill -9` 后 `RecoverDriver` 冷启动;毒丸隔离;从快照恢复;dev2 护栏 | `tests/recover/`(进程内,随 DST 与每夜运行);`tests/chaos/`(测试网/模拟盘 + 故障代理,dev2 联调窗口内) | 进程内用例每夜;测试网混沌每周(联调窗口内);发布前 | 全部场景通过;任何"重连后未重同步"视为 P0 |
| 全链路延迟 | `tick_to_trade`、`rep_to_order`、`hedge_host` 等主机内分段的直方图与预算比 | M1.5 延迟探针与 M3 末满订阅探针:按需租用实例(只记录,结论写 Agent Note,M3 末探针据此定档,[11](../design/11-实施路线图.md));升级主机上的生产直方图(订单类分段,样本来自 [latency-probe](../strategies/latency-probe/README.md))与按事件加权的离线统计(行情类分段)(门槛 ⑩,§8.2) | M1.5;M3 末;M7 在升级主机上 `prod` 与 `sim` 同机运行时 | 硬线与目标见 §8.2;p99.9 只记录 |
| 可选扩展:wire 级 mock 回环 | mock 按各所格式回放捕获报文 + 故障注入,驱动 hft-core **生产二进制** | `apps/mock-exchange`(**冻结**,解冻前先按 core 规格复核);场景在 `tests/mock/` | 解冻并启用后:短场景进 PR、长场景进参考机运行(触发条件见 [mock-exchange](../apps/mock-exchange/README.md)) | 场景断言全过;预热后分配次数 = 0 |

"每个 PR" = PR CI(`ubuntu-latest`);"每夜" = CI 对 `main` 的定时回归冒烟(`ubuntu-latest` 主批次 ≤ 15 min + `ubuntu-24.04-arm` 专项 ≤ 10 min);"发布" = 版本标签触发、在 `ubuntu-24.04-arm` 上的 aarch64 作业;"服务器累计" = 服务器上 `hft-verify` 运行验证包(§6.1);频次与时长上限见 §6,CI 资源规则见 [development](development.md#免费额度)。参考机的定义见 [benchmarks/](../benchmarks/README.md#参考机与-ci)。

## 3. 各层细则

### 3.1 单元与属性

- OMS 用 `proptest-state-machine`:参考模型与被测 OMS 同步步进,覆盖 fill-before-ack、cancel-reject-because-filled、重复回报、乱序回报、未知 client id 回报(含暂存后归属与超时转 `ExternalFill`)、`Unknown` 收敛、墓碑期迟到成交([research/04 §7](../design/research/04-oss-frameworks-and-patterns.md))。
- 定点:随机资产指数 × 随机价格/数量,对照 i128 十进制参考实现;覆盖 BTC 1e-8 数量步长、PEPE 类 1e12 数量([13 R4](../design/13-设计评审与优化.md))。
- 簿:随机分档快照(`L2Snapshot{origin = TopN}`)按整替换应用到稠密数组簿与参考 `BTreeMap` 簿,每次比对 BBO 与全部档;增量深度启用时(可选扩展),随机增量序列同样每步比对 BBO 与前 N 档。
- ledger:任意成交序列在四种 `fee_asset_rule` 下,仓位/库存按净量、费用晚到时补差、`net_qty` 与交易所余额一致;余额基线与 Adopt 余额条目;资金费自动校正(`AdjustDelta::Funding`)只作用于结算时刻早于 `funding_recon_lag_ms` 的差额,且同一笔不重复校正。
- 执行层:目标跟踪、切片(两腿 lot 对齐)、腿顺序、对冲量上限、Broken 有界自动平仓的属性(见 [hft-exec](../crates/hft-exec/README.md))。
- 风控:MarginStress 按到强平距离 `liq_dist` 分档(交易所 `liq_px` 与 ledger 估算取更保守者;比率交叉校验),档位边界与滞回(见 [hft-risk](../crates/hft-risk/README.md))。
- 模拟撮合:资金守恒、记忆衰减、拒单规则(见 [hft-sim](../crates/hft-sim/README.md#测试与验收))。
- 可选扩展(链上):tx 生命周期与 nonce——传输错误序列只能产生 `Unknown`;nonce 只在 `IncludedOk`/`Dropped` 后推进或回收。

### 3.2 金样

- 组织、来源、脱敏、覆盖要求与更新流程归 [fixtures/README.md](../fixtures/README.md)。core 覆盖 Binance、OKX、Bitget 的现货与 USDT 永续;行情消息按 core 行情范围(BBO、逐笔成交、标记价/指数价、资金费、BBO 补帧,需要深度时的分档快照流;定义归 [hft-venue](../crates/hft-venue/README.md)),增量深度与 REST 深度快照的金样随该可选扩展启用。
- 金样测试比对**全部字段**(含 `ex_ts_ns`、`ex_send_ns`、`update_id`、`flags`),不比对 `rx_tsc`/`hw_rx_ns`。
- refdata 版本随金样固定(`meta.toml` 记录);资产指数变化 = 新金样,不改旧金样。

### 3.3 Fuzz

- `cargo-fuzz`(固定日期的 nightly 工具链,见 [development](development.md#工具链));种子语料 = 该 venue 的全部金样输入 + 变异。
- 解码器入口签名与实盘相同(`&[u8]` → 事件或错误位);fuzz 目标不调用非公开函数。core 解码器每个协议一个 fuzz 目标(`binance-usdm`、`binance-spot`、`okx-swap`、`okx-spot`、`bitget-usdt-futures`、`bitget-spot`,目标内按首字节分派到行情、用户流与 REST 响应解码);另有 `hft-journal` 读者、`hft-event` 解码与十进制解析目标。
- **计入门槛 ③ 的只有上面 6 个 core 解码器目标**。其他 fuzz 目标(journal 读者、`hft-event` 解码、十进制解析,以及各 crate README 列出的其他目标)照常在 PR、CI 每夜与服务器上运行,失败同样必须修复,但不计入 ③。
- 差分目标:十进制字符串 → 定点解析,对照参考实现;不一致视同崩溃。
- **单输入超时**:每个 fuzz 目标的 harness 在处理一个输入前后读本线程 CPU 时钟(`CLOCK_THREAD_CPUTIME_ID`),> 10 ms 即记为"超时";不按墙钟判定(节流与调度不造成超时)。**存法**(本条是唯一归属):harness 把该输入写进环境变量 `HFT_FUZZ_TIMEOUT_DIR` 指定的目录(文件名 = 输入的 SHA-1;未设置时写当前目录下的 `timeouts/`),然后**正常返回**——不 `exit`、不 `abort`(libFuzzer 会把回调里的退出记为崩溃并写 `crash-` 文件);目录由 `hft-verify` 设置并在运行后归入疑似(归类归 [tools](../tools/README.md#hft-verify))。libFuzzer 的 `-timeout` 只作兜底,取 ≥ 600 s(它按墙钟判定,批处理在 `hftbatch.slice` 内被饿时不应误触发)。PR 冒烟、CI 每夜与服务器累计用同一口径。
- **CI 冒烟**:PR 只跑改动涉及的目标(各 60 s);每夜 `cargo xtask fuzz --nightly` 按轮转表 `tools/xtask/fuzz-targets.toml` 每目标短跑,合计 ≤ 4 min。语料经 `actions/cache` 按目标保存(键 `fuzz-<target>-<run_id>`,恢复前缀 `fuzz-<target>-`);缓存被逐出只损失覆盖。语料不入库。CI 的运行不计入 ③。
- **服务器累计**:验证包(§6.1)为每个目标带一个 libFuzzer 二进制(固定 nightly + ASan)与初始语料(金样输入 + 打包时的 CI 语料缓存)。`hft-verify` 按轮转表在预算内运行,子进程以 `ASAN_OPTIONS=quarantine_size_mb=16` 运行并另设 `-malloc_limit_mb`(与 `-rss_limit_mb` 的取值关系归 tools),计量按子进程 CPU 时间;受限主机上的内存超限与超时先记**疑似**(§6.1 规则 1);语料在服务器状态目录里持续增长(可再生,不上传);运行记录、账本快照与崩溃输入由 journal-archiver 的 `files` source 备份到 S3 `hft/verify/`;崩溃输入进崩溃目录(`hft-verify crash list` 列出),由人最小化后经 PR 写进 `fixtures/<venue>/regressions/`,成为每个 PR 的回归金样。命令与目录归 [tools](../tools/README.md),验证包与安装归 [deploy](../deploy/README.md)。

### 3.4 DST

- 框架在 [`hft-testkit`](../crates/hft-testkit/README.md#6-dst-框架):一个种子决定假交易所的回报时序、故障注入点、定时器触发与输入环交错;被测 sequencer 以回放驱动方式单线程运行;框架对策略集泛型。场景套件(以 testkit 的两个测试策略运行)在 hft-testkit 内,同一组套件由 `tests/dst/` 的 cargo test 入口、`cargo xtask dst` 与验证包里的 `hft-dst` 运行器调用;以已注册生产策略运行的场景在 `strategies/registry/tests/`。
- 故障集(目录见 hft-testkit):断连、序号缺口(增量深度启用时)、重复/乱序/延迟回报(灰色故障)、限速拒绝、部分成交、第二腿被拒、**第二腿盘口长时间不变而连接健康**(必须照常对冲)、第二腿 BBO 连接失活或处于重同步、第二腿资金不足、两腿 lot 不齐、对手价超滑点、Broken 长时间无人处理、无 client id 成交(暂存后归属或超时转外来成交)、`ReconcileReport` 与本地不一致(含资金费逐笔差异及其在余额上的同额差)、控制命令过期、**未登记的交易所错误码**(期望 `Unknown`,不是 `Reject`)、**无过期保护的所返回 NotFound**(只能由对账收敛)、到强平距离路径(含交易所 `liq_px` 与 ledger 估算不一致)与数据过期、**单边 +30%(1 h 内)行情**(含只有一所变价)、**L3 期间另一所 BBO 断开或下单连接断开**、L3 期间策略挂着 reduce-only 平仓单、L3 中途重启、自身强平/ADL 外部成交、用户流断开期间的 ADL(只能由对账发现)、人工外来成交(`manual_hold`)、残余介于尘埃线与最小下单量之间、基础币扣费与费用晚到、账户配置与 UID 不符、全仓资金费只推余额、未知用户流事件、下架公告、persist 滞后与磁盘低水位、同一输入连续崩溃、补腿进行中崩溃、长停机后冷启动、`stop_core` 停机标记后的重启、Drain 期间在途链接、`engine_flatten` 之后策略继续发目标、advisor 落后、模拟回报洪峰、Viewer 紧急停止。[scenarios](tests/scenarios.md) 中级别为 `dst` 的每个用例对应一个场景。
- **种子来源**:PR 冒烟用由提交 SHA 派生的 200 个种子 + `fixtures/dst-seeds/` 回归集;CI 每夜用由日期派生的一批种子(回归冒烟;`cargo xtask dst` 的参数与种子派生方式归 [tools](../tools/README.md#cargo-xtask-dst));服务器上 `hft-verify` 分配与账本中全部已有区间(全部主机)不重叠的新种子区间。上线门槛 ④ 的累计只计服务器上的新种子。
- 可选扩展(链上,chain-core 启用时):回执源 403/超时、多源不一致、重组、泄露/重组回灌后迟到上链——期望该交易因合约 `maxBlock` 检查而 revert,状态机不因迟到回执产生仓位([tx-onchain-expiry](../.agents/notes/proposed/architecture/2026-10-01-tx-onchain-expiry.md))。
- 失败输出:种子、第一个违反的不变量、违反前 200 条事件的 journal 片段(可直接喂 `replayer`)。

### 3.5 验证回放

- 输入:要验证的 UTC 日的 seq journal(通常是前一日;补跑规则见 §6;运行该实例的主机上的本机冷副本:dev2 的 `sim` 实例与联调窗口内的测试网实例;升级主机的 `prod` 与 `sim`),或 PR 用的 `fixtures/replay/` 样本(S3 指针 + 哈希,CI 按哈希缓存);含 Sim 账户的 journal 同样适用(模拟回报是 Input)。
- 输出比对:每个 Output 记录(`OrderDecision`、`ExecDecision`、`CmdResult`…)逐字节相等;每个 `StateSnapshot` 的状态哈希相等。
- journal 自足([13 R6](../design/13-设计评审与优化.md)):回放只读 journal,不读任何外部配置文件。
- **Meta 类记录不比对**(分类见 [hot-path-rules D5](hot-path-rules.md#确定性d));测试:改动 Meta 内容后比对结果不变。
- **每日 `--advice recompute` 的起点**:`replayer verify --journal <D 的 seq journal> --advice recompute --model-checkpoint <path>` 对每个要验证的日 D 跑一次,起点是 journal-archiver 为 D 归档的 Model 检查点(D 的 UTC 00:00 之后写出的第一份,[journal-archiver](../apps/journal-archiver/README.md#model-检查点每日归档));从检查点的 `last_seq` 起,策略 `Model` 从每条 Advice 的 `basis_seq` 处重算,按 replayer 的配对键与 journal 中的 Advice 配对后载荷逐字节相同(重算多出而 journal 中没有的——被 advice outbox 合并掉的——计 `coalesced`,不算分歧;配对键与配对规则归 [replayer](../apps/replayer/README.md#verify))(Model 确定性:检查点含原始样本节与路径状态节,"检查点 → 恢复 → 续算"与连续运行逐字节相同,规则归 [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md));`InjectWarmup` 是 Input,重算时照常投递。路径状态节被丢弃(恢复的检查点路径节版本与 `Model::PATH_VERSION` 不符,或当日 journal 中有 `Note{ModelPathReset}`)时,依据时刻落在该检查点 `last_seq` 之后 `Model::path_settle_ms` 之内的 Advice 不比较、其后照常逐字节比较(规则归 SDK),派生状态重建点写进报告的 `recompute_from_seq`(取法归 [replayer](../apps/replayer/README.md#verify))。当日没有归档检查点 → 跳过当日 recompute,报告标 `recompute = skipped`,watchtower 报 P2(不使门槛 ⑦ 重新计时,§8.1)。
- **每日验证报告**(`hft-replay-verify` 写 `/var/lib/hft/replay-verify/<instance>/<date>.json`,`<date>` 是被验证的 UTC 日,不是运行日;watchtower `ReplayVerify` 规则读取):`{instance, date, release, verify: "pass"|"fail", first_divergent_seq?, component?, recompute: "pass"|"fail"|"skipped", coalesced?, checkpoint_key?, recompute_from_seq?, skipped_reason?, generated_utc, input_continuity, recovery_gaps}`(恢复缺口字段的含义与通过范围归 [replayer verify](../apps/replayer/README.md#verify) 第 4 条;`coalesced` = 当日重算多出、不算分歧的 Advice 条数,`recompute = skipped` 时缺省;`checkpoint_key` 是所用检查点键的列表,每个带 Model 的策略实例一项 `{strategy_instance, kind_id, layout_version, schema_id, last_seq}`;`skipped_reason` ∈ `advice_journal`(未重算)、`model_no_start`(带 Model 的实例没有可用起点,含检查点键不符)、`input_incomplete`、`incompatible`(该日 verify 以退出码 3、4 结束);各字段的填法归 [replayer verify](../apps/replayer/README.md#verify) 第 9 条)。
- 验证回放是**离线**工具;生产冷启动用 `RecoverDriver`(§3.6),不以逐字节一致为启动条件。

### 3.6 崩溃恢复与冷启动

生产冷启动的规则归 [`hft-sequencer` 的 `RecoverDriver`](../crates/hft-sequencer/README.md#冷启动恢复recoverdriver) 与 [hft-core 崩溃隔离与从快照恢复](../apps/hft-core/README.md#崩溃隔离与从快照恢复),必测用例 RC-1–RC-12(`tests/recover/`,DST 与混沌共用)见 [scenarios §3](tests/scenarios.md#3-崩溃恢复与冷启动)。

## 4. 必须在 DST 与属性测试中持续成立的不变量

不变量 INV-1–INV-20(序号即原编号;"不变量 n" = INV-n)的定义与每步检查条件见 [invariants.md](tests/invariants.md),hft-testkit 的检查器 `I<n>` 逐条实现。

### 4.1 模式、账户与人工干预

用例 MD-1–MD-25 见 [scenarios §4](tests/scenarios.md#4-模式账户与人工干预)。

### 4.2 恢复、OMS 与对账场景

用例 `B-n`(B-1–B-22 中在用的编号)见 [scenarios §5](tests/scenarios.md#5-恢复oms-与对账)。

### 4.3 执行正确性用例

用例 A5、B1–B6、C1–C15 见 [scenarios §6](tests/scenarios.md#6-执行正确性)。

### 4.4 断腿、第二腿、保证金、资金费与建议新鲜度用例

用例 BK-1–BK-7、QF-1、LK-1–LK-4、LT-1、FL-1、MS-2、MS-2a、MS-3–MS-6、AX-1、FD-1、FD-2、AL-1 见 [scenarios §7](tests/scenarios.md#7-断腿第二腿保证金资金费与建议新鲜度);标"必过"的 QF-1、MS-2、MS-3(④)与 MS-2a、MS-4(⑦)是上线门槛的点名项(§8)。

## 5. OMS 与执行的必测失败路径

用例 FP-1–FP-12(来自 [05 §8](../design/05-CEX执行层.md))见 [scenarios §8](tests/scenarios.md#8-oms-与执行的必测失败路径)。

## 6. CI 运行矩阵

RPL-012 的文件与真实 Model 随机回放在独立 `replay-dst` 作业运行,PR/每夜的种子数归 [用例](../apps/replayer/TESTS.md#rpl-012-dst-生成的种子-journal-全部回放一致)。作业使用 `ubuntu-latest`,安全上限 30 分钟,为独立冷构建和逐种子文件回放留出时间;它不占用或削减下表既有 DST、HashTrail 与基准验证。调度与制品归 [development](development.md#质量门quality-gatesyml)。

只用 GitHub 托管 runner(PR 质量门在 `ubuntu-latest`,aarch64 发布构建与验证包在 `ubuntu-24.04-arm`);**不注册自托管 runner**,服务器不跑 CI、不编译。服务器只安装 CI 产出的发布制品与验证包,并以 `hft-verify` 运行累计 fuzz 与 DST(§6.1)。工作流文件、作业图、构建剖面与发布步骤归 [development](development.md),制品与验证包的格式与安装归 [deploy](../deploy/README.md);公开仓库的免费范围与资源规则唯一归属 [development 免费额度](development.md#免费额度)。本节定义每类作业跑什么与时长上限。

| 时机 | 在哪里 | 运行 | 时长上限 | 阻断 |
|---|---|---|---|---|
| PR(opened/synchronize/reopened)与 `main` 推送(`ci.yml`) | `ubuntu-latest`(x86_64) | **常驻**:fmt、clippy、`cargo deny check licenses bans`、`deps`、`hot-check`、`gen-schema --check`、`doc-check`、单元/属性、策略测试、金样(解码与模拟撮合)、`fixtures check`、DST 冒烟(200 种子 + 回归集)、`HashTrail` 金样核对(`fixtures/dst/hashtrail.toml` 一致性集登记顺序的前 50 个种子,`cargo xtask dst --hashtrail check --first 50`,[tools](../tools/README.md#cargo-xtask-dst))、基准编译、`decoupling-gate`(PR 差异检查,不构建)。**按路径**:改动 `crates/`、`apps/`、`strategies/`、`tools/` → `scaffold-gate` 与生产策略集成测试;改动解码器、journal 读者或十进制解析 → 改动的 fuzz 目标各 60 s;改动策略/执行层/风控/OMS/sequencer → `fixtures/replay/` 样本 `replay-verify`;改动热 crate 或 `benchmarks/` → 基准短跑(数值只报告);改动 `rust-toolchain.toml` → 同一作业内对 base 与 head 各跑一次基准短跑并报告相对差(只报告,[hot-path-rules B4](hot-path-rules.md#构建b));改动 `python/`、`deploy/`、`.github/` 或 CI 回归用例 → `ops-py` 作业的对应步骤(deploy 的 systemd 容器测试按 [development](development.md#ci-概要) 的 `gen-host` 路径判定运行);只改文档 → 只跑 `doc-check`(与不构建的 `decoupling-gate`,它判定本次是否只改文档)。基准编译与短跑用 `ci` 剖面(不用 `bench`/`release-hot`);`scaffold-gate` 复用本作业的主 target 目录 | 目标 ≤ 12 分钟(含构建);`timeout-minutes: 20`,`rust` 作业 30(安全上限:托管 runner 速度波动约 ±50%,给冷构建留出完成测试的时间) | 合并 |
| PR 评审提交、标签增删(`decoupling.yml`) | `ubuntu-latest` | 只重跑 `decoupling-gate`(豁免需 `engine-change` 标签 + Admin 批准,规则归 development) | — | 合并 |
| `main` 每夜(x86 主批次) | `ubuntu-latest` | **回归冒烟**:`HashTrail` 金样核对(一致性集全部 500 个种子,`cargo xtask dst --hashtrail check`)与一批由日期派生的 DST 种子(种子数与步数归 [tools](../tools/README.md#cargo-xtask-dst)),两者合计 ≤ 6 min;fuzz 按轮转表每目标短跑(合计 ≤ 4 min);`tests/recover/` 进程内恢复用例;deploy 检查全量运行(含容器测试,`deploy/` 存在才执行)。**不计入 ③④**。 | ≤ 15 min(含构建) | 下一次发布 |
| `main` 每夜(aarch64 专项) | `ubuntu-24.04-arm` | 只编译并运行与架构有关的测试:`hft-rt` 的 TSC 读取(`cntvct_el0`)与换算、原子与内存序(SPSC 环、journal 提交游标的 Acquire/Release,多线程压力)、`hostprobe` 主机检查、`neoverse-v1` 特性核对;工具链与依赖缓存用自己的键,不与 arm 发布作业共用(缓存键归 development) | ≤ 10 min(`timeout-minutes: 10`;含容器内装工具链) | 下一次发布 |
| 正式版标签 `vX.Y.Z` | `ubuntu-24.04-arm`(aarch64);一致性集的 x86_64 一侧在同一工作流的 `ubuntu-latest` 作业 `conformance-x86` | aarch64 一致性集:单元/金样、固定 500 个 DST 种子在**同一提交**上于 x86_64 与 aarch64 各跑一次,两侧的 `HashTrail` 都与该提交的金样 `fixtures/dst/hashtrail.toml` 逐字节比较(不变量 9;金样的生成与 PR 核对归 [development](development.md),登记归 [fixtures](../fixtures/README.md))、`fixtures/replay/` 样本 verify;通过后构建发布制品;`verify-bundle.yml` 构建同一标签的验证包(§6.1,正式版必跑) | 发布目标 ≤ 30 min,`timeout-minutes: 90`;验证包 `timeout-minutes: 60` | 发布 |
| 开发构建 `vX.Y.Z-dev.N`(标签,或对 `ci.yml` 全绿的 PR 头提交 `workflow_dispatch(sha, dev_n)`) | `ubuntu-24.04-arm` | 只构建 dev2 需要的二进制(`release` 剖面),跳过 aarch64 一致性集;不自动出验证包(需要时对该提交 `workflow_dispatch` `verify-bundle.yml`) | — | dev2 安装 |
| 每季度 | `ubuntu-latest` | 应急路径演练 `release-x86-cross.yml`:交叉编译 aarch64 发布归档(工作流与验收归 [development](development.md)) | — | — |
| 服务器常态(`hft-verify.timer`) | dev2(常态,护栏内,在 `hftbatch.slice` 内只用空闲 CPU,CPU 权重归 hft-core);升级主机只在 M7 期间且在门槛 ⑩ 的 7 天测量窗口之外;M8 起只在 dev2 | `hft-verify run --budget-min N`:验证包里 core 解码器的 fuzz 目标与 DST 新种子,写运行记录、更新服务器账本(§6.1) | 不占 CI 额度;预算与单元设置归 deploy | 上线门槛 ③④ |
| 每日 | 运行该实例的主机(dev2;升级主机),已安装的发布制品,不是 CI | `hft-replay-verify`:对每个缺报告的 UTC 日(通常只有前一日)的 journal 跑 `replayer verify` 与 `--advice recompute`(本机冷副本,起点为该日归档的 Model 检查点,§3.5;补跑规则见下);`hftbatch.slice` 内低优先级,运行期间 `hft-journal-archiver` 与 `hft-pipeline` 暂停(单元设置归 deploy);dev2 护栏一级动作最先停 `hft-verify`,其次停它;联调窗口内的测试网/模拟盘冒烟(非阻塞告警) | 不占 CI 额度 | 上线门槛 ⑦ 记录 |
| 每周 | dev2(护栏内,联调窗口内) | 测试网/模拟盘混沌(故障代理);交易所侧复测不在 dev2,在生产 AZ 的探针与生产机上做([az-probe 手册](runbooks/az-probe.md)) | — | 下一次发布 |
| 发布前 | dev2 | 前一日 journal verify 通过、kill 与死手演练(测试网/模拟盘账户)、版本变化冷启动与回滚演练(§3.6);改动 `venue-*`、`hft-venue`、`hft-net`、`hft-oms` 的版本先在 dev2 测试网联调窗口装同一版本冒烟 | — | 激活 |
| 里程碑结束(从 M1.5 起) | 按需租用的参考机(与目标主机类同型,不是 CI runner;`[需授权]` 由负责人开机) | `bench-gate --reference`(单 crate 上限与相对回归;主机类选定后加路径绝对门槛,[benchmarks](../benchmarks/README.md#参考机与-ci)) | ≤ 60 min | 里程碑验收;M7 起另阻断发布 |
| 正式实盘前 | 升级主机(`prod` 与 `sim` 同机运行) | 门槛 ⑩ 延迟验收(§8.2) | 7 天 | 正式实盘 |

- **x86_64 跑质量门**:纯逻辑 crate 跨平台;架构相关代码(`hft-rt` 的 TSC、`hft-net`)在两种架构上都编译;arm 专项每夜在 `ubuntu-24.04-arm` 上跑;确定性跨架构一致由标签作业的 aarch64 一致性集验证,不一致即不发布。
- **基准**:托管 runner 上的微基准与路径基准只报告数值(含分配次数),只有编译失败或运行出错才阻断;预热后零分配的合并门槛由各 crate 的 `CountingAlloc` 单元测试与 `StrategyHarness` 承担。按回归阻断只在参考机上进行。
- **CI 不保存累计账本**:每夜冒烟的结果只进每夜摘要;fuzz 语料只放 `actions/cache`。
- **每日回放校验的补跑**(规则唯一归属本条):`hft-replay-verify` 每次运行按日期从早到晚处理本机冷副本中**全部**缺少每日验证报告的 UTC 日(`<date>` 的 seq 段已全部封存、该日报告文件不存在),不只前一日;联调窗口、护栏锁存或停机期间没跑的日子因此在下一次运行时补齐。补跑写出的报告与按时写出的报告同等计入门槛 ⑦(迟到报告视同当日,`generated_utc` 只作记录)。冷副本已按保留期删除而仍无报告的日子不能补跑:该日按 ⑦ 的"每日回放通过"未满足处理(§8.1)。
- CI 的作业依赖用显式的 `needs.<job>.result == 'success'` 判定,不依赖隐式 `success()`([defensive-patterns](defensive-patterns.md#ci-条件显式判定上游结果))。

### 6.1 服务器上的累计运行

**验证包**:CI 在 `ubuntu-24.04-arm` 上构建 `hft-verify-<ver>-aarch64.tar.zst`(+ `.sha256`,作为同一标签的 Release 资产),内含每个 core 解码器的 fuzz 目标二进制(固定 nightly + ASan 的 libFuzzer)、`bin/hft-dst`(`ci` 剖面的 DST 运行器,[hft-testkit](../crates/hft-testkit/README.md#8-hft-dst-运行器))、`bin/hft-verify`(驱动)与初始语料。安装、单元与资源设置归 [deploy](../deploy/README.md)(内存数在 [hft-core 内存表](../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)),`hft-verify` 的命令、轮转表、运行记录与状态目录归 [tools](../tools/README.md);服务器不联网下载语料,也不编译。

**运行记录与账本**:每次 `hft-verify run` 按时间片写不可变的运行记录(主机、验证包版本与 SHA-256、各目标组的 `path_digest`、开始与结束 UTC、每个 fuzz 目标的子进程 CPU 秒数与失败/疑似数、DST 种子区间与种子数、违反数与失败种子;格式归 tools)。**服务器账本**是按完成时刻顺序折叠**全部主机**的运行记录(本机记录 + S3 `hft/verify/` 下各主机的记录,由 journal-archiver 的 `files` source 上传)得到的累计量;各主机本地的账本文件只是缓存。门槛 ③④ 只读服务器账本。规则(本节是唯一归属;阈值在 §8):

1. **只有失败清零**:失败 = `hft-verify` 分类表([tools](../tools/README.md#hft-verify),信号与退出码的归类只在那里)中的"失败"类:某 core fuzz 目标出现 panic 或崩溃(含 ASan 报告与子进程以致命信号结束);任一 DST 违反;`hft-dst` 以致命信号结束(失败种子取它在每个种子开始前写下的"进行中种子"标记,[hft-testkit](../crates/hft-testkit/README.md#8-hft-dst-运行器))。fuzz 失败使该目标累计清零,DST 失败使 DST 累计清零,之后从 0 继续累计。清零的运行记录指明崩溃输入或失败种子(`hft-verify crash list`),由人最小化后经 PR 写进 `fixtures/<venue>/regressions/` 或 `fixtures/dst-seeds/`。
   - **疑似**:服务器是资源受限的主机,其上的单输入超时(§3.3 口径)与 libFuzzer 报告的内存超限(`-rss_limit_mb`/`-malloc_limit_mb`)先记疑似,不清零;CI 用默认限额对最小化后的输入复现(复现作业与结论的记录方式归 tools 与 development)。复现 → 按失败处理,清零时刻取原运行的完成时刻(折叠顺序不变);未复现 → 结案,不清零。该目标有未结案的疑似时,其累计量不能作为门槛 ③ 的通过证据。
2. **漏跑不清零**:定时器未触发、护栏停掉、`ExecCondition` 拒绝启动(护栏锁存 `/var/lib/hft/guard/tripped`、联调窗口标志 `/run/hft/window/active`、`hft-replay-verify` 正在运行)、主机停机 → 没有运行记录,账本原样保留;按 slice 剩余内存算出的 fuzz 限额 ≤ 0 而跳过的运行(运行记录标 `skipped = insufficient_memory`,[tools](../tools/README.md#hft-verify))同样按漏跑处理,不计入运行次数与天数;被中途停止的运行只计已写入运行记录的进度(落盘粒度归 tools);`hft-verify` 或其子进程被外部终止(`SIGKILL`:cgroup OOM、护栏、`TimeoutStopSec`;单元停止时的 `SIGTERM`/`SIGINT` 与 libFuzzer 因此以退出码 72 结束;归类见分类表)按**中止**处理,不算失败、不清零;中止时正在跑的 DST 种子不计入,由本主机下一次运行重跑。
3. **计量**:③ 按 fuzz 子进程的 CPU 时间(用户 + 系统,`wait4` 的资源统计)累计,不按墙钟;④ 按新种子数累计。
4. **运行与天数**:④ 要求的运行次数按"≥ 7 次运行,分布在 ≥ 7 个不同 UTC 日"计(运行的 UTC 日取其结束时刻;只计贡献了新种子的运行)。
5. **种子不重叠**:DST 新种子区间与账本中已有的任何区间(任何主机)重叠的部分只计一次。
6. **跨版本累计**:验证包为每个目标组记录 `path_digest`(构建时对该组相关源路径求哈希,写进 `VERIFY.json`;③ 每个 core 目标 = 对应的 `venue-*`、`hft-venue-proto` 与 `hft-types` 的十进制解析;④ = 引擎 crate(`tools/xtask/deps.toml` 中 `role = engine` 的条目)与 `hft-testkit`)。候选版本 X 的累计量只折叠 `path_digest` 与 X 相同的运行(`hft-verify ledger show --candidate <X>`,输出归 tools);`path_digest` 不同的运行(含其失败)不进入 X 的累计,它们发现的崩溃输入照常经 PR 进回归金样(X 若仍有该缺陷,PR CI 即失败)。
7. **重建**:本机账本或运行记录丢失时,`hft-verify ledger rebuild` 从本机运行记录与 S3 `hft/verify/` 的运行记录按完成时刻重新折叠(失败照规则 1 清零、重叠照规则 5 去重、版本照规则 6 分组),结果标 `rebuilt_from` 并告警;只有任何运行记录都无法取得时才清零。不猜测缺失的运行。
8. **候选版本补跑**:开正式实盘所用的版本 X,其验证包本身必须在服务器上补跑每个 core fuzz 目标 ≥ 15 CPU·min、DST 新种子 ≥ 20,000、零失败、无未结案疑似;这些运行同样计入累计。补跑出现失败 → 按规则 1 清零,且 X 不得用于开正式实盘。
9. CI 的 PR 冒烟、每夜冒烟与任何本地运行都不进服务器账本。

**在哪台服务器跑**:dev2 常态运行(`hftbatch.slice` 内只用空闲 CPU、全部 hft 单元中最先被 OOM 杀与最先被护栏停);升级主机只在 M7 期间、门槛 ⑩ 的 7 天测量窗口之外运行;正式实盘(M8)开始后升级主机不再运行,之后版本的累计在 dev2 上继续。

**达成时间估算**(吞吐待实测):fuzz 6 个目标 × 2 CPU·h = 12 CPU·h;DST 200,000 种子按 `hft-dst` 默认步数每种子 0.1–0.7 s ≈ 6–39 CPU·h。dev2 上批处理只用旧系统剩下的空闲 CPU(可用量待 M3 标定演练实测,[hft-core dev2 护栏](../apps/hft-core/README.md#dev2-护栏));升级主机的批处理核上约 1 天。`hft-verify ledger show` 按最近 7 次运行的吞吐给出预计达成日期,供排期。

## 7. 按主机类的验收

主机类见 [hft-core 线程计划](../apps/hft-core/README.md#线程计划);延迟口径归 [latency-budget §7](latency-budget.md#7-主机类与验收口径)。同一份二进制在各主机类上只换配置。

| 主机类 | 功能与正确性 | 延迟 | 资源 |
|---|---|---|---|
| `dev2`(现有 2 核服务器,与旧框架正式账户同机) | 本页 §2–§5 中需要实机的部分:测试网/模拟盘联调(联调窗口内另起指向测试网端点、持测试网 key 的 `prod` 角色实例)、模拟账户长跑(`sim` 实例)、冷启动/崩溃恢复/回滚、kill 与死手演练(测试网/模拟盘账户)、每日 `replay-verify`;运行 CI 构建的验证包做累计 fuzz 与 DST(`hft-verify`,§6.1),**不编译**;不放正式实盘账户 | **无验收**;直方图只记录 | 按 [hft-core 内存表](../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)(含 `hft-verify` 一行);[dev2 护栏](../apps/hft-core/README.md#dev2-护栏) 演练通过;`HostPressure` 7 天无未解释 P1;同机旧框架正式账户的成交与 PSI 不因 hft 劣化 |
| `c4`(可选) | 同上 | 无验收 | 同内存表 |
| 升级主机(默认 `c32`;`c16` 只在 M3 末满订阅探针达标时保留为候选;`c8` 只能承载 prod、不能与 sim 同机;按 M3 末满订阅探针选定,条件与定义归 hft-core) | 正式实盘:`prod` 实例(只含 Live 账户)与 `sim` 实例(只含 Sim 账户、不持 key)同机;sim 只用非隔离核、独立 slice、独立 journal 与冷盘目录、独立出口 IP 与独立网卡;M7 期间(⑩ 窗口之外)运行 `hft-verify`,M8 起不运行 | 门槛 ⑩(§8.2) | 按 [hft-core 内存表](../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)与[线程计划](../apps/hft-core/README.md#线程计划)的"prod + sim 同机"列;`sim` 永不占 `prod` 的热核 |
| `metal`(可选) | 同上 | bake-off | 同上 |

线程合并不改变确定性:同一 journal 在 `dev2` 与升级主机上验证回放结果相同。

## 8. 上线门槛

交付路径:**dev2**(M6:开发测试、测试网/模拟盘联调、模拟账户长跑;不放正式实盘账户)→ **统一升级主机**(M7:`prod` 与 `sim` 两个实例同机,延迟验收 ⑩)→ **正式实盘**(M8,[runbooks/launch](runbooks/launch.md))。M1.5 延迟探针、M3 末满订阅探针与之后每个里程碑的参考机运行提供早期延迟证据,只记录、定档或作里程碑验收,不是本表的门槛。下表是开正式实盘的前提:frontmatter `scope: core` 的组件全部满足后,按 launch 手册在升级主机上开正式实盘;`optional` 组件不在门槛内。③ ④ 是跨多次运行的**累计量**,只卡正式实盘,不是模拟账户(⑦)运行的前提;计数来源是**服务器账本**(§6.1):只有失败清零,漏跑不清零,丢失时按本机与 S3 的运行记录重建。编号是各章节与 README 引用的稳定 id,不重排。

| # | 门槛 | 通过条件 | 证据(用例) |
|---|---|---|---|
| ① | 单元 / 属性 | 全绿;§4 不变量零违反;策略测试与生产策略集成测试全绿 | 全仓 `TESTS.md` 与 [docs/tests](tests/README.md) 中级别为 `unit`、`table`、`prop`、`integ`、`proc`、`replay` 的全部用例:`cargo xtask test-trace` 在 `current_milestone = M8` 下未实现、多余、不一致三张清单为空,CI 全绿;[INV-1–INV-20](tests/invariants.md)(INV-10 为可选扩展,不计)在全部 DST 与属性运行中零违反 |
| ② | 金样 | Binance、OKX、Bitget 的现货与 USDT 永续:每种订阅消息、每种回报(含自身强平/ADL 识别、账户配置与 UID、保证金数据、可忽略的用户流事件)、每个"结果未知"错误码都有金样且全绿;模拟撮合金样全绿;覆盖清单归 [fixtures](../fixtures/README.md#测试与验收) | 级别为 `golden` 的全部用例(`venue-binance`、`venue-okx`、`venue-bitget` 与 hft-sim 的 `TESTS.md`)CI 全绿 |
| ③ | fuzz | §3.3 所列 6 个 core 解码器目标,每个在服务器账本中(按 §6.1 规则 6 只折叠 `path_digest` 与正式实盘所用版本相同的运行)累计 ≥ 2 CPU·h(子进程 CPU 时间),期间零失败、无未结案疑似;该目标任一失败使它的累计清零;正式实盘所用版本的验证包完成候选版本补跑(§6.1 规则 8) | 级别为 `fuzz`、目标为 §3.3 六个 core 解码器的用例:服务器账本(`hft-verify ledger show --candidate <X>`)的累计与候选版本补跑记录;其余 `fuzz` 用例 CI 全绿 |
| ④ | DST | 服务器账本中(同样只折叠 `path_digest` 相同的运行)新种子累计 ≥ 200,000 个、零违反,来自 ≥ 7 次运行且分布在 ≥ 7 个不同 UTC 日;任一违反使累计清零;正式实盘所用版本完成候选版本补跑(§6.1 规则 8);跨组件场景(§4.1–§4.4 所指的 [scenarios](tests/scenarios.md) §4–§7)全部通过,标"必过"的 QF-1、MS-2、MS-3 通过;失败种子回归集全绿 | 服务器账本的新种子累计与零违反(同上命令);全仓级别为 `dst` 的全部用例 CI 全绿;[scenarios](tests/scenarios.md) §4–§8 的全部用例(MD、`B-n`、A5/B1–B6/C1–C15、BK、QF、LK、LT、FL、MS、AX、FD、AL、FP 各族)通过;必过项 QF-1、MS-2、MS-3;`fixtures/dst-seeds/` 回归集全绿 |
| ⑤ | 恢复与混沌 | §3.6 所指的 RC-1–RC-12 全部用例;`kill -9`、版本变化冷启动、长停机冷启动、毒丸、`--recover-from-snapshot` 回滚、Drain → 重启、dev2 护栏演练;测试网/模拟盘混沌 | RC-1–RC-12;`门槛` 字段含 ⑤ 的其余用例(B-2、B-3、B-8、B-17、B-22 与组件 `TESTS.md` 中的同类用例);[drills](tests/drills.md) 中 `门槛` 字段含 ⑤ 的演练(含测试网/模拟盘混沌)按 `证据` 字段归档的记录 |
| ⑥ | 三所测试网/模拟盘联调 | Binance 测试网(现货与 USDⓈ-M)、OKX 模拟盘、Bitget 模拟盘(各所可用性与 API 覆盖逐所**待核实**,结果写进各 `venue-*` README):在 dev2 上以指向测试网的 `prod` 角色实例(`feed_env = "testnet"`、`order_env = "testnet"`,refdata 用测试网合约与测试网账户;测试网符号与主网的对应关系**待核实**,[12 A21](../design/12-风险与未决问题.md)),在联调窗口内(`hft-window start`,窗口内批处理单元因窗口标志不启动,不打断连续运行)对每个上线策略用到的每个 (venue, 市场) 跑真实连接的下单/撤单/成交/断连/重同步、账户配置与 UID 查询、kill-switch 撤单与 reconciler 对账,连续 3 天无未解释异常(dev2 护栏停掉测试网实例的时段暂停计时、恢复后续计,不重新计时;停机原因写进门槛记录)。某所测试网/模拟盘不提供的接口或行为(含测试网上取不到 UID 的查询,如 Binance USDⓈ-M 测试网能否经现货端点取 UID,待核实,方法见 [venue-binance](../crates/venue-binance/README.md#开发备忘)),在门槛记录里逐项列出并给出替代证据:交易所文档示例金样;或对正式账户的只读查询——正式子账户在 launch D 才开通,以它为证据的项在 A–C 记为"待补证",D 的门槛核对前补齐;wire 级 [mock-exchange](../apps/mock-exchange/README.md) 为冻结组件,解冻后才能作证据;无替代证据的项须 Admin 确认接受 | 级别为 `testnet` 的全部用例按 `证据` 字段归档的记录;门槛记录中逐项列出的替代证据与 Admin 确认 |
| ⑦ | 模拟账户 | 每个上线策略的 Sim 账户用实时行情在 `sim` 实例上连续运行两段(计时、暂停、重新计时与证据绑定见 §8.1):**dev2 段**(dev2 容量下的缩小规模,≥ 14 天,launch C 的前置)与**满规模段**(升级主机 `sim`,按正式参数的规模,在 ⑩ 测量窗口之外连续 ≥ 7 天,launch D 切 Live 的前置);每段:每日账户日报与策略日报按时生成;无 P0、无未解释的 P1;`replay-verify` 每日逐字节通过,`--advice recompute` 每日通过或因当日无检查点跳过(§3.5);hft-sim 的验收项(资金守恒、资金费与真实费率一致、拒单规则与交易所一致)通过;压力用例以最终参数运行(满规模段结束时重跑):**MS-2a(单边 +30%,零强平)与 MS-4(L3 路径)必过**,非门槛变体运行并记录(§8.1) | 每段的每日账户日报、策略日报与每日验证报告(§3.5);MS-2a 与 MS-4(hft-sim 压力用例 `one-sided-30`、`l3-path` 以上线策略的最终参数运行);hft-sim `TESTS.md` 中 `门槛` 字段含 ⑦ 的验收用例;drills 中 `门槛` 字段含 ⑦ 的长跑记录 |
| ⑧ | 解耦门槛 | CI 中新增第二个测试策略(`scaffold-gate`:以 `new-strategy --with-model --with-pipeline` 生成带 Model 与 pipeline 插件的探针,并对它做一次 hft-core 装配检查,步骤归 [tools](../tools/README.md#cargo-xtask-scaffold-gate)、夹具归 [hft-testkit §5](../crates/hft-testkit/README.md#5-解耦门槛夹具)),引擎 crate 与 app 的 diff 为零;`decoupling-gate` 通过;依赖方向与 doc-check 通过 | tools 与 hft-testkit 的 `TESTS.md` 中 `门槛` 字段含 ⑧ 的用例(`scaffold-gate`、`decoupling-gate`、依赖方向、doc-check)CI 全绿 |
| ⑨ | 保留编号,不使用 | — | — |
| ⑩ | 延迟 | 在统一升级主机上、`prod` 与 `sim` 同机运行时连续 7 天测(样本来源、分组、硬线与判定见 §8.2;窗口前的 A/A、窗口的加长与延长按 §8.2 第 7 条):`tick_to_trade` 与 `rep_to_order` 的 p50、p99 不劣于 [latency-budget §3](latency-budget.md#3-cex-路径预算) 的验收下限(硬线);达到 §3 基准形态为目标,落在目标与硬线之间通过,差距与原因写进验收记录并由 Admin 签字;**p99.9 只记录**;同机干扰按 §8.2 第 7 条(本页是该判据的唯一归属);选定主机类的参考机 `bench-gate --reference` 通过;`dev2` 只记录不验收;共享 CI runner 上不判任何时间门槛 | 级别为 `bench` 的全部用例在选定主机类参考机上的 `bench-gate --reference` 输出(基准名登记在 [benchmarks](../benchmarks/README.md#基准名登记));`host` 级与 drills 中 `门槛` 字段含 ⑩ 的用例的验收记录(§8.2 的报告、块表与 Admin 签字) |
| ⑪ | 运维与演练 | ops_bot 工作流逐条验收:老板的 Viewer 角色只读,唯一的写权限是紧急停止(`/stop`:非 HARD、不带 flatten 的 `Kill{Global \| Account}`,确认绑定预览的 `cmd_hash` + TTL 60 s,不能 `Resume`),其余写命令被拒;ctl-gateway 不可达时 `/stop` 经 kill-switch 本地触发 `stop_core`(撤单、写停机标记后停 `hft-core@prod`、不平仓;任一账户 MarginStress ≥ L2、有 Broken 或在途链接时先只撤单,停引擎需第二次确认)并 P0,hft-core 下次启动由停机标记写入 `Kill`、`/resume kill` 才恢复开仓,ops_bot 另记待补 Kill、hft-core 恢复后先自动补发 `Kill` 再接受 `/resume`;kill(全局、按账户、HARD)与交易所死手演练、kill-switch 演练(含本地触发)通过,场合与频率以 [incident-response kill 演练](runbooks/incident-response.md#kill-演练) 为准(正式账户只做 selftest、dry-run 与维护窗口内以 `SmokeOrder` 挂单的零成交撤单冒烟,不 SIGSTOP);**机外看门狗与电话告警演练通过**:停掉升级主机的 watchtower 或阻断 `/healthz` → `hft-offbox-watchdog.service` 在 ≤ 75 s 内经独立 Telegram 与电话通道告警(判定时长加一个探测周期,取值与看门狗所在主机归 [deploy 机外看门狗](../deploy/README.md#机外看门狗);电话通道供应商已选定);`hft-maint start` 的维护静默期间同样操作只发 Telegram、不打电话,静默到期后恢复电话;[incident-response "交易主机失联"](runbooks/incident-response.md#s-交易主机失联)一节的授权表已指定两名人员;升级主机上对正式账户的每把 key(hft-core、kill-switch、reconciler)完成 UID 与账户配置核验 | `门槛` 字段含 ⑪ 的全部用例([drills](tests/drills.md)、[scenarios](tests/scenarios.md) 的 MD-11–MD-13 与各组件 `TESTS.md`):`host`/`testnet`/`drill` 级按 `证据` 字段归档的记录,其余级别 CI 全绿;级别为 `drill` 而 `门槛` 不含 ⑪ 的用例(如切 Live 之后的演练)不计入 ⑪ |

**正式实盘前置**(不是门槛编号;步骤、记录与签字归 [launch D](runbooks/launch.md)):D-smoke 零成交冒烟、D-shadow 正式实例 Shadow 3–7 天(与同参数 sim 逐决策比对),以及上线签字记录中初始 `RiskParams` 上限与第一次放宽条件的必填数值。

门槛之外:正式实盘的风险上限由运营在 `RiskParams` 设定并写进上线签字记录(步骤见 [launch](runbooks/launch.md));上线策略的永续杠杆由运营在交易所设定,引擎核验交易中永续 inst 的实际杠杆不超过 `RiskParams.max_perp_leverage_x100`(超出 → `ACCOUNT_CFG` 不符、禁开,[hft-risk](../crates/hft-risk/README.md))。

### 8.1 模拟账户长跑的计时与证据

门槛 ⑦ 的计时规则(launch 与 incident-response 链接这里):

1. **证据绑定**:⑦ 的证据是一段运行,每天绑定 `(发布版本, StrategyParams SHA-256, RiskParams SHA-256)`(SHA 取 journal 中生效 `ConfigBlob` 的内容哈希,与 journal-archiver `config/` 对象同一口径);每日日报记录当日生效的三元组。门槛按策略计:每个上线策略各有自己的 ⑦ 证据。
2. **两段**:dev2 段在 dev2 的 `sim` 实例上按其容量运行(容量归 [hft-core](../apps/hft-core/README.md#线程计划)),通过后才进入 launch C;满规模段在升级主机的 `sim` 实例上按正式参数的 group 规模运行,与 ⑩ 测量窗口不重叠(⑩ 窗口内该主机的 `sim` 按 §8.2 启停,不计入 ⑦),结束时以最终三元组重跑压力用例(第 6 条),通过后才在 launch D 切 Live。
3. **暂停**:`sim` 实例的停机(护栏停机及其自动重启、采集窗口、发版切换、故障)按墙钟累计;累计 ≤ 24 h 视为暂停(计时冻结,恢复后继续),累计超过 24 h → 重新计时。
4. **重新计时**:新版本相对上一版本改动了该策略包(含其 pipeline 插件)、`hft-strat-lib`、`hft-strategy-sdk`、`hft-exec`、`hft-risk` 或 `hft-oms` 的代码(按两个版本 SHA 之间这些路径的 `git diff` 判定);或 `StrategyParams` 的改动超出 schema 字段属性 `FieldDesc.minor_change` 允许的范围(由 SDK `params::minor_change_check` 按 schema 通用判定,不按策略写规则;`MinorChange` 各变体的语义归 [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md));或 `RiskParams` 有任何改动;或任一天没有通过的每日回放报告(§6 补跑之后仍缺)。重新计时后 ⑦ 在任一可用的 `sim` 实例上从 0 开始;发生在 launch C 之后时,升级主机 `sim` 上满规模连续 ≥ 14 天同时满足两段。
   - **group 增删与 `open_allowed` 变化**:`minor_change_check` 只比较两版都有的 group,这两类改动由本条判定——增删 group(按 `GroupId`;同一 `GroupId` 的 `GroupSpec` 指纹变化视同删除后新增)与 `open_allowed` 0↔1 都**不重新计时**;但新增的 group 自身在 `sim` 实例上连续运行满 7 天之前不计入 ⑦ 证据(从它首次出现在生效参数中起算,暂停与重新计时按本节规则)。
5. **决策不变的修复**:diff 只涉及 `venue-*`、`hft-venue`、`hft-net`、冷进程(`tier: cold` 的 app 与 crate)与 `deploy/`,且新版本的 `replayer verify` 对最近 7 天的 journal 逐字节一致 → 不重新计时,新版本记入证据;发生在 D-shadow 期间时 D-shadow 的重计归 [launch D](runbooks/launch.md)。不属于第 4 条、也不属于本条的版本变化同样不重新计时,三元组的变化记入证据。第 4、5 条未列入的模块(hft-sim、hft-sequencer、hft-ledger、hft-types、hft-book)与 `[sim]` 配置的改动如何计时:策略研究后定(R8-12)。
6. **压力用例用最终参数**:MS-2a、MS-4 与非门槛变体以 ⑦ 结束时生效的三元组运行(满规模段结束时在升级主机上重跑);之后三元组再变 → 重跑。交易 inst 的分档 MMR 必须在 refdata 中(缺失 → 用例拒绝运行)。非门槛变体运行并记录结果,出现强平须在 ⑦ 记录中解释,不卡门槛;用例清单、变换与判定归 [hft-sim 压力用例](../crates/hft-sim/README.md#压力用例上线门槛必过项)。
7. **每日回放与 recompute**:每日回放(`hft-replay-verify`)按 §6 补跑;recompute 从该日归档的 Model 检查点起算(§3.5);当日无检查点 → 跳过当日 recompute 并 P2,不使 ⑦ 重新计时。

### 8.2 延迟验收的测法

门槛 ⑩ 的口径(分段定义、预算数字、校准插值与误差界公式归 [latency-budget](latency-budget.md),本节只定义测什么、怎么判):

1. **时间与配置**:M7,统一升级主机,`prod` 与 `sim` 同机;`prod` 接主网实时行情、下单回路指向测试网/模拟盘(`order_env = "testnet"`),只含测试网账户;**只有一个**连续的主网行情窗口(7 天;按第 7 条加长或延长时随之变长,至多 14 天;第 7 条的 A/A 在窗口之前);窗口内该主机不运行 `hft-verify`,`sim` 按第 7 条启停。
2. **样本来源**:
   - **订单类分段**(`tick_to_trade`、`rep_to_order`、`hedge_host`、`decide`、`egress`、`order_rtt`、`leg_gap`)取 `prod` 的 `LatencySample`:`prod` 运行 [latency-probe](../strategies/latency-probe/README.md)(只能绑定非正式账户),`quote` 模式产生行情触发的 `tick_to_trade` 样本与回报触发的 `rep_to_order` 样本(Ack 后同一事件内撤单,主网行情负载下);同时运行的上线策略的样本同样计入。`rep_to_order` 含新单、撤单与改单三种 `action` 的样本,`order_rtt` 只计 `action = New`(字段归 hft-event,样本合成归 hft-oms)。样本量、周期与探针变体(含跨 gw 的变体)归 latency-probe。
   - **行情类分段**(`kernel_rx`、`ingest`、`feed_host`,以及诊断分段 `nic_to_kernel`、`kernel_to_user`)取**按事件加权**的离线统计:`replayer latency`(规格归 [replayer](../apps/replayer/README.md))以 hft-telemetry 的 `event_weighted()` 对窗口内 seq journal 的每条行情记录计算;探针样本不用于这些分段;`kernel_rx` 另并列上下界(第 9 条)。
   - **`pair` 模式只作小样本确认**(它要求 `feed_env = "testnet"`):在 M6 dev2 联调窗口或 M7 任意 ≥ 1 天的测试网行情时段运行,`hedge_host`、`leg_gap` 结果单独成数据集,只记录。
3. **时钟误差过滤**:含 PHC 端点的分段按 latency-budget §2.1 用样本前后两条 `ClockStatus` 校准对线性插值换算,逐样本给误差界;误差界超过 §2.1 阈值的样本计入 `unsynced_cal`,不进分位数;HW 时间戳不可用时以 `sw_rx`(REALTIME)为锚(该组按 SW 数据集)。⑩ 报告逐组附误差界分布(p50/p99/max)与 `unsynced_cal` 占比(按第 9 条的突发、平静两层分别给出),占比需在验收记录中解释。
4. **数据集与样本量**:按 [latency-budget §5](latency-budget.md#5-分组与不同数据集不混算) 分组与分数据集(HW 与 SW 时间戳不合并),样本数不足的组按 §5 的规则不报相应分位数。**主判定数据集**:默认用 HW 数据集;以下任一情况该组改用 SW 锚数据集判定,硬线与目标的数值不变,报告注明"不含网卡到软中断一段":HW RX 时间戳不可用(HW RX 时间戳与 PTP 的可用性由 M1.5 与 M3 末探针核实,替代测法见第 10 条);该组 `unsynced_cal` 占比 > `unsynced_cal_max`(待标定,方法:M1.5 探针上按层的占比分布);突发层的 `unsynced_cal` 占比超过平静层的 2 倍(占比随负载上升时 HW 数据集在繁忙时段被成批丢样本;倍数待标定,方法同上)。
5. **判定**(每个 (venue, 触发 kind) 组的 p50 与 p99):
   - **硬线** = latency-budget §3 的验收下限,适用 `tick_to_trade` 与 `rep_to_order`;任一组越过硬线 → ⑩ 不通过。
   - **目标** = latency-budget §3 的基准形态:`tick_to_trade` 与 `rep_to_order` 达到目标为通过;落在目标与硬线之间同样通过,差距与原因写进验收记录并由 Admin 签字。
   - 同机干扰按第 7 条判定。
6. **只记录**:p99.9(固定节拍突发与周期性停顿下);`hedge_host`(`rep_to_order` 中 `link_role = Second` 的子集,对照其基准形态)与其余主机内分段(`kernel_rx`、`ingest`、`feed_host`、`decide`、`egress`、`leg_gap`、`order_rtt`、`nic_to_kernel`、`kernel_to_user`)对照 §3 报告,超出须在验收记录中解释,不单独判定。
7. **同机干扰**(全仓只此一个判据,其他文档链接这里):
   - **A/A**:7 天窗口之前,在同一主机上连续 1–2 天让 `sim` 两半都关(其余配置与窗口相同),以 `watchtower interference --aa` 按下面的分块与配平规则给每块的两小时贴"开/关"标签,得到各组 R 的区间与半宽。A/A 的区间应包含 1;不包含 → 先查明块内顺序或时段带来的系统偏差,再开窗口。A/A 半宽外推到 7 天(半宽 × √(A/A 天数 / 7))> `max_halfwidth` → 窗口初始长度相应加长,使外推半宽 ≤ `max_halfwidth`(至多 14 天)。
   - **开关**:从窗口起点(一个 UTC 偶数整点)起每 2 小时一块;每块一小时开 `hft-core@sim`、一小时关(启动后的 Warmup 期计入"开");某一小时内 `sim` 状态有变化(`Mixed`)的块整块不参与。**按资金费块配平**:含 00:00、08:00、16:00 UTC 任一整点的块是资金费块,其余是普通块;排程按段生成(初始窗口为一段,每次延长为一段),每段内两类块各自先开与先关的块数相差 ≤ 1,哪些块先开由记录在案的种子随机排列决定(做法见 [host-upgrade](runbooks/host-upgrade.md#步骤) 第 8 步:`hft-sim-alternate --config /etc/hft/measure.toml`,文件的键归 [deploy](../deploy/README.md#实现说明))。
   - **统计**:按第 4 条的分组,把全部"开"小时与全部"关"小时的样本各自合并,比值 R = p99(开) / p99(关);以块为单位有放回重抽样 2,000 次(种子写进报告)得到 R 的 bootstrap 分布,95% 置信区间取该分布的 2.5 与 97.5 分位,半宽 = (上界 − 下界) / 2。
   - **判定**:`prod` 的 `tick_to_trade` 与 `rep_to_order` 各组在开、关两侧各有 n ≥ 10,000 时作硬判定:上界 ≤ 1.05 通过;上界 > 1.05 且半宽 ≤ `max_halfwidth`(0.03)不通过;上界 > 1.05 且半宽 > `max_halfwidth` → **区间过宽,不判失败**:窗口按 2 天一段延长(按同一配平规则排程)后重算,窗口总长至多 14 天;到 14 天仍过宽 → 该组只记录,验收记录写明并由 Admin 签字。任一侧 n < 10,000 的组(含全部 `hedge_host` 组)只记录 R 与区间。
   - 计算由 `watchtower interference` 给出(纯函数 `interference` 在 [hft-telemetry](../crates/hft-telemetry/README.md));报告附块表(每块的类别与开关顺序、两类块的配平计数)、A/A 结果、种子与每组的 (n 开, n 关, R, 区间, 半宽)。
8. **参考机**:选定主机类的参考机 `bench-gate --reference` 通过([benchmarks](../benchmarks/README.md#测试与验收))。
9. **突发/平静分层与 `kernel_rx` 上下界**(M1.5 与 M3 末探针按此抽样;⑩ 报告按此并列):
   - **层**:每条行情记录按 hft-telemetry 的 `stratum()` 分层——同一连接上与它共享 `rx_tsc` 的行情记录数 ≥ 2,或其 `MarketHdr.rx_empty_gap_ns` > `burst_gap_ns` → 突发;否则平静。`burst_gap_ns` 待标定(方法:M1.5 探针上 `rx_empty_gap_ns` 与同次读出帧数的联合分布;初值取 latency-budget §3 的 `kernel_rx` p99 预算)。行情触发的探针样本取其触发记录的层。
   - **抽样与合成**:两层的探针样本各自满足 latency-budget §5 的样本量下限(不足的层延长运行或提高该层的触发配额;配额的实现归 latency-probe)。报告每层的 p50/p99,并给加权合成值:权重 = 同一窗口 `event_weighted()` 中两层行情记录的占比,合成的 p 分位 = 使 Σ(权重 × 该层样本的经验分布函数)≥ p 的最小值;某层样本不足时不出合成值。⑩ 报告对 `tick_to_trade` 同样并列两层与合成值(只记录,判定仍按第 5 条)。
   - **`kernel_rx` 上下界**:下界 = 按记录的 `hw_rx` 算出的 `kernel_rx`(一次读出多个包时 `hw_rx` 的取法归 [hft-net §7](../crates/hft-net/README.md#7-时间戳));上界 = `rx_empty_gap_ns` + 同一记录的 `nic_to_kernel`(本连接上次确认接收队列为空以来的等待,加网卡到软中断一段);SW 数据集没有 `nic_to_kernel`,只给 `rx_empty_gap_ns` 并注明;`rx_empty_gap_ns = 0`(无确认)的记录不出上界,计数报告。两界都报 p50/p99,不单独判定。
   - 报告列出窗口内 ENA `*_allowance_exceeded` 计数有增长的小时(watchtower 采集;整形发生在 `hw_rx` 之前,`kernel_rx` 看不到)。
10. **HW 戳不可用时的替代测法**(M1.5 与 M3 末探针;租用与步骤归 [11 实施路线图](../design/11-实施路线图.md)):① 同 CPG 放一台回显对端,对 TCP ping-pong 的 RTT 做 A/B(IRQ 核空闲 / 保持唤醒 × 有 / 无压舱流量);② 只在探针上用 bpftrace 记录 IRQ 核上 `irq_handler_entry` → `napi_poll` → `netif_receive_skb` 的间隔(软中断排队时长)。HW 戳不可用时,ENI-B2 的启用判据(归 [hft-core 线程计划](../apps/hft-core/README.md#线程计划))以"二者之一在深度/标记价突发窗口内显著上升"代替 `nic_to_kernel`,阈值与原判据一样写在该探针的 Agent Note。
