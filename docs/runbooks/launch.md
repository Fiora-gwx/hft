---
description: "从联调到正式实盘:A dev2 开发测试与三所测试网/模拟盘联调 → B dev2 上模拟账户连续 ≥ 14 天(缩小规模的门槛 ⑦)→ C 统一升级主机(prod + sim 同机,延迟验收,之后 sim 满规模复核 ⑦)→ D 正式实盘(正式子账户开通与核验、杠杆与现货禁借币、预热、签字的初始限额、测试网到正式的切换、SmokeOrder 冒烟单 D-smoke、正式实例 Shadow 3–7 天 D-shadow、切 Live、与模拟并行校准);没有小额实盘阶段;回退 = Drain"
scope: core
---

# 上线(联调 → 模拟 → 统一升级 → 正式实盘)

## 适用范围

- 把一个 ccex 策略实例上线到正式实盘(首个是 [portfolio-carry](../../strategies/portfolio-carry/README.md))。一个账户只绑定一个策略实例([one-account-per-strategy-instance](../../.agents/notes/proposed/architecture/2026-10-02-one-account-per-strategy-instance.md));正式实盘用新开的子账户,不接管任何旧仓位。接管现有系统的账户与仓位走 [migration](migration.md)(optional)。
- 首批交易所:Binance、OKX、Bitget(现货 + USDT 本位永续)。
- **每个交易所只用一个市场**:一个策略实例的账户在同一交易所只开**一个**子账户、只用**一个**市场(现货或 USDT 永续)——refdata 加载以 `SameExchangeMarkets` 拒绝,`SetParams` 以 `InvalidArgs{SameExchangeLegs}` 拒绝两腿同交易所的 group(规则归 [hft-refdata](../../crates/hft-refdata/README.md),理由见 [no-same-exchange-spot-perp](../../.agents/notes/proposed/architecture/2026-10-03-no-same-exchange-spot-perp.md))。组合携带的典型开法:交易所 X 的永续子账户 + 交易所 Y(≠ X)的现货子账户。测试网账户与模拟账户同样遵守。
- **四个阶段,顺序不可跳,没有小额实盘、没有按档放量的阶段**([no-small-live](../../.agents/notes/proposed/architecture/2026-10-03-no-small-live.md)):A 联调与 B 模拟账户在 `dev2`(M6),C 统一升级主机(M7),D 正式实盘(M8;里程碑见 [11 §3](../../design/11-实施路线图.md))。D 在切 Live 之前有两步不开仓、不算小额实盘的验证:零成交冒烟 D-smoke 与正式实例 Shadow 3–7 天 D-shadow([shadow-before-live](../../.agents/notes/proposed/process/2026-10-03-shadow-before-live.md))。**`dev2` 从不持有正式实盘账户**:dev2 上只有 `sim` 实例与联调窗口里只持测试网 key 的 `prod` 角色实例,部署侧由 IAM 与预检保证([deploy 秘密与实例角色](../../deploy/README.md#秘密与实例角色))。
- 门槛的编号与内容唯一定义在 [testing §8](../testing.md#8-上线门槛);③④ 的累计量来自服务器上运行的验证包(dev2 常态运行,M7 期间升级主机在 ⑩ 测量窗口之外也运行;账本规则见 [testing §6.1](../testing.md#61-服务器上的累计运行),部署见 [deploy 验证包](../../deploy/README.md#验证包与-hft-verify))。本页只写每阶段开始前要求哪些:

| 阶段 | 在哪 | 开始前必须通过 | 本阶段产出的证据 |
|---|---|---|---|
| A 联调 | dev2:常驻 `sim` 实例 + 联调窗口里的测试网 `prod` 角色实例 | 无(开发构建来自 CI 全绿的提交) | ⑥;⑪ 中的 kill、死手、kill-switch 与 `/stop` 兜底演练(测试网账户) |
| B 模拟账户 | dev2:`sim` 实例 | ①、②、⑤、⑧,以及 A 的验收 | dev2 缩小规模的 ⑦(规模与计时规则见 testing §8,含必过的压力用例);⑪ 中的 ops_bot 工作流(含老板 `/stop` 演练) |
| C 统一升级主机 | 升级主机:prod(测试网账户)+ sim 同机 | dev2 缩小规模的 ⑦ 通过;负责人的升级决定与选定的主机档位 | ⑩(实时行情 + 测试网/模拟盘下单回路,prod 与 sim 同机运行时测);⑩ 窗口之后 sim 满规模复核 ⑦ ≥ 7 天并以最终参数重跑压力用例;在新主机上重做 ⑪ 的演练;机外看门狗、维护静默与电话告警演练 |
| D 正式实盘 | 升级主机:prod(正式 Live 账户)+ sim | ①–⑧、⑩、⑪ 全部(③④ 的累计量只在这里要求,不卡 A–C;⑦ 含 C 阶段的满规模复核);D 内的 D-smoke 与 D-shadow 先于切 Live 完成 | 上线签字记录 |

- **正式实盘的风险上限由运营在 `RiskParams` 设定**(D 第 28 步)。初始上限与"第一次放宽"的条件是上线签字记录里的**必填数值**(不是门槛,但没有数值不能签字);建议从保守值起步。

## 前置条件

1. 当前版本已按 [release-and-deploy](release-and-deploy.md) 安装并激活。开发构建只在 dev2 上运行;正式 Live 账户只由正式版 `vX.Y.Z` 驱动,开发构建遇到指向正式端点的 Live 账户拒绝启动([deploy 制品](../../deploy/README.md#制品从哪里来))。
2. 该实例的参数:pipeline 实例配置的两腿 venue 位于不同交易所;policy 与**本实例的**人工覆盖项(`overrides/<实例>.toml`,[pipeline 实例配置](../../python/pipeline/README.md#实例配置))已就绪;`RiskParams` 由 `hft-cfg` 生成并评审。
3. kill-switch 的目标表与 reconciler 的账户表随 refdata/部署包含该阶段用到的账户(A、C:测试网账户;D:正式账户)。
4. D 阶段:授权人(老板或负责人)已在上线签字记录中签字,记录的必填项:资金规模;初始 `RiskParams` 各上限的数值(账户总敞口、单笔名义、日亏额度、`max_perp_leverage_x100` 等,字段归 [hft-risk](../../crates/hft-risk/README.md));"第一次放宽"的条件(观察天数、成交笔数、P0/P1 计数、reconciler `Ignore` 连续天数、MarginStress 最高档位等,每项写数值)与放宽后的目标数值。核对:DRL-30。
5. 两个 hft-core 实例的账户名不相交(规则归 [hft-core 实例角色](../../apps/hft-core/README.md#实例角色prod-与-sim):账户名即 `AccountMeta.label`,sim 实例的必须以 `sim-` 开头,prod 实例的不得以它开头)。

## 步骤

### A. 联调(dev2,测试网/模拟盘)

1. **测试网/模拟盘账户** `[需授权]`(改外部系统):Binance Demo Trading(现货与 USDⓈ-M;端点与账户形态以 venue-binance 为准,待核实)、OKX 模拟盘、Bitget 模拟盘;各所的可用性、API 覆盖与端点以 venue README 为准(逐所待核实,方法:在 dev2 上用开发构建逐所调用账户与下单接口,结果记进 venue README;[Binance](../../crates/venue-binance/README.md)、[OKX](../../crates/venue-okx/README.md)、[Bitget](../../crates/venue-bitget/README.md))。按正式账户的开法建账户(每所一个市场),记下每个账户的 UID。用测试网 key 读不到 UID 的账户(能否读到逐所待核实,方法:联调第一天用该 key 调 UID 查询接口,结论写进 venue README)在 hft-cfg 输入中写 `uid_policy = "unverifiable"` 与 Admin 签字 `uid_policy_signoff`(签字写进上线记录):只允许 `is_prod = false` 的账户;此时 `IDENTITY` 由 kill 自检通过加一次成功签名的只读请求放行,并有一条 P1 Note 标明未核验(字段归 [hft-refdata](../../crates/hft-refdata/README.md#数据结构),放行规则归 [hft-risk](../../crates/hft-risk/README.md))。两个 `is_prod = false` 账户在同一交易所的**不同 `VenueId`** 上登记同一 UID(Binance 演示环境的现货与 USDⓈ-M 共用一个 UID)只产生 P1 警告(hft-core 启动时写 `Note{RefdataWarning}`),不拒绝加载;同一 `VenueId` 上重复仍是加载错误(规则归 hft-refdata)。核对:DRL-1。
2. **测试网 key**:与正式 key 同样的权限划分(下单会话 key 与 REST key——按所不同,以 [hft-core](../../apps/hft-core/README.md#实例角色prod-与-sim) 为准——只读 key、撤单 key),IP 白名单 = dev2 的 prod 出口地址(C 阶段再加升级主机的 prod 出口 EIP);存 Secrets Manager 的 `hft/testnet/<account>/<venue>/…`。核对:DRL-2、DRL-3。
3. **账户配置**:按 D 第 21 步对测试网账户做同样的设置(持仓模式、保证金模式、杠杆、现货禁借币),作为 D 的预演;不支持的项在上线记录中注明。要跑延迟探针的测试网账户,其 `RiskParams` 另须满足 [latency-probe](../../strategies/latency-probe/README.md) 的风控参数要求(价格带 ≥ `far_bp`、`ioc_max_cross` ≥ `pair_cross`、单笔上限 ≥ 最小名义)。核对:DRL-4;杠杆超上限与借币打开的拒绝:DRL-5、DRL-6。
4. **IAM、出口地址与配置** `[需授权]`:
   - dev2 的 IAM(改生产主机 IAM;实例角色的秘密与 `AssumeRole` 授权、各消费者角色的 S3 与 KMS 授权一次完成,权限集合唯一归属 [deploy 秘密与实例角色](../../deploy/README.md#秘密与实例角色));完成后 `preflight.sh` 的凭证文件、S3 `HEAD` 与 KMS 写入探测都通过([deploy IMDS 锁定](../../deploy/README.md#imds-锁定与凭证代理));
   - dev2 网卡上为 sim 分配第二个私有 IP + EIP(sim 不占旧系统主地址的交易所限额,[deploy 出口 IP](../../deploy/README.md#出口-ip));
   - Admin 用 `hft-cfg` 生成 dev2 `prod` 实例的 refdata——**只含测试网账户**(`AccountMeta{label, kind = Live, is_prod = false, bound_instance}`,`label` 不以 `sim-` 开头;每个 venue 一行 `AccountVenues`,`venue_uid` = 测试网 UID,读不到 UID 的账户按第 1 步设 `uid_policy`;字段归 [hft-refdata](../../crates/hft-refdata/README.md))与测试网的 inst 与端点(测试网的符号映射是否与正式不同待核实,方法:对比两套端点的合约列表接口,假设 A21);`/etc/hft/hft-core-prod.toml` 设 `instance.feed_env = "testnet"` 与 `instance.order_env = "testnet"`(行情与下单都走测试网,避免主网价格对测试网下单造成的错配;字段归 [hft-core](../../apps/hft-core/README.md#配置文件字段));`preflight.sh` 通过(dev2 的 `formal_live = false` 检查)。dev2 上任何正式账户都会被 hft-core 以 `ProdAccountOnDev2` 拒绝。核对:DRL-7。
5. `[需授权]` **开联调窗口**:`sudo /opt/hft/current-core/deploy/dev2/hft-window start`(写窗口标志 `/run/hft/window/active`、停止批处理定时器与服务,再启动 reconciler 与 `hft-core@prod`,[deploy dev2 护栏](../../deploy/README.md#dev2-护栏))。第一分钟验收同 [release-and-deploy](release-and-deploy.md) 步骤 16:每个测试网账户每个 venue 的启动闸门位全 1,重点 `IDENTITY`(每把 key 查到的 UID = `venue_uid`,每把分别核)与 `ACCOUNT_CFG`。核对:DRL-8。
6. **联调清单**(通过条件归 testing §8 ⑥):下单/撤单/成交、断连与重同步、账户配置与 UID 查询、用户流解码、`Unknown` 由证据收敛、两所测试网之间的两腿链接与对冲、自动平仓。每项的 journal 证据链接写进上线记录。用例:DRL-9–DRL-11、DRL-13;自动平仓:DRL-12(在 B 第 14 步的模拟账户上用 hft-sim 的受控故障注入制造;A–C 期间自然出现的 `Broken` 同样按 [incident-response A](incident-response.md#a-裸敞口nakedexposure断腿broken) 处置并记录)。
7. **演练**(⑪ 的一部分,在测试网账户上):按 [incident-response kill 演练](incident-response.md#kill-演练) 做 kill(全局、按账户、HARD)、交易所死手、`kill-switch fire`、老板 `/stop` 与 `stop_core` 兜底(ctl 不可达 → kill-switch 撤单并停止 `hft-core@prod` → 恢复后待补 Kill 自动补发 → 交易员 `/resume`)。用例:DRL-88–DRL-98。
8. **关联调窗口**:`/shutdown`(Drain)→ `sudo …/hft-window stop`;期望 journal 末尾是 Drain 快照、测试网挂单为 0、窗口标志已删除、批处理单元与定时器恢复(DRL-14)。`hft-window stop` 报"prod 仍有热段"(测试网实例在窗口内崩溃)→ 先 `sudo …/hft-window start` 让测试网实例恢复,再重跑 `hft-window stop`([deploy dev2 护栏](../../deploy/README.md#dev2-护栏);CORE-212)。

### B. 模拟账户(dev2)

9. **模拟账户**:在 sim 实例的 refdata 中使用该策略的预置模拟实例池(dev2 上每个研究中的策略 `sim_pool = 2` 个,[hft-sim](../../crates/hft-sim/README.md#预置模拟实例池))里空闲的一个——换参数用 `SetParams`、清空用 `/simreset`,都不重启。池中没有空闲实例时才在 sim 实例的 refdata 中新增 `AccountMeta{label = "sim-…", kind = Sim(SimAccountCfg), bound_instance = <模拟实例>}`(refdata 变更):`initial_balances` 按计划资金在各 venue 的分配;`fee_tier` = 计划的正式子账户费率档;`latency_model` 取实测的下单往返与回执分布;`margin_model` 按各所规则。模拟账户的 venue 组合与将来的正式账户一一对应(同样每个交易所一个市场)。
10. **参数**:模拟实例有自己的覆盖项(`overrides/<模拟实例>.toml`);在模拟实例上的试验只写它自己,不会流到任何实盘实例([pipeline 参数通道](../../python/pipeline/README.md#参数通道))。按 [Model 预热](#model-预热) 完成预热;`/mode <模拟实例> <模拟账户> live`(Trader + 确认)。期望:`/sim <模拟账户>` 开始出现成交与日报,所有消息带"模拟"标签。核对(第 9、10 步):DRL-15。
11. **运行到满足 dev2 缩小规模的 ⑦**:证据绑定(发布版本, `StrategyParams` SHA, `RiskParams` SHA);连续 ≥ 14 天,每天看 `/sim` 日报(PnL 分解、成交率、两腿间隔、裸敞口、保证金占用、各所 `liq_dist` 与 MarginStress 档位、策略 Note 汇总),P0 为零、P1 都有解释、`replay verify` 每日通过(`hft-replay-verify` 的结果文件,`/status` 可见;联调窗口期间的回放由之后的运行补跑,迟到报告的算法归 testing §8)。规模(dev2 的 sim 容量)、停机、改代码或改参数是暂停还是重新计时,按 testing §8 的规则(参数小改由 schema 字段属性 `FieldDesc.minor_change` 判定)。护栏停掉 sim 后由守护按规则自动重启(每 UTC 日至多一次,[incident-response P](incident-response.md#p-dev2-护栏触发))。研究用采集窗口要先停两个 hft-core 实例(hft-core 的 dev2 整机表),排期上先采集、后长跑。核对(第 11、12 步):DRL-19。
12. **压力用例**:按 [hft-sim 压力用例](../../crates/hft-sim/README.md) 以最终参数运行:门槛项"零强平"与必过的 L3 路径变体(依次出现 L1/L2/L3 且无强平),其余变体只记录;交易 inst 的分档 MMR 必须在 refdata 中,缺失时用例拒绝运行(用例清单归 testing §8 与 hft-sim)。参数在 ⑦ 期间的改动超出 schema 字段属性 `FieldDesc.minor_change` 允许的范围时,压力用例重跑。
13. **演练**(在模拟账户上):`/simtransfer` 演练一次跨所划转;把永续所的保证金划走以拉近强平距离,观察 MarginStress 三档(按 `liq_dist`,[hft-risk](../../crates/hft-risk/README.md))的行为与 `/treasury` 建议;演练后 `/simreset` 恢复初始余额(前置条件归 [control-plane `SimReset`](../subsystems/control-plane.md))。用例:DRL-20、DRL-21;L3 期间现货报价不可用的串行兜底:DRL-22。
14. **Broken 自动平仓**(演练):在 dev2 `sim` 实例的一个模拟账户上,Admin 经 `cli:<login>` 发控制面命令 [`SimFault`](../subsystems/control-plane.md#命令集)`{account = <模拟账户>, venue = <第二腿所在 venue>, fault = RejectNew}`(受控故障注入,sim-gw 侧语义归 [hft-sim](../../crates/hft-sim/README.md);只对 Sim 账户生效,prod 实例拒绝)让该 venue 的 sim-gw 拒单,等第一腿成交、第二腿重试耗尽进入 `Broken` 后发同一命令 `fault = Off` 撤销注入,按 [incident-response A](incident-response.md#a-裸敞口nakedexposure断腿broken) 处置并记录(DRL-12:group 进入 `Broken`、账户 `HedgeFailure` cap 生效,无人干预 `broken_auto_fix_ms` 后出现一次 `Note{ExecAutoFixDone}` 或 `Note{ExecAutoFixFailed}`)。A–C 期间自然出现的 `Broken` 同样按 incident-response A 处置并记录。
15. **演练退出命令**(命令名与语义归 [portfolio-carry](../../strategies/portfolio-carry/README.md)):对一个有仓位的 group 发 `/cmd <模拟实例> <模拟账户> <group> exit`(`reduce_only`,不需确认),期望该 group 目标设为 0、`reason = Exit`、不受成本门约束;再用 `/params <模拟实例> set force_exit <group> true` 做一次;最后对实例发 `/cmd <模拟实例> <模拟账户> none exit_all`,期望全部 group 退出。用例:DRL-23。
16. **ops_bot 工作流**(⑪):老板以 Viewer 走完只读面板并在模拟账户上做一次 `/stop`,交易员恢复;记录进 [ops_bot 验收](../../python/ops_bot/README.md#测试与验收)。用例:DRL-24。

### C. 统一升级主机(M7)

17. 按 [host-upgrade](host-upgrade.md) 执行:按 M3 末满订阅探针的结论选定主机档位(默认 `c32`;`c16` 只在探针达标且启用 `epoll_pwait(0)` 时)与是否启用 ENI-B2、挂 ENI-B 与 sim 用的 ENI-C 并分配出口地址、在新主机上启动 sim(模拟账户从 `initial_balances` 重新开始;dev2 的 sim 停用)与只含测试网账户的 prod;测试网 key 的白名单加入新主机的 prod 出口 EIP。用例:DRL-49–DRL-61。
18. **延迟验收 ⑩**:prod 与 sim 同机运行,实时行情 + 测试网/模拟盘下单回路,按 [latency-budget](../latency-budget.md) 与 testing §8 的口径采集一个主网行情窗口(7 天;窗口之前 1–2 天的 A/A、按 A/A 加长与区间过宽时的延长按 testing §8.2 第 7 条,窗口至多 14 天);判定(硬线、目标与签字、证据来源)归 [testing §8.2](../testing.md#82-延迟验收的测法),不另取测试网行情窗口(latency-probe 的 `pair` 模式只作小样本确认,只记录)。同机干扰按 testing ⑩ 的方法(2 小时一块、块内先开后关的顺序按记录的种子随机、两类块各自配平)测,步骤在 [host-upgrade](host-upgrade.md) 第 7、8 步。A/A 与测量窗口内升级主机不运行 `hft-verify`。用例:DRL-56、DRL-57;判定 CORE-407。
    **⑦ 满规模复核**:⑩ 窗口结束后,升级主机的 sim 实例以满规模(研究策略的全部模拟实例与 group)连续运行 ≥ 7 天,并以最终参数重跑压力用例(规则归 testing §8 ⑦);可与 D 的第 21–31 步并行,D 第 33 步之前完成(DRL-59)。
19. 在新主机上用测试网账户重做 A 第 7 步的演练(部署形态变了,gw 的 kill 字、kill-switch 的出口、polkit 授权与触发套接字都要在新主机上验证;DRL-25),并做 [host-upgrade](host-upgrade.md) 第 11 步的换 AZ 恢复演练(DRL-60)。
20. **机外看门狗**(⑪ 前置):dev2 上 `hft-offbox-watchdog` 指向新主机的 `/healthz`(目标写在 dev2 的主机本地文件 `/etc/hft/offbox-watchdog.toml`,[deploy 机外看门狗](../../deploy/README.md#机外看门狗));演练:屏蔽安全组或停 watchtower → 达到看门狗的判定时长后的一个探测周期内 Telegram 与电话告警发出(时长归 deploy)、恢复通知到达(DRL-26);再在新主机上 `sudo hft-maint start 10` 后停 watchtower → 只收到 Telegram、没有电话,截止后恢复电话告警(DRL-27);[az-probe](az-probe.md) 结论为升级主机与 dev2 同 AZ 时,第三 AZ 的 `t4g.nano` 看门狗(`[需授权]`)已启用并同样演练(DRL-28);[incident-response S](incident-response.md#s-交易主机失联) 的授权表已由负责人填好两人的名字(DRL-29)。

### D. 正式实盘(M8)

21. **开正式子账户** `[需授权]`(改外部系统):在实例用到的每个交易所,用主账户新建**一个**子账户(命名 `<实例>-<交易所>-<spot|perp>`),只开通它承担的那个市场(永续子账户开通 USDT 本位合约;现货子账户不开合约、不开杠杆)。记下每个子账户的 UID。随后完成账户配置(逐项截图或导出存入上线记录):
    - 按该 venue `VenueCaps` 的 `required_position_mode`/`required_margin_mode` 及 venue README 的其他要求(单向持仓、全仓、多资产模式、手续费抵扣)设置;要求值的唯一归属是各 venue README。
    - **永续杠杆**:在交易所把交易宇宙内每个永续 inst(该所按账户设置时对账户)的杠杆设为 L——L 按策略 README 的要求取值(portfolio-carry 的取值与理由归其 README)。引擎侧硬约束:L ≤ `RiskParams.max_perp_leverage_x100` / 100(默认 3 倍),超过 → 该账户 `ACCOUNT_CFG` 不符(禁开)+ P1([hft-risk](../../crates/hft-risk/README.md))。启动后 `/risk <a>` 显示的每 inst 实际杠杆必须等于 L。
    - **现货子账户关闭借币**:OKX `enableSpotBorrow = false`;Bitget 现货杠杆/借币关闭;Binance 现货子账户不开杠杆账户;各所的设置位置与查询字段以 venue README 为准(待核实,方法:在测试网/模拟盘账户上按 A 第 3 步设置后读账户配置接口,与网页显示对照)。`ACCOUNT_CFG` 核验它(不符时 `AccountConfigReport.mismatches` 含 `AUTO_BORROW`,闸门不放行)。
    核对:DRL-32。
22. **API key**(每个子账户):
    - 交易 key(下单会话 key 与 REST key;Binance 为两把、OKX 与 Bitget 为同一把,以 hft-core 为准):只开交易权限,**不开提现与划转**;IP 白名单 = 升级主机 `egress.toml` 中 `prod` 列表的 EIP,**绝不加入 sim 的出口 EIP 或 dev2 的地址**;
    - 只读 key(reconciler)、撤单 key(kill-switch;该所支持只撤单权限时用之,否则单独一把交易 key 只给 kill-switch,权限粒度以 venue README 为准):同一白名单;
    - 全部存入 Secrets Manager 的 `hft/live/<account>/<venue>/…`([hft-secrets](../../tools/README.md#hft-secrets));key 不出现在聊天、仓库、journal、ops-log 中(ops-log 只记 key 的 ID 与创建时间)。每把 key 的 UID 在启动时由使用它的进程分别核对(hft-core 每把交易 key、kill-switch 自检、reconciler 启动),不等于 `venue_uid` → 该进程退出 2 并 P0。
    - 核对:DRL-33。
23. **从测试网切到正式配置** `[需授权]`(M7 → M8)。开始前把第 7 步的正式 refdata、`hft-core-prod.toml`、kill-switch 与 reconciler 的正式配置和新的 `/etc/hft/pipeline.toml` 准备好并评审(只读,不安装)。第 3 步停掉 watchtower 后机外看门狗读不到维护静默的延长([deploy 机外看门狗](../../deploy/README.md#机外看门狗)),所以第 3–8 步要在一次静默(≤ 30 min)内完成:第 3 步之前再执行一次 `sudo hft-maint start 30` 并等看门狗读到(≤ 15 s);来不及时在第 8 步启动 watchtower 后立即再续一次,其间的电话告警属预期,事先告知值班人。各小步的检查点:DRL-34。
    1. 在升级主机上 `sudo hft-maint start 30`(机外看门狗只发 Telegram);prod 以 `/shutdown`(Drain)停止;期望 journal 末尾是 Drain 快照。
    2. 以 `hft-archive` 身份运行 `journal-archiver check --prefix hft-testnet`(写法见 [deploy 运维命令的身份](../../deploy/README.md#imds-锁定与凭证代理);root 的实例角色没有 S3 权限),确认测试网期间的 prod journal 每个小时都已提交到 `hft-testnet/<host>/prod/` 前缀(M7 期间升级主机上 prod 的 archiver source 前缀为 `hft-testnet`,规则归 [journal-archiver](../../apps/journal-archiver/README.md)),**且该前缀下没有 quarantine 条目**;有未提交小时 → 等 archiver 追平;有 quarantine → 按 journal-archiver 的处理步骤清零后再继续,不搬目录。
    3. 停止读 prod journal 的冷进程、kill-switch 与验证包:`systemctl stop hft-watchtower hft-journal-archiver hft-ops-bot hft-pipeline hft-treasury hft-reconciler hft-kill-switch hft-verify.timer hft-verify.service`(停定时器不会停正在运行的服务,两个都要停;kill-switch 只在启动时读目标表与 key,不重启它就会继续以测试网账户为撤单目标)。`/var/lib/hft/ops_bot/pending_kill.json` 中不得有 `pending` 条目(有则先按 [incident-response R](incident-response.md#r-老板紧急停止stop之后) 处理完),之后把该文件移到 `pending_kill.testnet-<date>.json`。
    4. 冷副本 `<cold_root>/prod/` 整体移到 `<cold_root>/prod-testnet-<date>/`(只读保留)。读者游标不需要处理:archiver 在第 7 步改前缀后从新前缀的空游标起步,watchtower 识别 journal 根被替换后从新 journal 起步(规则归 [journal-archiver](../../apps/journal-archiver/README.md#使用本组件) 与 [watchtower](../../apps/watchtower/README.md))。
    5. 清掉 hugetlbfs 上 prod 实例子目录中的测试网热段文件(否则正式配置首次启动会与旧热段冲突,以 journal 恢复失败退出 4)。
    6. **归档测试网的冷进程状态**:`/var/lib/hft/kill-switch/stop-marker.json*`(含 `.consumed`、`.aborted`)移到 `/var/lib/hft/kill-switch/testnet-<date>/`——残留的测试网标记会让正式实例首次启动就写入 Kill 并 P0;pipeline 中 prod 实例的 `versions/<实例>/`、`overrides/<实例>.toml` 与 `<实例>/` 状态移到 `/var/lib/hft/pipeline/testnet-<date>/`——否则实例名不变时测试网期间的 group 与覆盖项会原样带进正式实例;核对 treasury 与 ops_bot 配置中的实例表与账户表(账户名随第 7 步的 refdata 换成正式账户)。
    7. archiver 的 prod source 前缀改为正式前缀(`hft`,对象键 `hft/<host>/prod/…`);换上新的 `/etc/hft/pipeline.toml`(prod 的 `s3_prefix = "hft"`,策略实例绑定正式账户);`/etc/hft/host-local.toml` 的 `manifest` 改为 `tokyo-cex-1-live`(`verify = false`,[deploy 主机清单](../../deploy/README.md#主机清单))并以当前正式版重新激活 `core` 组——同一发布里两份清单,不另发补丁版,`hft-verify.*` 随之删除([deploy 验证包](../../deploy/README.md#验证包与-hft-verify))。Admin 用 `hft-cfg` 生成**只含正式账户**的 refdata(`AccountMeta{label, kind = Live, is_prod = true, bound_instance}`,`label` 不以 `sim-` 开头;每个 venue 一行 `AccountVenues`,`venue_uid` = 正式子账户 UID;每个交易所只出现一次,`SameExchangeMarkets` 在生成时报出)与正式 `/etc/hft/hft-core-prod.toml`(`instance.feed_env = "prod"`、`instance.order_env = "prod"`,只允许在升级主机类上);kill-switch 与 reconciler 的配置同步换成正式账户;`preflight.sh` 通过(含 IMDS 锁定与 `hft-verify.*` 的 `LoadState = not-found`)。
    8. 按 [release-and-deploy](release-and-deploy.md) 步骤 15 的顺序启动冷进程(kill-switch 以正式配置先于 hft-core 启动),再以正式配置首次启动 prod(`config_source = file`,首次启动规则归 [hft-core](../../apps/hft-core/README.md#启动顺序));全部策略实例 `RunMode = Shadow`;`sudo hft-maint stop`。
24. **身份与配置核验**:`/status` 中每个正式账户在每个 venue 的启动闸门位全部为 1(`IDENTITY`、`ACCOUNT_CFG`、`RECONCILED`、kill 位自检);`kill-switch selftest` 通过,且 `kill-switch status` 的目标表只含正式账户(没有任何测试网账户)。任一位为 0 或目标表不符 → 停止,按位修正(UID 不符多半是 key 放错了账户,立即在交易所禁用该 key 并重新核对)。核对:DRL-35。
25. **入金** `[需授权]`:由人在主账户向各子账户划入初始资金(交易 key 无划转权限);永续子账户的资金划到合约钱包。金额与分配参考 `/treasury <a>`;每笔用 ops_bot `/transfer` 登记(`RecordTransfer`)。
26. **余额基线**:非策略资产(手续费抵扣币、尘埃)用 `/baseline <a> <venue>` 登记;之后等 reconciler 连续两轮该账户全部 `Ignore`。资金费差异只告警、由账本自动校正,不参与这一判定([incident-response B3](incident-response.md#b3-资金费差异))。 核对(第 25、26 步):DRL-36。
27. **预热**:正式实例按 [Model 预热](#model-预热) 预热(它与模拟实例的 Model 各自独立)。 用例:DRL-16。
28. **限额**:按上线签字记录中的初始数值设定 `RiskParams`,经运维 CLI `SetRisk` 下发(确认);`/risk <a>` 核对生效值与签字记录逐项相等。单笔上限过小、两腿 lot 的最小公倍数名义超过 `max_chunk_notional` 时,`SetParams` 以 `InvalidArgs{LotMismatch}` 拒绝(规则归 [hft-exec](../../crates/hft-exec/README.md)),按提示调大限额或换 inst(改动写回签字记录)。 核对:DRL-37。
29. **校准对**:在 pipeline 配置中把正式实例与同参数的模拟实例(sim 实例上)声明为校准对;两者共享的覆盖项只能显式以 `scope = pair` 写入([pipeline 参数通道](../../python/pipeline/README.md#参数通道)),默认每个实例只改自己。 每日核对见 DRL-47。
30. **D-smoke 零成交冒烟** `[需授权]`(维护窗口内;实例仍为 `Shadow`,不产生仓位;冒烟单的命令语义归 [control-plane `SmokeOrder`](../subsystems/control-plane.md)):对每个(正式账户, venue):
    1. **冒烟单**:Admin 在 ops_bot 私聊 `/smoke <a> <venue> <inst> buy`(或运维 CLI 的 `SmokeOrder`),预览核对账户标签、价格(离对手价 ≥ `smoke_far_bp`;本引擎不检查交易所的价格保护带,被交易所拒单时如实回报,规则归 [hft-oms 冒烟单](../../crates/hft-oms/README.md#冒烟单smokeorder))与数量(最小名义 × 1.1)后确认;期望 `CmdResult` ok、journal 中同一事件有该单的 `OrderDecision`、`/orders <a>` 显示该单(OMS 槽带 `SMOKE` 标志)、`ttl_ms`(缺省 10 s)到期后被 OMS 撤销;记下订单号。该单在 OMS 里,reconciler 不把它当外来挂单,不出现 `Recon` cap。核对:DRL-38。
    2. **交易所死手**(该 venue 有死手时):再发一张 `ttl_ms = 60000` 的冒烟单;`/status` 显示 gw 对该 inst 的死手刷新在进行(冒烟单使死手覆盖该 inst);运维 CLI 发不撤挂单的 `Drain{cancel_resting = false}`(hft-core 干净退出,kill-switch 不动作)→ 挂单在该所死手时限内被交易所撤销,核对交易所订单历史中的撤销时刻与原因;没有死手的 venue 记"不适用"。`Drain{cancel_resting = false}` 时 SMOKE 槽保留到进程退出、不按 `ttl_ms` 撤销(规则归 [hft-oms 冒烟单](../../crates/hft-oms/README.md#冒烟单smokeorder));OMS 仍先撤了该单 → 属缺陷,修复后重做本小步。重启 prod,闸门全 1。核对:DRL-39。
    3. **kill-switch 正式端点撤单**:再发一张 `ttl_ms = 60000` 的冒烟单 → `kill-switch fire --account <a> --venue <v> --reason "D-smoke" --yes` → 挂单被撤且 kill-switch 核验为 0;OMS 经用户流看到撤单,槽终结。核对:DRL-40。
    4. reconciler 连续两轮该 (account, venue) 全部 `Ignore`。
    5. 每把 key 的 UID 与 `ACCOUNT_CFG` 核验结果(第 24 步)与本步的订单号、时刻一起写进上线记录。第 4、5 小步的核对:DRL-41。
31. **D-shadow 正式实例 Shadow 3–7 天**:正式 `prod` 实例全部策略实例保持 `RunMode = Shadow`(决策照算、订单不发),打通真实保证金(`MarginReport`)、资金费流水、用户流、对账与告警;同参数的模拟实例(第 29 步的校准对)同时运行。每天:`replayer compare --a <prod 当日 journal> --b <sim 当日 journal> --map <正式账户>=<模拟账户>[,…] --out <dir>`(规格归 [replayer](../../apps/replayer/README.md))逐决策比对,每条差异写出原因(行情接入差异、账户状态差异、缺陷);reconciler 全部 `Ignore`、资金费流水与账本一致(`FundingCorrected` 有解释)、无 P0、P1 都有解释。不满 3 天不进入下一步;出现未解释的差异 → 修复后重新计 3 天(修复属"决策不变的修复"时同样只重计 D-shadow 的 3 天、不使 ⑦ 重新计时,判定归 [testing §8](../testing.md#8-上线门槛))。 用例:DRL-42;未解释差异的重计:DRL-43。
32. **门槛核对**:上线记录中逐项列出 ①–⑧、⑩、⑪ 的证据链接、D-smoke 与 D-shadow 的记录、签字记录;缺任何一项不进行下一步(DRL-44;签字记录的必填数值 DRL-30)。
    - Admin 名单:负责人核对 GitHub 侧的 `.github/hft-admins.txt`(`quota_override` 与签字字段的 Admin 校验以它为准,[development 免费额度](../development.md#免费额度))与 ctl 签发者表 `/etc/hft/ctl-principals.toml`(hft-core 配置,只在启动时加载,改动需重启;由 deploy 生成;[control-plane 角色](../subsystems/control-plane.md#角色))中的 Admin 是同一组人,不一致当天修正(改签发者表后重启 hft-core 再核一次)(DRL-31)。
    - 各 runbook 页首次用于正式账户之前的走查记录齐全(DRL-112)。
    - ③④:负责人在 dev2 上以 `hft-archive` 身份把 S3 `hft/verify/` 同步到本机目录(写法见 [deploy 运维命令的身份](../../deploy/README.md#imds-锁定与凭证代理)),再运行 `hft-verify ledger show --candidate <开正式实盘所用版本> --runs-from <该目录>`(命令归 [tools](../../tools/README.md#hft-verify)),只累计 `path_digest` 与该版本相同的运行,另含该版本验证包的补跑(规则归 [testing §6.1](../testing.md#61-服务器上的累计运行));输出存入上线记录。
    - ⑦:dev2 缩小规模的 ⑦ 与 C 阶段 sim 满规模复核(第 18 步)两份证据都在。
33. `[需授权]` `/mode <实例> <正式账户> live`(Trader;PREVIEW 预览首行核对目标账户标签为 `[💰实盘·…]` 与当前有效模式,再确认)。 核对:DRL-45。
34. **盯前几笔**:每笔成交后 `/p <a>` 与交易所网页/REST 核对两腿数量与余额(以净量计,注意按基础币扣手续费的所)、`/targets <a>` 无 `Stuck`/`Broken`、`/risk <a>` 无 `ExternalFill` 且 `liq_dist` 合理、journal 无 `Note{ExecAutoFixDone|ExecAutoFixFailed}`;任何不一致按 [incident-response](incident-response.md) 处理。 核对(切 Live 后首个 UTC 日的每一笔):DRL-46。
35. **每日**:reconciler 全部 `Ignore`(资金费差异只告警,不在此列);`/sim <模拟> vs <实盘>` 校准报告(滑点、成交率、两腿间隔、延迟差异)——模拟与正式实盘并行校准;差异稳定后把建议的 `SimAccountCfg` 调整经 refdata 变更回写 sim 实例(维护窗口内批量进行)。 核对:DRL-47。
36. **调整限额**:"第一次放宽"在签字记录中的条件全部满足后,按签字的目标数值经 `SetRisk` 放宽(升级类,确认);条件未满足时的放宽需要授权人重新签字。之后的调整同样记录日期、新值与授权人;放大策略资金参数时先作用于模拟实例确认无异常。 核对:DRL-48。

### Model 预热

策略的慢速部分(`Model`)需要历史样本(例如 168 h 的资金费与基差统计)才产出有效的 Advice;没有检查点时从空起步。预热把历史样本作为 Input 注入([warmup-injection](../../.agents/notes/proposed/architecture/2026-10-03-warmup-injection.md)),不用等几天。B 阶段的模拟实例与 D 阶段的正式实例各自预热。

- **何时需要**:实例第一次启动;或 journal 中出现该实例的 `Note{ModelCheckpointDiscarded}`、启动时没有可用检查点(检查点规则见 [hft-core](../../apps/hft-core/README.md));按 group:出现 `Note{ModelGroupReset}`(结构参数热更)或参数新增了 group——这两种由 pipeline 自动处理([pipeline 预热](../../python/pipeline/README.md#model-预热))。非结构参数变化、refdata 变化、发版**不会**使检查点失效。
- **注入**:ops_bot `/warmup <实例>`(Trader),或在主机上以 `hft-pipeline` 身份运行 `hft-pipeline warmup --config /etc/hft/pipeline.toml --instance <实例>`(它以 pipeline 主体发命令,写法见 [deploy 运维命令的身份](../../deploy/README.md#imds-锁定与凭证代理))。策略的 pipeline 插件从交易所公共 REST 的 K 线/资金费历史或 collector 数据生成样本,并产出自成一体的块列表(每块 ≤ 4096 B);框架一块一条地发 `InjectWarmup{instance, local_kind, chunk_no, last, payload}`。期望:journal 中该实例的 `InjectWarmup` 从 `chunk_no = 0` 连续到 `last = true`,每条 `CmdResult` 为 ok;`/pipeline <实例>` 显示本次预热完成;策略报告中的样本数字段达到该策略的要求(字段名见策略 README)。核对:DRL-16。
- **不符**:某块被拒 → 看 `CmdResult` 原因,修正后整批重发(重发语义归策略 README);数据源不可用 → 换另一来源(REST ↔ collector)再跑;都不行时让实例在 `Shadow` 下自然积累样本,并在上线记录中写明推迟。用例:DRL-17(中途中断后整批重发)、DRL-18(换数据源)。

## 验收

- A:每个测试网账户每个 venue 的闸门位全为 1;⑥ 的联调清单逐项有 journal 证据;kill、死手、kill-switch、`/stop` 与 `stop_core` 兜底演练有记录;联调窗口结束时测试网挂单为 0、批处理已恢复。
- B:⑦ 满足(计时规则与压力用例按 testing §8),日报完整,零越权、零 P0;退出命令与老板 `/stop` 演练有记录。
- C:⑩ 按 [testing §8.2](../testing.md#82-延迟验收的测法) 判定通过(落在目标与硬线之间时有 Admin 签字);同机干扰在 testing ⑩ 的判据内;sim 满规模复核 ⑦ 通过;新主机上演练与换 AZ 恢复演练通过;机外看门狗、维护静默与电话告警演练通过;B 第 14 步的 Broken 注入演练(DRL-12)通过,A–C 期间自然出现过的 `Broken` 已按 incident-response A 处置并记录。
- D:签字记录的必填数值齐全;门槛核对表完整;每个交易所只有一个子账户、一个市场;杠杆与禁借币截图存档且 `/risk` 显示的实际杠杆与 L 一致;key 权限与白名单截图存档;D-smoke 每个(账户, venue)有冒烟单的订单号与结论;D-shadow ≥ 3 天、差异全部解释;对账连续两轮 `Ignore`;切 Live 后首日每笔成交与交易所核对一致。

## 回退

- **实例级**(只停这个实例):`/mode <实例> <a> closeonly`(降级,立即,不需确认)= 禁止开新仓,持仓的处理按策略规则(portfolio-carry 在 CloseOnly 下持有、不强制退出,规则归其 README);需要退出时用策略命令——单个 group `exit`(`reduce_only`)或 `/params … set force_exit <group> true`,全部 group 用实例级 `exit_all`;需要立即平仓时 `/kill account <a> flatten`(Trader + 确认;对 `Broken` 的 group 同样生效,先减永续腿;该账户同时置清仓锁,直到 `/resume kill <a>` 只接受更接近 0 的目标,规则归 [hft-exec](../../crates/hft-exec/README.md))。仓位归零后可 `/mode … shadow`。
- **老板紧急停止**:老板(Viewer)发现异常可 `/stop <a>` 或 `/stop`(停开仓、撤挂单,对冲与平仓继续,带确认);ctl-gateway 不可达时 ops_bot 改经 kill-switch 撤单并停止 hft-core(`stop_core`)并 P0;之后由值班交易员按 [incident-response R](incident-response.md#r-老板紧急停止stop之后) 接手,恢复只能由交易员做。
- **系统级**(版本或主机问题):`/shutdown`(`Drain`,降级,不需确认;等待条件归 [hft-sequencer](../../crates/hft-sequencer/README.md),含未完成的 MarginStress/Hard 引擎减仓,满足后写快照退出);回滚版本按 [release-and-deploy 回退](release-and-deploy.md#回退)。
- **紧急**:按 [incident-response](incident-response.md) 的"通用首五分钟";疑似 key 泄露按 K 节,先在交易所禁用 key,再 `/kill account <a> hard`;交易主机失联按 S 节。
- A–C 不涉及正式资金,回退 = 从该阶段开头重做;D-smoke 与 D-shadow 不开仓,失败时修复后重做该步;切 Live 后回退能否直接重新切 Live,由负责人在上线记录中决定(需要时回到 C 的演练或重做 D-shadow)。

## 记录

`ops-log/<date>-launch-<实例>.md`:各阶段起止时间与门槛证据链接;上线签字记录(资金规模、初始 `RiskParams` 数值、第一次放宽的条件与目标数值、授权人);测试网账户与正式子账户各自的交易所、市场、UID、开通时间、账户配置截图(含杠杆 L 与禁借币)、key ID 与权限截图(不含 key)、IP 白名单;refdata 与 `RiskParams` 的版本;预热的数据源、样本区间与 `InjectWarmup` 块数;入金与 `RecordTransfer` 对应关系;模拟期每日日报链接与压力用例结果;延迟验收直方图链接;D-smoke 每张冒烟单的订单号、死手撤销时刻、kill-switch 核验结果;M7 → M8 切换时搬走的冷副本目录与 `pending_kill`、停机标记、pipeline 状态的归档位置,以及切换后的清单名;D-shadow 每日 `replayer compare` 报告与差异解释;正式实盘首日成交核对表(两腿、净量、手续费资产、两腿间隔、与交易所核对结果);每次调整限额的日期、新值与授权人。
