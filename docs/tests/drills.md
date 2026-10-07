# 演练与人工步骤的测试规格(drills)

格式与规则见 [docs/tests](README.md)。本页是 [runbooks](../runbooks/README.md) 中演练、上线检查点与人工步骤的测试规格:每个用例给出在哪里做、谁授权、按哪一步做、什么算通过、记录写到哪里。步骤的唯一归属是 runbook,本页只链接并写出这一次执行要核对的具体结果;规则的唯一归属是各组件 README 与 [testing](../testing.md),本页不重抄。

## 1. 约定

- **编号** `DRL-n`,全仓唯一,删除的编号不复用。runbook 中每个演练或检查点步骤旁标出对应编号。
- **分工**:组件 README 自己定义的演练(例如 [hft-core dev2 护栏](../../apps/hft-core/README.md#dev2-护栏) 的护栏演练、[deploy](../../deploy/README.md) 测试表中的脚本与单元核对)归该组件的 `TESTS.md`,本页不重复,只在第 2 节的覆盖表与 runbook 中引用其编号;跨组件的确定性场景归 [scenarios](scenarios.md),不变量归 [invariants](invariants.md)。本页只收 runbook 定义的、需要真实主机、交易所或人的用例。
- **级别**:`testnet` = 在交易所测试网/模拟盘账户上(dev2 联调窗口或 M7 升级主机上只含测试网账户的 `prod`);`host` = 在 dev2、升级主机、探针或参考机上,不需要人做判断;`drill` = 有人参与判断或操作(电话是否打到、预览是否显示、签字)。
- **授权**:`[需授权]` 的含义与授权人归 [runbooks 写法规则](../runbooks/README.md#写法规则) 与 [值班与授权](../runbooks/README.md#值班与授权);授权人与时刻写进证据。
- **前置中的参数**:用例依赖的参数在 `前置` 中显式写出取值;执行时以 `hft-cfg` 生成的该次配置为准,证据里记下生效配置的 SHA-256。
- **证据**:每次执行写 `ops-log/<UTC 日期>-DRL-<n>.md`(附件放同名目录 `ops-log/<UTC 日期>-DRL-<n>/`),内容为每一步的开始/结束 UTC 时刻、执行人、授权人、命令输出或截图、相关 journal 的 `seq` 区间,以及逐条通过标准的结论;runbook 自己的记录文件(如 `ops-log/<date>-launch-<实例>.md`)链接它。
- **实现**:能由脚本采集与断言的写 `tests/host/DRL-<n>.sh`(只读:读 journal、`systemctl`、`journalctl`、`/status` 与交易所只读接口,把结论写进证据目录;不改变任何状态);必须由人操作或判断的写对应的 runbook 步骤链接。
- **门槛**:`门槛` 字段列出该用例的执行记录作为哪一项[上线门槛](../testing.md#8-上线门槛)的证据;切 Live 之后才执行的用例不带 `门槛`。
- **失败即停**:任一通过标准不满足 → 按 runbook 该步写的"不符时"处理,修复后从该用例的第 1 步重做;不在同一次执行里跳过失败项。

## 2. 覆盖

| runbook 小节 | 用例 |
|---|---|
| [README 写法规则、值班与授权](../runbooks/README.md#值班与授权) | DRL-29、DRL-31、DRL-112 |
| [README 测试与验收](../runbooks/README.md#测试与验收) | DRL-112;kill 演练的频率 DRL-100 |
| [launch 适用范围、前置条件](../runbooks/launch.md#前置条件) | DRL-1、DRL-7、DRL-30、DRL-32、DRL-34 |
| [launch A 联调](../runbooks/launch.md#a-联调dev2测试网模拟盘) | DRL-1–DRL-14;第 7 步的演练 DRL-88–DRL-98 |
| [launch B 模拟账户](../runbooks/launch.md#b-模拟账户dev2) | DRL-15、DRL-19–DRL-24;第 14 步 DRL-12 |
| [launch C 统一升级主机](../runbooks/launch.md#c-统一升级主机m7) | DRL-25–DRL-29、DRL-56、DRL-57、DRL-59、DRL-60;第 17 步 DRL-49–DRL-61 |
| [launch D 正式实盘](../runbooks/launch.md#d-正式实盘m8) | DRL-30–DRL-48;第 27 步 DRL-16 |
| [launch Model 预热](../runbooks/launch.md#model-预热) | DRL-16–DRL-18 |
| [launch 验收、回退、记录](../runbooks/launch.md#验收) | 各阶段用例的通过标准与证据;回退用到的动作 DRL-85、DRL-88、DRL-91、DRL-92 |
| [host-upgrade 前置条件](../runbooks/host-upgrade.md#前置条件) | DRL-49–DRL-52 |
| [host-upgrade 步骤、验收、回退](../runbooks/host-upgrade.md#步骤) | DRL-53–DRL-61;第 9、10 步 DRL-25–DRL-28;⑩ 的判定 CORE-407([hft-core TESTS](../../apps/hft-core/TESTS.md)) |
| [host-tuning](../runbooks/host-tuning.md#步骤) | DRL-62–DRL-68;隔离核噪声 TOOL-112([tools TESTS](../../tools/TESTS.md)) |
| [az-probe](../runbooks/az-probe.md#步骤) | DRL-69–DRL-71;工具报告 TOOL-098、TOOL-099 |
| [release-and-deploy 步骤、验收、回退](../runbooks/release-and-deploy.md#步骤) | DRL-72–DRL-79、DRL-83–DRL-86;一次性传参的移除 CORE-288 |
| [release-and-deploy refdata 变更](../runbooks/release-and-deploy.md#refdata-变更) | DRL-80–DRL-82;Drain 末复核不通过与新 refdata 启动失败由 CORE-324、CORE-225 验证 |
| [incident-response 分级、通用首五分钟](../runbooks/incident-response.md#通用首五分钟) | DRL-87 |
| [incident-response A](../runbooks/incident-response.md#a-裸敞口nakedexposure断腿broken) | DRL-12 |
| [incident-response B、B2](../runbooks/incident-response.md#b2-人工在交易所网页操作之后adoptexchange) | DRL-106 |
| [incident-response B3](../runbooks/incident-response.md#b3-资金费差异) | 无演练:资金费自动校正与告警由 hft-ledger、watchtower 的用例验证 |
| [incident-response C、D](../runbooks/incident-response.md#d-hft-core-崩溃重启风暴拒绝启动) | DRL-102、DRL-104、DRL-86;systemd 重启行为 CORE-015–CORE-017 |
| [incident-response E](../runbooks/incident-response.md#e-sequencer-停摆sequencerstall) | DRL-103、DRL-95 |
| [incident-response F](../runbooks/incident-response.md#f-行情静默断连限速风暴延迟超预算) | DRL-10 |
| [incident-response G](../runbooks/incident-response.md#g-控制面不可用需要撤单但命令发不进去) | DRL-101 |
| [incident-response H](../runbooks/incident-response.md#h-存储退化persist-滞后磁盘热段内存) | DRL-107 |
| [incident-response I、J](../runbooks/incident-response.md#j-隔离核闯入irq-漂移只适用于-c4c8c16c32metal) | 无演练:`ClockOffset`、`IsolatedCoreIntrusion`、`IrqDrift` 告警是 watchtower 的可选扩展;IRQ 落点与隔离核由 DRL-63 与 hostprobe 核对 |
| [incident-response K](../runbooks/incident-response.md#k-秘密泄露或可疑访问) | DRL-108 |
| [incident-response L](../runbooks/incident-response.md#l-按账户-hard) | DRL-91 |
| [incident-response M](../runbooks/incident-response.md#m-保证金压力marginstress) | DRL-21、DRL-22 |
| [incident-response N](../runbooks/incident-response.md#n-外部成交强平adl人工交割externalfill) | DRL-105、DRL-106 |
| [incident-response O](../runbooks/incident-response.md#o-交易所公告下架资金费结算周期变化维护) | DRL-109 |
| [incident-response P](../runbooks/incident-response.md#p-dev2-护栏触发) | CORE-210–CORE-217([hft-core TESTS](../../apps/hft-core/TESTS.md)) |
| [incident-response Q](../runbooks/incident-response.md#q-链上optional) | 里程碑外(可选扩展) |
| [incident-response R](../runbooks/incident-response.md#r-老板紧急停止stop之后) | DRL-92–DRL-97 |
| [incident-response S](../runbooks/incident-response.md#s-交易主机失联) | DRL-26–DRL-29、DRL-60 |
| [incident-response kill 演练](../runbooks/incident-response.md#kill-演练) | DRL-88–DRL-100 |
| [development 应急交叉编译、免费额度](../development.md#应急交叉编译release-x86-crossyml) | DRL-110(DRL-111 已删除) |
| [migration](../runbooks/migration.md)、[bsc-node](../runbooks/bsc-node.md) | 里程碑外(可选扩展;启用时补写) |

## 3. 上线(launch)

### DRL-1 测试网账户每所一个市场、UID 已登记,读不到 UID 的账户带 Admin 签字

- 规则:[launch A 第 1、4 步](../runbooks/launch.md#a-联调dev2测试网模拟盘)、[hft-refdata 数据结构](../../crates/hft-refdata/README.md#数据结构)、[hft-cfg](../../tools/README.md#hft-cfg)
- 级别:testnet
- 里程碑:M6
- 门槛:⑥
- 环境:dev2;联调窗口内的 `prod` 角色实例(`instance.feed_env = "testnet"`、`instance.order_env = "testnet"`),refdata 只含测试网账户
- 授权:`[需授权]`(开测试网账户、开联调窗口)
- 前置:每个测试网账户按正式账户的开法建好(每所一个市场);读不到 UID 的账户在 hft-cfg 输入里写 `uid_policy = "unverifiable"` 与 `uid_policy_signoff = "<Admin 的 GitHub 登录名>"`
- 步骤:按 [launch A 第 1、4、5 步](../runbooks/launch.md#a-联调dev2测试网模拟盘)
  1. Admin 运行 `hft-cfg` 生成 refdata 与差异报告
  2. 开联调窗口,等第一分钟验收结束
- 通过标准:
  - 差异报告中每个测试网账户每个 venue 恰好一行 `AccountVenues`,同一账户内每个交易所只出现一个 `VenueId`,`hft-cfg` 退出 0
  - `venue_uid` 为空串的账户都带 `uid_policy = "unverifiable"`,其 `uid_policy_signoff` 的登录名在 `.github/hft-admins.txt` 中;`venue_uid` 非空的账户 `uid_policy` 为 `"verified"`
  - 每个账户每个 venue 的最后一条 `Note{StartupGate}` 的新 gate_bits 含 `IDENTITY`;每个 `unverifiable` 的 (账户, venue) 恰好一条 `Note{IdentityUnverified}`(P1),`verified` 的没有
  - 两个 `is_prod = false` 账户在同一交易所的不同 `VenueId` 上登记同一 `venue_uid`(Binance 演示环境现货与 USDⓈ-M 共用 UID)时 hft-core 完成启动,journal 有一条 `Note{RefdataWarning}`(P1),告警群收到内容含 `DuplicateVenueUid` 的 P1;没有这种账户时记"不适用"
  - 同一 `VenueId` 上两个账户登记同一 `venue_uid` 时 `hft-cfg` 退出 1(`DuplicateVenueUid`),不产出 blob
  - 每所"能否用测试网 key 读到 UID"的结论已写进对应 venue README(证据记 PR 链接)
- 证据:`ops-log/<UTC 日期>-DRL-1.md`(差异报告、`StartupGate` 与 `IdentityUnverified` 的 journal 摘录、告警截图、PR 链接)
- 实现:`tests/host/DRL-1.sh`

### DRL-2 测试网 key 按正式 key 的权限划分建立,白名单只含 dev2 的 prod 出口地址

- 规则:[launch A 第 2 步](../runbooks/launch.md#a-联调dev2测试网模拟盘)、[deploy 秘密与实例角色](../../deploy/README.md#秘密与实例角色)
- 级别:testnet
- 里程碑:M6
- 门槛:⑥
- 环境:各所测试网/模拟盘的账户后台;dev2 的 Secrets Manager 前缀 `hft/testnet/`
- 授权:`[需授权]`(在交易所建 key、写 Secrets Manager)
- 步骤:按 [launch A 第 2 步](../runbooks/launch.md#a-联调dev2测试网模拟盘);对每把 key 截取后台的权限页与白名单页
- 通过标准:
  - 每个 (账户, venue) 的 key 集合与 [hft-core 实例角色](../../apps/hft-core/README.md#实例角色prod-与-sim) 列出的该所集合相同(下单会话 key、REST key、只读 key、撤单 key)
  - 交易 key 与撤单 key 的权限页上提现与划转均为关闭
  - 每把 key 的 IP 白名单恰好是 dev2 `egress.toml` 中 `prod` 列表的地址
  - Secrets Manager 中每把 key 位于 `hft/testnet/<account>/<venue>/…`;`hft-secrets --check` 退出 0
  - ops-log、仓库与 journal 中只出现 key 的 ID 与创建时间(对 journal 与 ops-log 按 key 前 8 个字符检索,命中 0 次)
- 证据:`ops-log/<UTC 日期>-DRL-2.md`(截图、`hft-secrets --check` 输出、检索输出)
- 实现:[launch A 第 2 步](../runbooks/launch.md#a-联调dev2测试网模拟盘)(人工核对截图)

### DRL-3 从白名单外地址使用测试网 key 被交易所拒绝

- 规则:[launch A 第 2 步](../runbooks/launch.md#a-联调dev2测试网模拟盘)、[deploy 出口 IP](../../deploy/README.md#出口-ip)
- 级别:testnet
- 里程碑:M6
- 门槛:⑥
- 环境:dev2;sim 的出口地址(第二个私有 IP + EIP,不在任何白名单)
- 授权:无(只读请求)
- 步骤:对每个 (账户, venue) 的只读 key,分别以 prod 出口地址与 sim 出口地址各发一次签名的账户只读请求(脚本以 `--interface <源私有 IP>` 绑定源地址;key 只在内存中使用)
- 通过标准:
  - 从 prod 出口地址的请求返回成功,响应中的 UID 等于 refdata 的 `venue_uid`(`unverifiable` 的账户只要求成功)
  - 从 sim 出口地址的请求被交易所拒绝(记录 HTTP 状态与交易所错误码),没有任何一个返回成功
- 证据:`ops-log/<UTC 日期>-DRL-3.md`(每个请求的源地址、HTTP 状态、错误码;不含签名与 key)
- 实现:`tests/host/DRL-3.sh`

### DRL-4 测试网账户按 D 第 21 步预演设置后 `ACCOUNT_CFG` 放行、实际杠杆等于 L

- 规则:[launch A 第 3 步](../runbooks/launch.md#a-联调dev2测试网模拟盘)、[hft-risk 永续杠杆上限](../../crates/hft-risk/README.md#永续杠杆上限)、[hft-risk 启动闸门](../../crates/hft-risk/README.md#启动闸门)
- 级别:testnet
- 里程碑:M6
- 门槛:⑥
- 环境:dev2 联调窗口,测试网 `prod` 角色实例
- 授权:`[需授权]`(改交易所账户设置)
- 前置:`RiskParams.max_perp_leverage_x100 = 300`;交易所侧永续杠杆 L = 2(每个永续 inst)
- 步骤:按 [launch A 第 3 步](../runbooks/launch.md#a-联调dev2测试网模拟盘) 设置持仓模式、保证金模式、杠杆 L 与现货禁借币,再开联调窗口
- 通过标准:
  - 每个 (账户, venue) 的 `ACCOUNT_CFG` 位为 1(`/status` 与 `Note{StartupGate}` 一致)
  - `/risk <a>` 显示的每个永续 inst 实际杠杆 = 2
  - 测试网不支持的设置项逐项列在上线记录中(项名与所)
- 证据:`ops-log/<UTC 日期>-DRL-4.md`(设置截图、`/risk` 输出、不支持项清单)
- 实现:`tests/host/DRL-4.sh`

### DRL-5 测试网永续杠杆超过上限时该账户禁开并 P1,改回后放行

- 规则:[hft-risk 永续杠杆上限](../../crates/hft-risk/README.md#永续杠杆上限)、[launch D 第 21 步](../runbooks/launch.md#d-正式实盘m8)
- 级别:testnet
- 里程碑:M6
- 门槛:⑥
- 环境:dev2 联调窗口;一个绑定 latency-probe 的测试网账户 a 的永续 venue
- 授权:`[需授权]`(改交易所账户设置)
- 前置:`RiskParams.max_perp_leverage_x100 = 300`;DRL-4 已通过;latency-probe 的 `quote` 组在 a 的永续 inst 上以 `probe_period_ms = 1000` 运行
- 步骤:
  1. 在交易所把该永续 inst 的杠杆改为 4
  2. 等下一份 `AccountConfigReport`,观察 2 min
  3. 改回 2,等下一份 `AccountConfigReport`
- 通过标准:
  - 步骤 2:出现 (a, 该 venue) 的 `Note{CapSet}`(`AccountCfg`,`params[0]` 含 `LEVERAGE` 位),告警群收到 `AccountConfigDrift` P1;此后 2 min 内交易所订单历史中 a 在该 venue 没有新挂单
  - 步骤 3:出现同一 cap 的 `Note{CapCleared}`,之后 60 s 内交易所订单历史中出现该 venue 的新探测单
- 证据:`ops-log/<UTC 日期>-DRL-5.md`(两份 `AccountConfigReport` 的 journal 摘录、告警、拒单计数)
- 实现:`tests/host/DRL-5.sh`

### DRL-6 测试网现货账户开启借币时 `AUTO_BORROW` 不符、闸门不放行

- 规则:[launch D 第 21 步](../runbooks/launch.md#d-正式实盘m8)、[hft-risk 启动闸门](../../crates/hft-risk/README.md#启动闸门)
- 级别:testnet
- 里程碑:M6
- 门槛:⑥
- 环境:dev2 联调窗口;一个测试网现货账户(该所测试网支持借币设置时)
- 授权:`[需授权]`(改交易所账户设置)
- 步骤:
  1. 联调窗口外在交易所打开该现货账户的借币(OKX 为 `enableSpotBorrow = true`)
  2. 开联调窗口
  3. 关窗口,关闭借币,再开窗口
- 通过标准:
  - 步骤 2:该 (账户, venue) 的 `AccountConfigReport.mismatches` 含 `AUTO_BORROW`,`ACCOUNT_CFG` 位为 0,该账户的启动闸门不放行(`Note{StartupGate}` 的 released = 0)
  - 步骤 3:`ACCOUNT_CFG` 为 1,闸门放行
  - 测试网不提供借币设置的所记"不适用"并写明所名
- 证据:`ops-log/<UTC 日期>-DRL-6.md`
- 实现:`tests/host/DRL-6.sh`

### DRL-7 dev2 的 IAM、sim 出口地址与只含测试网账户的 prod 角色配置通过预检

- 规则:[launch A 第 4 步](../runbooks/launch.md#a-联调dev2测试网模拟盘)、[deploy 秘密与实例角色](../../deploy/README.md#秘密与实例角色)、[deploy IMDS 锁定与凭证代理](../../deploy/README.md#imds-锁定与凭证代理)、[deploy 出口 IP](../../deploy/README.md#出口-ip)
- 级别:host
- 里程碑:M6
- 环境:dev2
- 授权:`[需授权]`(改 IAM、分配私有 IP 与 EIP)
- 步骤:按 [launch A 第 4 步](../runbooks/launch.md#a-联调dev2测试网模拟盘)
- 通过标准:
  - `sudo /opt/hft/current-core/deploy/release/preflight.sh` 退出 0,输出中凭证文件、S3 `HEAD`、KMS 写入探测与 `formal_live = false` 各项均为通过
  - `hft-core --config /etc/hft/hft-core-prod.toml --check` 退出 0;该文件 `instance.feed_env = "testnet"`、`instance.order_env = "testnet"`
  - refdata 中每个 `AccountMeta` 的 `is_prod = false`、`kind = Live`、`label` 不以 `sim-` 开头
  - dev2 网卡上 sim 的第二私有 IP 关联了 EIP,该 EIP 不同于 prod 出口地址
  - 正式账户混入 dev2 refdata 时的拒绝由 CORE-041.7、CORE-041.8 验证,本用例不重做
- 证据:`ops-log/<UTC 日期>-DRL-7.md`(预检输出、`--check` 输出、`aws ec2 describe-addresses` 只读输出)
- 实现:`tests/host/DRL-7.sh`

### DRL-8 开联调窗口后第一分钟内全部测试网账户闸门放行

- 规则:[launch A 第 5 步](../runbooks/launch.md#a-联调dev2测试网模拟盘)、[release-and-deploy 第 16 步](../runbooks/release-and-deploy.md#d-启动与第一分钟验收)、[deploy dev2 护栏](../../deploy/README.md#dev2-护栏)
- 级别:testnet
- 里程碑:M6
- 门槛:⑥
- 环境:dev2
- 授权:`[需授权]`(开联调窗口)
- 前置:DRL-1、DRL-4、DRL-7 通过;`hft-dev2-guard` active
- 步骤:`sudo /opt/hft/current-core/deploy/dev2/hft-window start`,之后 60 s 内按第 16 步逐项核对
- 通过标准:
  - `/run/hft/window/active` 存在;`hft-verify`、`hft-replay-verify`、`hft-journal-archiver`、`hft-pipeline`、`hft-treasury` 为 inactive,两个批处理定时器已停
  - `hft-reconciler` 与 `hft-core@prod` 为 active;`Note{StartupStep}` 步 1–8 都有结束记录,结束记录的 bit 0(超预算)为 0
  - 每个测试网账户每个 venue 的 `Note{StartupGate}` released = 1,gate_bits 含 `IDENTITY` 与 `ACCOUNT_CFG`
  - `kill-switch selftest` 与 reconciler 启动日志中每把 key 的 UID 核对通过
- 证据:`ops-log/<UTC 日期>-DRL-8.md`
- 实现:`tests/host/DRL-8.sh`

### DRL-9 每个上线 (venue, 市场) 在测试网上完成下单、撤单、成交与用户流解码

- 规则:[launch A 第 6 步](../runbooks/launch.md#a-联调dev2测试网模拟盘)、[testing ⑥](../testing.md#8-上线门槛)
- 级别:testnet
- 里程碑:M6
- 门槛:⑥
- 环境:dev2 联调窗口;每个上线策略用到的每个 (venue, 市场) 各一个测试网账户,绑定 latency-probe
- 授权:`[需授权]`(联调窗口)
- 前置:latency-probe `enabled = true`;`quote` 组 `probe_period_ms = 1000`、`far_bp = 300 bp`;`pair` 组 `pair_period_ms = 10000`、`pair_flat_ms = 5000`;`RiskParams` 按 [latency-probe 风控参数要求](../../strategies/latency-probe/README.md#用途与运行要求)
- 步骤:运行 1 h;期间人工在交易所网页核对 3 张探测单
- 通过标准:
  - 每个 (venue, 市场) 的 journal 中:≥ 100 张 `quote` 探测单各有 `Ack` 与随后的撤单回执;≥ 10 个 `pair` 第一腿与第二腿 `Fill`;每个平仓链接终态后两腿净量为 0
  - 用户流每类回报都被解码:未知事件计数增量 = 0(`ignorable_user_events` 之外)
  - 人工核对的 3 张单在交易所订单历史中的状态、数量与 journal 一致
  - 1 h 内无 P0
- 证据:`ops-log/<UTC 日期>-DRL-9.md`(每个 (venue, 市场) 的计数表与 `seq` 区间)
- 实现:`tests/host/DRL-9.sh`

### DRL-10 测试网行情与下单连接被切断后自动重连、重同步,其间该 venue 不开新仓

- 规则:[launch A 第 6 步](../runbooks/launch.md#a-联调dev2测试网模拟盘)、[incident-response F](../runbooks/incident-response.md#f-行情静默断连限速风暴延迟超预算)、[hft-exec 报价可用](../../crates/hft-exec/README.md#报价可用连接健康)、[testing ⑤](../testing.md#8-上线门槛)
- 级别:testnet
- 里程碑:M6
- 门槛:⑤、⑥
- 环境:dev2 联调窗口;测试网账户 a(latency-probe),venue v
- 授权:`[需授权]`(在 dev2 上加临时 nft 规则;规则只匹配 hft-core 服务用户的 uid,不影响旧系统)
- 前置:`RiskParams.conn_liveness_ms = 2000`;OMS `unknown_alarm_ms = 120000`(大于断开时长与恢复时间之和,使本用例不触发 `UnknownTimeout`);latency-probe `quote` 组在 v 上运行,`probe_period_ms = 1000`
- 步骤:
  1. 加 nft 规则丢弃 hft-core 服务用户发往 v 行情端点的报文,保持 60 s,删除规则
  2. 同样对 v 的下单与用户流端点做一次,保持 60 s,删除规则
- 通过标准:
  - 步骤 1:规则生效后 3 s 起到删除规则为止,v 上没有新的探测单出环(该 inst 报价不可用,探测按 `quote_ok` 跳过);出现 v 行情连接的 `ConnState{Down}`;删除规则后 60 s 内出现 `ConnState{Up}`、该 inst 的簿重新 `Live`,之后探测单恢复
  - 步骤 2:出现 v 下单连接的 `ConnState{Down}`;恢复后重新登录、用户流 `Up`;删除规则后 60 s 内 (a, v) 的 `Unknown` 槽数为 0(期间产生的 `Unknown` 由查单收敛,没有被人工改状态)
  - 两步期间没有任何人工干预,P0 数为 0;没有新增 `Note{CapSet}`(行情与连接的可用性不设 cap)
- 证据:`ops-log/<UTC 日期>-DRL-10.md`(nft 规则的加删时刻、`ConnState` 与 cap 的 journal 摘录)
- 实现:`tests/host/DRL-10.sh`

### DRL-11 两所测试网之间的两腿链接按成交净量对冲并两腿归零

- 规则:[launch A 第 6 步](../runbooks/launch.md#a-联调dev2测试网模拟盘)、[hft-exec PairOnFill](../../crates/hft-exec/README.md#paironfill)、[latency-probe pair 探测](../../strategies/latency-probe/README.md#pair-探测)
- 级别:testnet
- 里程碑:M6
- 门槛:⑥
- 环境:dev2 联调窗口;测试网账户 a,A 腿与 B 腿在两个不同交易所
- 授权:`[需授权]`(联调窗口)
- 前置:latency-probe `pair` 组 `pair_period_ms = 10000`、`pair_flat_ms = 5000`、`pair_cross = 50 bp`;`target` 组 `target_period_ms = 60000`
- 步骤:运行 2 h
- 通过标准:
  - 每个有成交的第一腿,同一 Input 内有第二腿 `OrderDecision`,第二腿数量 = 第一腿成交净量 × 1/1(按 `pair_lot` 对齐)
  - 每个链接 `LinkNotice{Done}`;平仓后两腿净量为 0;`target` 组的开仓与 0/0 目标都到达 `Done`
  - 2 h 内 `Note{ExecBroken}` 0 条;reconciler 全部 `Ignore`
- 证据:`ops-log/<UTC 日期>-DRL-11.md`(链接数、`hedge_host` 样本数、对账报告 ID)
- 实现:`tests/host/DRL-11.sh`

### DRL-12 模拟账户上以受控故障注入制造的 Broken 按 incident-response A 处置:自动有界平仓、HedgeFailure 人工解除

- 规则:[incident-response A](../runbooks/incident-response.md#a-裸敞口nakedexposure断腿broken)、[hft-exec Broken 处置](../../crates/hft-exec/README.md#broken-处置)、[launch B 第 14 步](../runbooks/launch.md#b-模拟账户dev2)、[hft-sim](../../crates/hft-sim/README.md)(受控故障注入)
- 级别:drill
- 里程碑:M6
- 门槛:⑤、⑪
- 环境:dev2 `sim` 实例的一个模拟账户 `a`(只含 Sim 账户);两腿 group `g` 的第二腿在 venue `VB`
- 授权:`[需授权]`(Admin 开启与撤销故障注入)
- 前置:`ExecConfig.broken_auto_fix_ms = 120000`;值班不对该 group 发 `/hedgefix`;注入只作用于账户 `a` 在 `VB` 的下单(sim-gw 对其新单回拒单)
- 步骤:按 [launch B 第 14 步](../runbooks/launch.md#b-模拟账户dev2) 以 `SimFault{account = a, venue = VB, fault = RejectNew}`(Admin 经 `cli:<login>`,[control-plane](../subsystems/control-plane.md#命令集))制造拒单,等 `g` 的第一腿成交、第二腿进入 `Broken` 后发 `SimFault{account = a, venue = VB, fault = Off}` 撤销注入;按 [incident-response A](../runbooks/incident-response.md#a-裸敞口nakedexposure断腿broken) 第 1、2、5 步处置;在自动平仓之前发一次 `/resume hedgefailure <a>`;另在 `prod` 实例上发同一 `SimFault` 命令一次
- 通过标准:
  - 出现 `Note{ExecBroken}` 与 `HedgeFailure` cap 的 `Note{CapSet}`,告警群 5 min 内有人 ack
  - 自动平仓之前的 `/resume hedgefailure <a>` 得到 `CmdResult{Rejected, detail = BrokenRemaining}`
  - Broken 开始后 120 s(按引擎运行时间,不含恢复期)之后出现恰好一条 `Note{ExecAutoFixDone}` 或 `Note{ExecAutoFixFailed}`;Done 时 `Note{CapCleared}`(Broken)随后出现
  - 裸量为 0、reconciler 下一轮 `Ignore` 之后 `/resume hedgefailure <a>` 成功,该账户恢复开仓
  - 注入期间只有账户 `a` 在 `VB` 的新单被 sim-gw 拒绝;撤销注入后的新单不再被拒
  - `prod` 实例上的注入命令被拒绝,`prod` 的任何账户都没有被注入的拒单
- 证据:`ops-log/<UTC 日期>-DRL-12.md`(时间线、Note 摘录、命令与 `CmdResult`)
- 实现:[incident-response A](../runbooks/incident-response.md#a-裸敞口nakedexposure断腿broken)(人工处置)

### DRL-13 门槛 ⑥:三所测试网连续 3 天无未解释异常,缺失项有替代证据

- 规则:[testing ⑥](../testing.md#8-上线门槛)、[launch A 验收](../runbooks/launch.md#验收)
- 级别:testnet
- 里程碑:M6
- 门槛:⑥
- 环境:dev2 联调窗口,连续运行
- 授权:`[需授权]`(联调窗口)
- 步骤:联调窗口连续运行 3 个 UTC 日;每天汇总 P0/P1 与解释
- 通过标准:
  - 3 个连续 UTC 日内 P0 = 0,每条 P1 在 ops-log 有原因;护栏停掉测试网实例的时段从计时中扣除并写明停机原因
  - DRL-9–DRL-11 在每个上线 (venue, 市场) 都有通过记录
  - 测试网不提供的接口或行为逐项列出,每项有替代证据(文档示例金样或正式账户只读查询)或 Admin 签字接受;正式账户只读查询类在 A–C 记"待补证",D 第 32 步前补齐
- 证据:`ops-log/<UTC 日期>-DRL-13.md`(每日汇总、替代证据清单与签字)
- 实现:`tests/host/DRL-13.sh`

### DRL-14 关联调窗口后 journal 以 Drain 快照结束、测试网挂单为 0、批处理恢复

- 规则:[launch A 第 8 步](../runbooks/launch.md#a-联调dev2测试网模拟盘)、[hft-core dev2 护栏](../../apps/hft-core/README.md#dev2-护栏)
- 级别:testnet
- 里程碑:M6
- 门槛:⑥
- 环境:dev2
- 授权:`[需授权]`
- 步骤:`/shutdown`(Drain)→ `sudo /opt/hft/current-core/deploy/dev2/hft-window stop`
- 通过标准:
  - prod journal 最后一条记录是 Drain 快照,`HeartbeatBlock.state = Stopped`
  - 各测试网账户在交易所的挂单数为 0(交易所只读接口)
  - `/run/hft/window/active` 不存在;`hft-journal-archiver`、`hft-pipeline`、`hft-treasury` active,两个批处理定时器显示下一次运行
  - `/dev/shm/hft/prod` 已用量只剩心跳块;窗口内崩溃留下热段时的分支由 CORE-212 验证
- 证据:`ops-log/<UTC 日期>-DRL-14.md`
- 实现:`tests/host/DRL-14.sh`

### DRL-15 预置模拟实例切 Live 后出现成交与日报,消息全部带"模拟"标签

- 规则:[launch B 第 9、10 步](../runbooks/launch.md#b-模拟账户dev2)、[hft-sim 预置模拟实例池](../../crates/hft-sim/README.md#预置模拟实例池)、[pipeline 参数通道](../../python/pipeline/README.md#参数通道)
- 级别:host
- 里程碑:M4
- 环境:dev2 的 `sim` 实例(`instance.name = "sim"`),该策略预置池中空闲的模拟实例 s 与其 Sim 账户 `sim-…`
- 授权:无(`/mode … live` 由 Trader 确认)
- 前置:`sim_pool = 2`;s 的覆盖项文件 `overrides/<s>.toml` 已就绪
- 步骤:按 [launch B 第 9、10 步](../runbooks/launch.md#b-模拟账户dev2):`SetParams` 换参数 → `/simreset` → 预热(DRL-16)→ `/mode <s> <模拟账户> live`
- 通过标准:
  - `SetParams` 与 `/simreset` 各得到 `CmdResult` ok,前后 `systemctl show -p MainPID hft-core@sim` 不变(没有重启)
  - `/mode` 的 `CmdResult` ok,该 cell 有效模式为 `Live`
  - 切 Live 后 24 h 内 `/sim <模拟账户>` 显示 ≥ 1 笔成交;下一个 UTC 日的账户日报与策略日报按时生成
  - 该时段 ops_bot 推送中关于该账户的每条消息都带"模拟"标签(抽查全部推送,0 条缺标签)
  - 参数变化只写进 `overrides/<s>.toml`,其他实例的覆盖项文件内容不变(SHA-256 前后相同)
- 证据:`ops-log/<UTC 日期>-DRL-15.md`
- 实现:`tests/host/DRL-15.sh`

### DRL-16 Model 预热的块从 0 连续到 last,全部 Applied,样本数达到策略要求

- 规则:[launch Model 预热](../runbooks/launch.md#model-预热)、[pipeline Model 预热](../../python/pipeline/README.md#model-预热)、[control-plane 命令集](../subsystems/control-plane.md#命令集)
- 级别:host
- 里程碑:M4
- 环境:首次启动、或出现 `Note{ModelCheckpointDiscarded}`、或没有可用检查点的策略实例(模拟实例在 B 阶段与 host-upgrade 第 5 步,正式实例在 launch D 第 27 步各执行一次)
- 授权:无
- 步骤:按 [launch Model 预热](../runbooks/launch.md#model-预热) 的"注入":`/warmup <实例>`(或以 `hft-pipeline` 身份运行 `hft-pipeline warmup --config /etc/hft/pipeline.toml --instance <实例>`)
- 通过标准:
  - journal 中该实例本批 `InjectWarmup` 的 `chunk_no` 从 0 起连续无缺号,最后一条 `last = true`,其余 `last = false`;每条 `payload` ≤ 4096 B
  - 每条都有 `CmdResult` 且结果为 Applied;没有 `Rejected`
  - `/pipeline <实例>` 显示本次预热完成;策略报告中的样本数字段 ≥ 该策略 README 规定的值
  - 证据记下数据源(REST 或 collector)、样本区间与块数
- 证据:`ops-log/<UTC 日期>-DRL-16.md`
- 实现:`tests/host/DRL-16.sh`

### DRL-17 预热中途中断后整批从 chunk_no = 0 重发并完成

- 规则:[launch Model 预热](../runbooks/launch.md#model-预热)、[pipeline Model 预热](../../python/pipeline/README.md#model-预热)
- 级别:host
- 里程碑:M4
- 环境:dev2 `sim` 实例的一个模拟实例 s(块数 ≥ 50 的预热)
- 授权:无
- 步骤:
  1. 开始对 s 预热;journal 出现 `chunk_no = 10` 的 `InjectWarmup` 后 `systemctl stop hft-pipeline`
  2. `systemctl start hft-pipeline`,再次 `/warmup <s>`
- 通过标准:
  - 第一批在 journal 中止于某个 `last = false` 的块,之后没有同批的块
  - 第二批的 `chunk_no` 从 0 起连续到 `last = true`,全部 Applied
  - 完成后策略报告的样本数与同一数据区间一次跑完的预热(DRL-16 的记录)相同
- 证据:`ops-log/<UTC 日期>-DRL-17.md`
- 实现:`tests/host/DRL-17.sh`

### DRL-18 预热的一个数据源不可用时换另一来源完成

- 规则:[launch Model 预热](../runbooks/launch.md#model-预热)
- 级别:host
- 里程碑:M4
- 环境:dev2 `sim` 实例的模拟实例 s;策略插件同时支持交易所公共 REST 与 collector 两个来源
- 授权:`[需授权]`(临时 nft 规则,只匹配 `hft-pipeline` 服务用户的 uid)
- 步骤:
  1. 丢弃 `hft-pipeline` 服务用户到所用交易所公共 REST 的报文,`/warmup <s>`
  2. 按插件的数据源配置改用 collector,再 `/warmup <s>`;删除 nft 规则
- 通过标准:
  - 步骤 1:`/pipeline <s>` 报数据源失败,journal 中没有本批的 `last = true` 块,P1 告警一条
  - 步骤 2:块连续、全部 Applied、样本数达到要求(同 DRL-16 的标准)
  - 两个来源都不可用时实例保持 `Shadow` 的决定写在上线记录中(本次只核对记录模板中有这一栏)
- 证据:`ops-log/<UTC 日期>-DRL-18.md`
- 实现:`tests/host/DRL-18.sh`

### DRL-19 门槛 ⑦ dev2 段:模拟账户连续 ≥ 14 天,证据绑定与每日报告齐全

- 规则:[launch B 第 11、12 步](../runbooks/launch.md#b-模拟账户dev2)、[testing §8.1](../testing.md#81-模拟账户长跑的计时与证据)、[hft-sim 压力用例](../../crates/hft-sim/README.md#压力用例上线门槛必过项)
- 级别:host
- 里程碑:M6
- 门槛:⑦
- 环境:dev2 `sim` 实例;每个上线策略的模拟实例与 Sim 账户
- 授权:无
- 步骤:按 [launch B 第 11、12 步](../runbooks/launch.md#b-模拟账户dev2) 运行并每日核对
- 通过标准:
  - 每天的日报记录当日生效的 (发布版本, `StrategyParams` SHA-256, `RiskParams` SHA-256);计时按 testing §8.1 计算,累计暂停 ≤ 24 h,连续运行 ≥ 14 天
  - 每个 UTC 日:账户日报与策略日报按时生成;P0 = 0;每条 P1 在 ops-log 有原因;`/var/lib/hft/replay-verify/sim/<date>.json` 结论为通过,recompute 通过或因当日无检查点跳过(跳过时有 P2)
  - 压力用例以 ⑦ 结束时的三元组运行:MS-2(`one-sided-30`)`liquidations = 0`;MS-4(`l3-path`)依次出现 L1、L2、L3 且 `liquidations = 0`;非门槛变体的结果与强平解释写进记录
  - 护栏停 sim 与自动重启的每一次在 `events.jsonl` 有记录,并计入暂停时长
- 证据:`ops-log/<UTC 日期>-DRL-19.md`(计时表、每日报告与回放结果链接、压力用例 `StressVerdict` 全部字段)
- 实现:`tests/host/DRL-19.sh`

### DRL-20 模拟账户跨所划转后两个 venue 的模拟余额与账本一致

- 规则:[launch B 第 13 步](../runbooks/launch.md#b-模拟账户dev2)、[hft-sim 现货余额与划转](../../crates/hft-sim/README.md#8-现货余额与划转)
- 级别:drill
- 里程碑:M6
- 环境:dev2 `sim` 实例,Sim 账户 s 的 venue A、B
- 授权:无
- 前置:s 在 A 上 USDT 可用余额 ≥ 200
- 步骤:`/simtransfer <s> <A> <B> USDT 100`;再发一次 `/simtransfer <s> <A> <B> USDT <大于可用余额的数>`
- 通过标准:
  - 第一次:`CmdResult` ok;sim-gw 的 `BalanceReport` 中 A 的 USDT 减 100、B 的 USDT 加 100;此后 2 轮对账无 `SimDrift`;`/treasury <s>` 显示新的分配
  - 第二次:`CmdResult{PreconditionFailed}`,detail 为 `InsufficientBalance`;两边余额不变
- 证据:`ops-log/<UTC 日期>-DRL-20.md`
- 实现:[launch B 第 13 步](../runbooks/launch.md#b-模拟账户dev2)(人工操作,`tests/host/DRL-20.sh` 读余额与对账)

### DRL-21 模拟账户拉近强平距离时依次进入 L1、L2、L3,恢复后按滞回逐档回到 L0

- 规则:[launch B 第 13 步](../runbooks/launch.md#b-模拟账户dev2)、[incident-response M](../runbooks/incident-response.md#m-保证金压力marginstress)、[hft-risk 保证金分档](../../crates/hft-risk/README.md#保证金分档marginstress)
- 级别:drill
- 里程碑:M6
- 环境:dev2 `sim` 实例,Sim 账户 s 在永续 venue P 上有永续持仓、在现货 venue S 上有对应现货
- 授权:无(`SetRisk` 收紧由 Admin 经运维 CLI 发出,放宽要确认)
- 前置:读 `/risk <s>` 的 P 上 `liq_dist` = d0(要求 d0 ≥ 0.25);Admin 以 `SetRisk` 把 s 的 `liq_dist_block_open`、`liq_dist_reduce`、`liq_dist_force` 设为 d0 − 0.05、d0 − 0.10、d0 − 0.15,`liq_dist_hyst = 0.02`、`deleverage_deadline_ms = 30000`、`force_step = 0.20`、`force_step_interval_ms = 5000`
- 步骤:
  1. 用 `/simtransfer` 把 P 的 USDT 分次转到 S,每次后看 `/risk <s>`,直到 `liq_dist` < d0 − 0.05,再到 < d0 − 0.10
  2. 到 L2 后不做任何操作,等 L2 截止
  3. L3 出步 2 次后把 USDT 转回 P,直到回到 L0;`/treasury <s>` 截图
  4. `SetRisk` 恢复原值;`/simreset <s>`
- 通过标准:
  - 步骤 1:先出现 `MarginStress` cap 的 `Note{CapSet}`(level 1)与 P1 告警,再出现 level 2 的变化与 `Note{MarginLevel}`,告警为 P0;两者之间 s 的 Open 被拒,对冲与平仓照常出环
  - 步骤 2:进入 L2 后 30 s(引擎时间)到期进入 L3(`DEADLINE_L3`),立即一步 `ForceDeleverage`、之后每 5 s 一步,每步永续名义减少 20%;永续腿先于现货腿出单
  - 步骤 3:按 0.02 的滞回逐档下降,回到 L0 时 `Note{CapCleared}`(MarginStress);`/treasury <s>` 给出划转建议
  - 步骤 4:`SetRisk` 的放宽经确认后生效;`/simreset` 后余额等于 `initial_balances`
  - 全程 sim-gw 的 `ExternalFill{Liquidation}` 为 0
- 证据:`ops-log/<UTC 日期>-DRL-21.md`(每次划转后的 `liq_dist`、档位时间线、`ForceDeleverage` 步与成交)
- 实现:`tests/host/DRL-21.sh`

### DRL-22 L3 减仓时现货 venue 报价不可用:先减永续并报 `ExecSerialFallback`,恢复后现货腿再减

- 规则:[incident-response M 第 6 步](../runbooks/incident-response.md#m-保证金压力marginstress)、[hft-exec 保证金危急的默认减仓](../../crates/hft-exec/README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:drill
- 里程碑:M6
- 环境:同 DRL-21,在其步骤 2 的 L3 期间执行
- 授权:`[需授权]`(临时 nft 规则,只匹配 `hft-sim` 服务用户的 uid 与 S 的行情端点)
- 前置:同 DRL-21;`RiskParams.conn_liveness_ms = 2000`
- 步骤:L3 第一次出步之前丢弃 sim 实例到 S 行情端点的报文;等 2 次出步;删除规则
- 通过标准:
  - 出现 `Note{ExecSerialFallback}`(P0),其后 P 上的永续减仓单照常每 5 s 一步,S 上没有现货单出环
  - 删除规则、S 的报价恢复可用后 60 s 内出现 S 上的现货减仓单,`/targets <s>` 中现货腿开始推进
- 证据:`ops-log/<UTC 日期>-DRL-22.md`
- 实现:`tests/host/DRL-22.sh`

### DRL-23 退出命令把 group 目标设为 0、`reason = Exit`,不需确认

- 规则:[launch B 第 15 步](../runbooks/launch.md#b-模拟账户dev2)、[ops_bot 策略命令与描述符](../../python/ops_bot/README.md#策略命令与描述符)
- 级别:drill
- 里程碑:M6
- 环境:dev2 `sim` 实例;上线策略的模拟实例 s 与 Sim 账户,至少 3 个有仓位的 group(g1、g2、g3)
- 授权:无
- 步骤:按 [launch B 第 15 步](../runbooks/launch.md#b-模拟账户dev2):对 g1 发 `exit`;对 g2 用 `/params <s> set force_exit g2 true`;再发实例级 `exit_all`
- 通过标准:
  - 三条命令都没有确认步骤,`CmdResult` ok(`exit` 与 `exit_all` 的 `StrategyCmd` 带 `reduce_only`)
  - g1、g2 在命令后的同一 Input 内有目标 0、`reason = Exit` 的 `TargetIntent`;`exit_all` 之后全部 group 都有这样的目标
  - 全部 group 两腿净量在 30 min 内回到 0(或剩余量在尘埃线以内),期间没有 Open
- 证据:`ops-log/<UTC 日期>-DRL-23.md`
- 实现:`tests/host/DRL-23.sh`

### DRL-24 老板以 Viewer 在模拟账户上 `/stop`,交易员恢复

- 规则:[launch B 第 16 步](../runbooks/launch.md#b-模拟账户dev2)、[ops_bot 紧急停止](../../python/ops_bot/README.md#紧急停止stop)、[ops_bot 角色与鉴权](../../python/ops_bot/README.md#角色与鉴权)
- 级别:drill
- 里程碑:M6
- 门槛:⑪
- 环境:dev2 `sim` 实例;老板的 Telegram 账号(Viewer)与值班交易员(Trader)
- 授权:无
- 步骤:按 [launch B 第 16 步](../runbooks/launch.md#b-模拟账户dev2):老板走完只读面板(账户、风险、目标、PnL、模拟报告、参数、策略 Note),`/stop <模拟账户>` 并确认;交易员 `/resume kill <模拟账户>`
- 通过标准:
  - 每个只读面板都返回内容,老板确认可读(证据中逐项打勾)
  - `/stop` 的预览含"不会平仓"提示与受影响账户;确认后 `CmdResult` ok,该账户 `Kill` cap(非 HARD、无 flatten、issuer `tg:<老板>`)生效、挂单被撤、持仓不变;全部 Trader 收到推送
  - 交易员 `/resume kill` 经确认后 `Kill` cap 删除,有效模式恢复
- 证据:`ops-log/<UTC 日期>-DRL-24.md`(截图与 journal 摘录);同时写进 ops_bot 验收记录
- 实现:[launch B 第 16 步](../runbooks/launch.md#b-模拟账户dev2)(人工)

### DRL-25 统一升级主机上用测试网账户重做 kill 演练,新部署形态的四项逐一验证

- 规则:[launch C 第 19 步](../runbooks/launch.md#c-统一升级主机m7)、[host-upgrade 第 9 步](../runbooks/host-upgrade.md#步骤)、[incident-response kill 演练](../runbooks/incident-response.md#kill-演练)、[kill-switch 本地触发](../../apps/kill-switch/README.md#本地触发ops_bot)
- 级别:drill
- 里程碑:M7
- 门槛:⑤、⑪
- 环境:统一升级主机,`prod`(`order_env = "testnet"`,只含测试网账户)与 `sim` 同机
- 授权:`[需授权]`(启停 hft-core、触发 kill-switch)
- 步骤:在本机按 DRL-88–DRL-98 执行;另核对下列四项
- 通过标准:
  - DRL-88–DRL-98 在本机全部通过
  - gw 的 kill 字:kill 期间 `/status` 中演练账户在每个 gw 的 kill 位为 1,`Resume` 后为 0
  - kill-switch 的出口:kill-switch 撤单期间 `ss -tnp` 中它到交易所的连接源地址在 `egress.toml` 的 `prod` 列表内
  - polkit:`stop_core` 时 kill-switch 以 `hft-ks` 身份对 `hft-core@prod` 的 `stop` 成功,回执 `core_stop = Stopped`
  - 触发套接字:`/run/hft/kill-switch/trigger.sock` 属主 `hft-ks:hft-ks-trigger`、权限 0660;以 `hft` 用户连接被拒并在 `actions.jsonl` 有 `trigger_denied`
- 证据:`ops-log/<UTC 日期>-DRL-25.md`;[kill 演练记录](../runbooks/incident-response.md#kill-演练记录) 增加一行
- 实现:`tests/host/DRL-25.sh`

### DRL-26 交易主机 `/healthz` 不可达时机外看门狗经 Telegram 与电话告警,恢复后发恢复通知

- 规则:[launch C 第 20 步](../runbooks/launch.md#c-统一升级主机m7)、[host-upgrade 第 10 步](../runbooks/host-upgrade.md#步骤)、[deploy 机外看门狗](../../deploy/README.md#机外看门狗)
- 级别:drill
- 里程碑:M7
- 门槛:⑪
- 环境:dev2 上的 `hft-offbox-watchdog`,目标为升级主机 ENI-A 私有 IP 的 `/healthz`;电话类告警供应商已选定
- 授权:`[需授权]`(改安全组;停 watchtower)
- 前置:`/etc/hft/offbox-watchdog.toml` 探测周期 15 s、判定 60 s;没有生效的维护静默
- 步骤:
  1. 从安全组移除 dev2 对 `healthz_port` 的放行,保持 3 min,恢复
  2. 在升级主机上 `systemctl stop hft-watchtower`,保持 3 min,`systemctl start hft-watchtower`
- 通过标准:两步各自满足
  - 看门狗日志中第一次失败探测与告警发出之间 ≤ 75 s(判定 60 s + 一个探测周期)
  - Telegram 告警与电话都到达,到达时刻记在证据中
  - 恢复后 ≤ 30 s(两个探测周期)内发出恢复通知并到达 Telegram
- 证据:`ops-log/<UTC 日期>-DRL-26.md`(看门狗日志摘录、Telegram 截图、电话记录)
- 实现:[host-upgrade 第 10 步](../runbooks/host-upgrade.md#步骤)(人工核对到达)

### DRL-27 维护静默期间同样的故障只发 Telegram,截止后恢复电话告警

- 规则:[launch C 第 20 步](../runbooks/launch.md#c-统一升级主机m7)、[deploy 机外看门狗](../../deploy/README.md#机外看门狗)、[watchtower 机外健康检查](../../apps/watchtower/README.md#机外健康检查healthz)
- 级别:drill
- 里程碑:M7
- 门槛:⑪
- 环境:同 DRL-26
- 授权:`[需授权]`
- 前置:同 DRL-26
- 步骤:在升级主机上 `sudo hft-maint start 10`;等看门狗日志出现读到的 `maintenance_until`;`systemctl stop hft-watchtower`;保持到截止后 3 min;`systemctl start hft-watchtower`
- 通过标准:
  - 看门狗在 `hft-maint start` 之后 ≤ 15 s 记录到 `maintenance_until`(约 10 min 后的时刻)
  - 截止之前:只有 Telegram 告警,没有电话
  - 截止之后 ≤ 75 s:电话告警到达
  - 恢复后发出恢复通知
- 证据:`ops-log/<UTC 日期>-DRL-27.md`
- 实现:[host-upgrade 第 10 步](../runbooks/host-upgrade.md#步骤)(人工核对到达)

### DRL-28 升级主机与 dev2 同 AZ 时第三 AZ 看门狗已启用并通过同样的演练

- 规则:[launch C 第 20 步](../runbooks/launch.md#c-统一升级主机m7)、[az-probe D 第 8 步](../runbooks/az-probe.md#d-判定)、[deploy 机外看门狗](../../deploy/README.md#机外看门狗)
- 级别:drill
- 里程碑:M7
- 门槛:⑪
- 环境:第三 AZ 的 `t4g.nano`(az-probe 报告写"需要第三 AZ 看门狗"时)
- 授权:`[需授权]`(开实例、改安全组)
- 步骤:按 DRL-26、DRL-27 的步骤,以第三 AZ 看门狗为观察点各做一次
- 通过标准:
  - az-probe 报告的结论为"需要":第三 AZ 看门狗的 DRL-26、DRL-27 通过标准全部满足,且 dev2 上的看门狗同时在运行
  - az-probe 报告的结论为"不需要":本用例记"不适用",证据链接该报告
- 证据:`ops-log/<UTC 日期>-DRL-28.md`
- 实现:[host-upgrade 第 10 步](../runbooks/host-upgrade.md#步骤)(人工)

### DRL-29 交易主机失联授权表已由负责人填好两名网页兜底人

- 规则:[incident-response S](../runbooks/incident-response.md#s-交易主机失联)、[runbooks 值班与授权](../runbooks/README.md#值班与授权)
- 级别:drill
- 里程碑:M7
- 门槛:⑪
- 环境:incident-response S 节的授权表
- 授权:负责人签字
- 步骤:负责人填写两人的名字;两人各自确认持有各所主账户的网页登录与划转、提现的二次验证
- 通过标准:
  - 授权表中网页兜底人 A、B 两行的"人"是两个不同的具体名字(不是"待负责人指定")
  - 两人的确认与负责人签字记录在证据中;正式子账户开通后(launch D 第 21 步)两人对每个正式子账户的网页登录再确认一次
- 证据:`ops-log/<UTC 日期>-DRL-29.md`
- 实现:[incident-response S](../runbooks/incident-response.md#s-交易主机失联)(人工)

### DRL-30 上线签字记录的必填数值齐全且由授权人签字

- 规则:[launch 前置条件第 4 条](../runbooks/launch.md#前置条件)、[launch D 第 32 步](../runbooks/launch.md#d-正式实盘m8)
- 级别:drill
- 里程碑:M8
- 环境:`ops-log/<date>-launch-<实例>.md` 的签字记录
- 授权:老板或负责人签字
- 步骤:授权人填写并签字;另一人逐项核对
- 通过标准:
  - 有数值:资金规模;初始 `RiskParams` 的账户总敞口、单笔名义、日亏额度、`max_perp_leverage_x100`
  - 有数值:第一次放宽的每个条件(观察天数、成交笔数、P0 与 P1 计数、reconciler `Ignore` 连续天数、MarginStress 最高档位)与放宽后的目标数值
  - 签字人是老板或负责人,签字时刻早于 D 第 28 步的 `SetRisk`
- 证据:签字记录本身与核对人的逐项勾选,`ops-log/<UTC 日期>-DRL-30.md` 链接它
- 实现:[launch D 第 32 步](../runbooks/launch.md#d-正式实盘m8)(人工)

### DRL-31 GitHub 侧的 Admin 名单与 ctl 签发者表的 Admin 是同一组人

- 规则:[launch D 第 32 步](../runbooks/launch.md#d-正式实盘m8)、[control-plane 角色](../subsystems/control-plane.md#角色)、[development 免费额度](../development.md#免费额度)
- 级别:drill
- 里程碑:M8
- 环境:`.github/hft-admins.txt` 与升级主机上的 ctl 签发者表 `/etc/hft/ctl-principals.toml`(deploy 生成,hft-core 只在启动时加载)
- 授权:负责人签字
- 步骤:负责人把两份名单逐人对应(GitHub 登录名 ↔ ctl 签发者);若修正了签发者表,重启 hft-core 后再核一次
- 通过标准:
  - 两份名单的人员集合相同:每个 GitHub 登录名对应 ctl 签发者表中恰好一个 Admin,反之亦然
  - 不一致的条目在核对当天修正,修正后重新核对一次
- 证据:`ops-log/<UTC 日期>-DRL-31.md`(对应表与负责人签字)
- 实现:[launch D 第 32 步](../runbooks/launch.md#d-正式实盘m8)(人工)

### DRL-32 每个交易所恰好一个正式子账户、一个市场,杠杆与禁借币设置生效

- 规则:[launch D 第 21 步](../runbooks/launch.md#d-正式实盘m8)、[hft-risk 永续杠杆上限](../../crates/hft-risk/README.md#永续杠杆上限)
- 级别:drill
- 里程碑:M8
- 环境:各所主账户后台;升级主机上的正式 `prod`(D 第 24 步之后)
- 授权:`[需授权]`(开子账户、改账户设置)
- 前置:签字记录中的杠杆 L 与 `max_perp_leverage_x100`
- 步骤:按 [launch D 第 21 步](../runbooks/launch.md#d-正式实盘m8)
- 通过标准:
  - 实例用到的每个交易所恰好一个子账户,命名 `<实例>-<交易所>-<spot|perp>`;现货子账户未开通合约与杠杆
  - 设置截图齐全(持仓模式、保证金模式、多资产、手续费抵扣、杠杆、借币)
  - `/risk <a>` 显示的每个永续 inst 实际杠杆 = L,且 L ≤ `max_perp_leverage_x100` / 100
  - 每个现货 venue 的 `AccountConfigReport.mismatches` 不含 `AUTO_BORROW`
- 证据:`ops-log/<UTC 日期>-DRL-32.md`
- 实现:[launch D 第 21 步](../runbooks/launch.md#d-正式实盘m8)(人工核对截图,`tests/host/DRL-32.sh` 读 `/risk` 与 `AccountConfigReport`)

### DRL-33 正式 key 不带提现与划转,白名单只含升级主机的 prod 出口 EIP,每把 key 的 UID 被使用它的进程核对

- 规则:[launch D 第 22 步](../runbooks/launch.md#d-正式实盘m8)、[tools hft-secrets](../../tools/README.md#hft-secrets)、[kill-switch 身份核验](../../apps/kill-switch/README.md#身份核验uid)
- 级别:drill
- 里程碑:M8
- 门槛:⑪
- 环境:各正式子账户后台;升级主机
- 授权:`[需授权]`(建 key、写 Secrets Manager)
- 步骤:按 [launch D 第 22 步](../runbooks/launch.md#d-正式实盘m8)
- 通过标准:
  - 交易 key 与撤单 key 的权限页上提现、划转为关闭;只读 key 只有读权限
  - 每把 key 的白名单恰好等于升级主机 `egress.toml` 中 `prod` 列表的 EIP;不含 sim 出口 EIP、ENI-B2 的 EIP 与 dev2 地址
  - 每把 key 位于 `hft/live/<account>/<venue>/…`;ops-log 中只出现 key 的 ID 与创建时间
  - D 第 23 步启动后:hft-core 对每把交易 key 的 `IDENTITY` 通过;`kill-switch selftest` 与 reconciler 启动日志的 UID 核对通过
- 证据:`ops-log/<UTC 日期>-DRL-33.md`(截图、白名单清单与 `egress.toml` 的对照)
- 实现:[launch D 第 22 步](../runbooks/launch.md#d-正式实盘m8)(人工)

### DRL-34 测试网到正式配置的切换在一次维护静默内完成,八个小步的检查点全部满足

- 规则:[launch D 第 23 步](../runbooks/launch.md#d-正式实盘m8)、[journal-archiver 配置](../../apps/journal-archiver/README.md#输出s3-布局)、[deploy 主机清单](../../deploy/README.md#主机清单)、[deploy 验证包](../../deploy/README.md#验证包与-hft-verify)
- 级别:drill
- 里程碑:M8
- 环境:升级主机;dev2 的机外看门狗
- 授权:`[需授权]`
- 前置:正式 refdata、`hft-core-prod.toml`、kill-switch 与 reconciler 的正式配置、新的 `/etc/hft/pipeline.toml` 已评审;值班已被告知可能的电话告警
- 步骤:按 [launch D 第 23 步](../runbooks/launch.md#d-正式实盘m8) 第 1–8 小步
- 通过标准:
  - 第 1 小步:看门狗在 ≤ 15 s 内读到 `maintenance_until`;prod journal 最后一条是 Drain 快照
  - 第 2 小步:`journal-archiver check --prefix hft-testnet` 输出未提交小时数 0、quarantine 数 0(不为 0 时停止,等追平或按 journal-archiver 清零后重跑本小步,两次输出都存证)
  - 第 3 小步:列出的冷进程、kill-switch 与 `hft-verify.timer`/`hft-verify.service` 均 inactive;`pending_kill.json` 中没有 `pending` 条目,文件已改名为 `pending_kill.testnet-<date>.json`
  - 第 4、5 小步:`<cold_root>/prod-testnet-<date>/` 存在且 `<cold_root>/prod/` 已不存在;hugetlbfs 上 prod 子目录中没有测试网热段文件
  - 第 6 小步:`stop-marker.json`、`.consumed`、`.aborted` 都在 `testnet-<date>/` 下,原目录没有它们;pipeline 中该实例的 `versions/`、`overrides/`、状态目录已移到 `/var/lib/hft/pipeline/testnet-<date>/`
  - 第 7 小步:archiver prod source 前缀为 `hft`;`/etc/hft/host-local.toml` 的 `manifest = "tokyo-cex-1-live"`;重新激活 `core` 组后 `systemctl show -p LoadState hft-verify.service hft-verify.timer` 都是 `not-found`;refdata 中全部 `AccountMeta.is_prod = true`、`label` 不以 `sim-` 开头、每个交易所只出现一次;`instance.feed_env = "prod"`、`instance.order_env = "prod"`;`preflight.sh` 退出 0
  - 第 8 小步:`hft-kill-switch` 早于 `hft-core@prod` 进入 active;全部策略实例 `RunMode = Shadow`;`hft-maint stop` 后 `maintenance_until` 为空;第 3–8 小步落在一次 ≤ 30 min 的静默内(或证据中有续期记录)
- 证据:`ops-log/<UTC 日期>-DRL-34.md`(每小步的命令输出与时刻,搬走的目录与归档位置)
- 实现:[launch D 第 23 步](../runbooks/launch.md#d-正式实盘m8)(人工,`tests/host/DRL-34.sh` 只读核对第 7、8 小步)

### DRL-35 正式账户每个 venue 的启动闸门全 1,kill-switch 目标表只含正式账户

- 规则:[launch D 第 24 步](../runbooks/launch.md#d-正式实盘m8)、[hft-risk 启动闸门](../../crates/hft-risk/README.md#启动闸门)、[kill-switch 撤单目标](../../apps/kill-switch/README.md#撤单目标)
- 级别:drill
- 里程碑:M8
- 门槛:⑪
- 环境:升级主机,正式 `prod`
- 授权:无
- 步骤:按 [launch D 第 24 步](../runbooks/launch.md#d-正式实盘m8)
- 通过标准:
  - 每个正式账户每个 venue 的最后一条 `Note{StartupGate}` released = 1,gate_bits 含 `IDENTITY`、`ACCOUNT_CFG`、`RECONCILED` 与 kill 位自检
  - `kill-switch selftest` 退出 0;`kill-switch status` 的目标表中每个账户都在正式 refdata 中,测试网账户数为 0
- 证据:`ops-log/<UTC 日期>-DRL-35.md`
- 实现:`tests/host/DRL-35.sh`

### DRL-36 入金逐笔登记,余额基线登记后对账连续两轮全部 `Ignore`

- 规则:[launch D 第 25、26 步](../runbooks/launch.md#d-正式实盘m8)、[incident-response B3](../runbooks/incident-response.md#b3-资金费差异)
- 级别:drill
- 里程碑:M8
- 环境:各所主账户后台;升级主机正式 `prod`
- 授权:`[需授权]`(划转资金)
- 步骤:按 [launch D 第 25、26 步](../runbooks/launch.md#d-正式实盘m8)
- 通过标准:
  - 每笔划转在交易所的划转号与一条 `RecordTransfer` 一一对应(数量与资产相同)
  - 非策略资产都有 `SetBalanceBaseline`;之后 reconciler 对每个正式账户每个 venue 连续两轮全部 `Ignore`(资金费条目不计)
- 证据:`ops-log/<UTC 日期>-DRL-36.md`(划转号与 `RecordTransfer` 对照表、两轮报告 ID)
- 实现:[launch D 第 25 步](../runbooks/launch.md#d-正式实盘m8)(人工)

### DRL-37 `SetRisk` 生效后的每个上限与签字记录逐项相等

- 规则:[launch D 第 28 步](../runbooks/launch.md#d-正式实盘m8)、[hft-risk 配置](../../crates/hft-risk/README.md#配置riskparams版本化经-setrisk-替换并作为-configblob-写入-journal)
- 级别:drill
- 里程碑:M8
- 环境:升级主机正式 `prod`
- 授权:`[需授权]`(Admin 经运维 CLI,确认)
- 步骤:按 [launch D 第 28 步](../runbooks/launch.md#d-正式实盘m8)
- 通过标准:
  - `SetRisk` 的 `CmdResult` ok;journal 中新的 `ConfigBlob`(`RiskParams`)SHA-256 记入证据
  - `/risk <a>` 显示的每个上限等于签字记录中的数值(逐项对照表无差异)
  - `SetParams` 以 `InvalidArgs{LotMismatch}` 拒绝时,调整与改动已回写签字记录并重新签字
- 证据:`ops-log/<UTC 日期>-DRL-37.md`
- 实现:`tests/host/DRL-37.sh`

### DRL-38 D-smoke:冒烟单按预览挂出,`ttl_ms` 到期由 OMS 撤销,不出现 `Recon`

- 规则:[launch D 第 30 步](../runbooks/launch.md#d-正式实盘m8)、[hft-oms 冒烟单](../../crates/hft-oms/README.md#冒烟单smokeorder)
- 级别:drill
- 里程碑:M8
- 门槛:⑪
- 环境:升级主机正式 `prod`,实例 `RunMode = Shadow`,维护窗口内;每个 (正式账户, venue) 各一次
- 授权:`[需授权]`
- 前置:`smoke_far_bp = 300 bp`;`ttl_ms = 10000`
- 步骤:按 [launch D 第 30 步](../runbooks/launch.md#d-正式实盘m8) 第 1 小步
- 通过标准:
  - 预览中的账户标签等于 a 的 `AccountMeta.label`,限价与对手价的距离 ≥ 300 bp,数量 = 最小名义 × 1.1 向上取整到 lot
  - 确认后 `CmdResult` ok;journal 中同一 Input 内有该单的 `OrderDecision`;`/orders <a>` 显示该单且槽带 `SMOKE` 标志
  - 下单后 10 s(± 1 s)出现该槽因 `SmokeTtl` 的撤单,交易所订单历史中该单为已撤销、成交量 0
  - 之后两轮对账中该 (account, venue) 没有 `Recon` cap;订单号记入上线记录
- 证据:`ops-log/<UTC 日期>-DRL-38.md`
- 实现:`tests/host/DRL-38.sh`

### DRL-39 D-smoke:不撤挂单的 Drain 后冒烟单由交易所死手在时限内撤销

- 规则:[launch D 第 30 步](../runbooks/launch.md#d-正式实盘m8)、[hft-oms 冒烟单](../../crates/hft-oms/README.md#冒烟单smokeorder)、[kill-switch 各所死手能力与兜底](../../apps/kill-switch/README.md#各所死手能力与兜底)
- 级别:drill
- 里程碑:M8
- 门槛:⑪
- 环境:同 DRL-38;只在 `VenueCaps.dms ≠ None` 的 venue
- 授权:`[需授权]`
- 前置:`ttl_ms = 60000`
- 步骤:按 [launch D 第 30 步](../runbooks/launch.md#d-正式实盘m8) 第 2 小步:冒烟单 → 核对死手刷新 → `Drain{cancel_resting = false}` → 等交易所撤单 → 重启 prod
- 通过标准:
  - `/status` 显示 gw 对该 inst 的死手刷新在进行
  - hft-core 干净退出(journal 末尾是 Drain 快照),kill-switch 无撤单动作(`actions.jsonl` 无该时段记录)
  - journal 中该 SMOKE 槽在退出前没有撤单请求;交易所订单历史中的撤销时刻 − hft-core 退出时刻 ≤ 该所死手时限,撤销原因为死手
  - 重启后闸门全 1;没有死手的 venue 记"不适用"
  - OMS 先撤了该单 → 不通过(缺陷),修复后重做本用例
- 证据:`ops-log/<UTC 日期>-DRL-39.md`
- 实现:`tests/host/DRL-39.sh`

### DRL-40 D-smoke:`kill-switch fire` 经正式端点撤掉冒烟单并核验为 0

- 规则:[launch D 第 30 步](../runbooks/launch.md#d-正式实盘m8)、[kill-switch 撤单动作](../../apps/kill-switch/README.md#撤单动作)
- 级别:drill
- 里程碑:M8
- 门槛:⑪
- 环境:同 DRL-38
- 授权:`[需授权]`
- 前置:`ttl_ms = 60000`
- 步骤:按 [launch D 第 30 步](../runbooks/launch.md#d-正式实盘m8) 第 3 小步:冒烟单 → `kill-switch fire --account <a> --venue <v> --reason "D-smoke" --yes`
- 通过标准:
  - `kill-switch fire` 输出该 (a, v) 撤单前 1、撤单后 0,`ok = true`
  - OMS 经用户流收到撤单回报,该 SMOKE 槽终结;`ttl_ms` 未到期前完成
- 证据:`ops-log/<UTC 日期>-DRL-40.md`
- 实现:`tests/host/DRL-40.sh`

### DRL-41 D-smoke 结束后对账连续两轮 `Ignore`,每个 (账户, venue) 的冒烟记录齐全

- 规则:[launch D 第 30 步](../runbooks/launch.md#d-正式实盘m8)
- 级别:drill
- 里程碑:M8
- 门槛:⑪
- 环境:同 DRL-38
- 授权:无
- 步骤:按 [launch D 第 30 步](../runbooks/launch.md#d-正式实盘m8) 第 4、5 小步
- 通过标准:
  - 每个 (正式账户, venue) 在 DRL-38–DRL-40 之后 reconciler 连续两轮全部 `Ignore`
  - 上线记录中每个 (正式账户, venue) 有:冒烟单订单号与时刻、死手撤销时刻(或"不适用")、kill-switch 核验结果、每把 key 的 UID 与 `ACCOUNT_CFG` 核验结果
- 证据:`ops-log/<UTC 日期>-DRL-41.md`
- 实现:`tests/host/DRL-41.sh`

### DRL-42 D-shadow:正式实例 Shadow ≥ 3 天,每日逐决策比对的差异全部解释

- 规则:[launch D 第 31 步](../runbooks/launch.md#d-正式实盘m8)、[replayer compare](../../apps/replayer/README.md#compare)
- 级别:drill
- 里程碑:M8
- 环境:升级主机正式 `prod`(全部策略实例 `RunMode = Shadow`)与同机 `sim` 中同参数的模拟实例(校准对)
- 授权:无
- 步骤:按 [launch D 第 31 步](../runbooks/launch.md#d-正式实盘m8) 每天运行 `replayer compare --a <prod 当日 journal> --b <sim 当日 journal> --map <正式账户>=<模拟账户> --out <dir>`
- 通过标准:
  - 连续 ≥ 3 个、≤ 7 个 UTC 日;每天的 compare 报告中每条差异有原因分类(行情接入差异、账户状态差异、缺陷)
  - 每天 reconciler 全部 `Ignore`;资金费流水与账本一致,每条 `Note{FundingCorrected}` 有解释;P0 = 0;每条 P1 有解释
  - 期间正式账户在交易所除冒烟单外没有任何订单(交易所订单历史)
- 证据:`ops-log/<UTC 日期>-DRL-42.md`(每日 compare 报告路径与差异解释表)
- 实现:`tests/host/DRL-42.sh`

### DRL-43 D-shadow 出现未解释差异时修复后重计 3 天,决策不变的修复不使 ⑦ 重新计时

- 规则:[launch D 第 31 步](../runbooks/launch.md#d-正式实盘m8)、[testing §8.1](../testing.md#81-模拟账户长跑的计时与证据)
- 级别:drill
- 里程碑:M8
- 环境:同 DRL-42,出现第一条未解释差异时
- 授权:无(修复的发布按 release-and-deploy 授权)
- 步骤:记录差异 → 修复并发布 → D-shadow 从 0 重新计天
- 通过标准:
  - D-shadow 记录中重计起点 = 修复版本激活后的第一个完整 UTC 日;重计后满 3 天才进入 D 第 32 步
  - 修复属于 testing §8.1 第 5 条"决策不变的修复"时(diff 路径与最近 7 天 `replayer verify` 逐字节一致的证据附上),⑦ 的计时表没有重新计时;不属于时 ⑦ 按 §8.1 第 4 条重新计时
- 证据:`ops-log/<UTC 日期>-DRL-43.md`
- 实现:[launch D 第 31 步](../runbooks/launch.md#d-正式实盘m8)(人工判定)

### DRL-44 门槛核对表列齐 ①–⑧、⑩、⑪ 的证据,③④ 按候选版本从账本读出

- 规则:[launch D 第 32 步](../runbooks/launch.md#d-正式实盘m8)、[testing §8](../testing.md#8-上线门槛)、[testing §6.1](../testing.md#61-服务器上的累计运行)、[tools hft-verify](../../tools/README.md#hft-verify)
- 级别:drill
- 里程碑:M8
- 环境:dev2(以 `hft-archive` 身份同步 S3 `hft/verify/`);上线记录
- 授权:负责人签字
- 步骤:按 [launch D 第 32 步](../runbooks/launch.md#d-正式实盘m8);运行 `hft-verify ledger show --candidate <开正式实盘所用版本> --runs-from <同步目录>`
- 通过标准:
  - 核对表中 ①–⑧、⑩、⑪ 每项都有证据链接,链接目标存在;D-smoke(DRL-38–DRL-41)与 D-shadow(DRL-42)记录、签字记录(DRL-30)都在
  - `hft-verify ledger show` 的输出存入上线记录,③④ 的累计量达到 testing §8 的数值且候选版本补跑已完成
  - ⑦ 有两份证据:DRL-19(dev2 段)与 DRL-59(满规模段)
  - 缺任何一项 → 不进入 D 第 33 步
- 证据:上线记录中的核对表;`ops-log/<UTC 日期>-DRL-44.md` 链接它
- 实现:[launch D 第 32 步](../runbooks/launch.md#d-正式实盘m8)(人工)

### DRL-45 切 Live 的预览首行显示实盘账户标签与当前有效模式,确认后生效

- 规则:[launch D 第 33 步](../runbooks/launch.md#d-正式实盘m8)、[ops_bot 二次确认](../../python/ops_bot/README.md#二次确认)
- 级别:drill
- 里程碑:M8
- 环境:升级主机正式 `prod`
- 授权:`[需授权]`
- 步骤:Trader `/mode <实例> <正式账户> live`,核对预览后确认
- 通过标准:
  - 预览首行为 `[💰实盘·…]` 形式的目标账户标签,并显示当前有效模式 `Shadow`
  - 确认后 `CmdResult` ok,该账户各 cell 期望模式与有效模式为 `Live`
- 证据:`ops-log/<UTC 日期>-DRL-45.md`(预览截图、`CmdResult`)
- 实现:[launch D 第 33 步](../runbooks/launch.md#d-正式实盘m8)(人工)

### DRL-46 切 Live 后首个 UTC 日的每一笔成交与交易所核对一致

- 规则:[launch D 第 34 步](../runbooks/launch.md#d-正式实盘m8)、[launch 验收 D](../runbooks/launch.md#验收)
- 级别:drill
- 里程碑:M8
- 环境:升级主机正式 `prod`
- 授权:无
- 步骤:按 [launch D 第 34 步](../runbooks/launch.md#d-正式实盘m8),每笔成交后核对
- 通过标准:
  - 首日每一笔成交:`/p <a>` 的两腿数量(按净量,含按基础币扣的手续费)与交易所网页或 REST 一致
  - 首日内 `/targets <a>` 没有 `Stuck` 或 `Broken`;`/risk <a>` 没有 `ExternalFill`;journal 没有 `Note{ExecAutoFixDone}` 或 `Note{ExecAutoFixFailed}`
  - 任一不一致 → 按 incident-response 处理并在核对表中记录处理结果
- 证据:`ops-log/<UTC 日期>-DRL-46.md`(首日成交核对表:两腿、净量、手续费资产、两腿间隔、核对结果)
- 实现:`tests/host/DRL-46.sh`

### DRL-47 每日对账全部 `Ignore`,模拟与实盘的校准报告按日生成

- 规则:[launch D 第 29、35 步](../runbooks/launch.md#d-正式实盘m8)、[hft-sim 与正式实盘并行校准](../../crates/hft-sim/README.md#与正式实盘并行校准)
- 级别:drill
- 里程碑:M8
- 环境:升级主机正式 `prod` 与 `sim` 中的校准对
- 授权:无(`SimAccountCfg` 回写按 refdata 变更授权)
- 步骤:按 [launch D 第 35 步](../runbooks/launch.md#d-正式实盘m8) 每日执行
- 通过标准:
  - pipeline 配置中校准对已声明;两者共享的覆盖项只出现在 `scope = pair` 的写入中
  - 每个 UTC 日 reconciler 全部 `Ignore`(资金费条目除外);`/sim <模拟> vs <实盘>` 报告含滑点、成交率、两腿间隔与延迟差异四项
  - `SimAccountCfg` 的调整只经 refdata 变更回写(DRL-80 的记录),在维护窗口内
- 证据:`ops-log/<UTC 日期>-DRL-47.md`
- 实现:`tests/host/DRL-47.sh`

### DRL-48 第一次放宽限额只在签字条件全部满足后按签字目标值执行

- 规则:[launch D 第 36 步](../runbooks/launch.md#d-正式实盘m8)
- 级别:drill
- 里程碑:M8
- 环境:升级主机正式 `prod`
- 授权:`[需授权]`(放宽的 `SetRisk` 需确认)
- 步骤:按 [launch D 第 36 步](../runbooks/launch.md#d-正式实盘m8)
- 通过标准:
  - 放宽之前,签字记录中每个条件都有满足的证据(天数、笔数、P0/P1 计数、`Ignore` 连续天数、MarginStress 最高档位的实际值)
  - `SetRisk` 之后 `/risk <a>` 的值等于签字的目标值
  - 条件未全部满足时的放宽有授权人新的签字,签字时刻早于该 `SetRisk`
  - 每次调整记录日期、新值与授权人
- 证据:`ops-log/<UTC 日期>-DRL-48.md`
- 实现:[launch D 第 36 步](../runbooks/launch.md#d-正式实盘m8)(人工)

## 4. 统一升级主机(host-upgrade)

### DRL-49 主机档位的依据已记录,两份主清单与待命清单按结论更新且生成器无差异

- 规则:[host-upgrade 前置条件第 1、2 条](../runbooks/host-upgrade.md#前置条件)、[deploy 主机清单](../../deploy/README.md#主机清单)、[hft-core 线程计划](../../apps/hft-core/README.md#线程计划)
- 级别:host
- 里程碑:M7
- 环境:仓库 `deploy/hosts/`;CI
- 授权:负责人的升级决定(签字)
- 步骤:按 [host-upgrade 前置条件第 2 条](../runbooks/host-upgrade.md#前置条件)
- 通过标准:
  - ops-log 与 M3 末探针的 Agent Note 中有选定档位(`c32` 或 `c16`)与是否启用 ENI-B2 的结论及其数据;选 `c16` 时同时记录探针达标与启用 `epoll_pwait(0)` 的证据
  - `hosts/tokyo-cex-1.toml` 与 `hosts/tokyo-cex-1-live.toml` 的 `instance_type`、`class`、`[eni_b2]` 与结论一致,两份 `.standby` 同步;CI 中 `cargo xtask gen-host --check` 通过
  - 两个实例的线程数 ≤ 核数、IRQ 专用核上无线程、每个下单 venue 的 gw 独占线程、sim 只在非隔离核、内存合计加系统余量 ≤ 实例内存(生成器输出无错误)
- 证据:`ops-log/<UTC 日期>-DRL-49.md`(决定、PR 链接、CI 运行链接)
- 实现:`tests/host/DRL-49.sh`(读 CI 结论与生成物)

### DRL-50 升级主机的网卡、出口地址、待命网卡、冷盘卷与安全组按清单就位

- 规则:[host-upgrade 前置条件第 3、4 条](../runbooks/host-upgrade.md#前置条件)、[deploy 出口 IP](../../deploy/README.md#出口-ip)、[deploy 主机清单](../../deploy/README.md#主机清单)
- 级别:host
- 里程碑:M7
- 环境:新实例(az-probe 选定的 AZ 与 CPG);第二 AZ
- 授权:`[需授权]`(AWS 资源)
- 步骤:按 [host-upgrade 前置条件第 3、4 条](../runbooks/host-upgrade.md#前置条件)
- 通过标准:
  - ENI-B、ENI-C(与启用时的 ENI-B2)挂在清单写的设备序号上;ENI-B 私有 IP 关联 prod 的 EIP,ENI-C 关联 sim 的 EIP,ENI-B2 的 EIP 不在任何 key 白名单中
  - `hft-egress-ip.sh` 退出 0;`/etc/hft/egress.toml` 与清单 `[egress]` 一致
  - 第二 AZ 中待命的 ENI-B、ENI-C 存在、状态为未挂载,其私有 IP 写在待命清单中
  - 冷副本卷是独立 gp3,容量、吞吐与 IOPS 等于清单 `[storage]`
  - 安全组对 `healthz_port` 只放行 dev2(与启用时的第三 AZ 看门狗)的私有 IP
  - `hostprobe verify` 对本档期望剖面退出 0;`/etc/hft/host-local.toml` 的 `manifest = "tokyo-cex-1"`
- 证据:`ops-log/<UTC 日期>-DRL-50.md`(只读的 `aws ec2 describe-*` 输出、hostprobe 输出)
- 实现:`tests/host/DRL-50.sh`

### DRL-51 升级主机的安装、两个实例的配置与秘密隔离通过全部检查

- 规则:[host-upgrade 前置条件第 5–7、9 条](../runbooks/host-upgrade.md#前置条件)、[deploy 秘密与实例角色](../../deploy/README.md#秘密与实例角色)、[deploy IMDS 锁定与凭证代理](../../deploy/README.md#imds-锁定与凭证代理)、[deploy 模拟实例](../../deploy/README.md#模拟实例sim)
- 级别:host
- 里程碑:M7
- 环境:新实例
- 授权:`[需授权]`(IAM)
- 步骤:按 [host-upgrade 前置条件第 5–7、9 条](../runbooks/host-upgrade.md#前置条件)
- 通过标准:
  - 当前正式版已安装,`core`、`cold`、`sim` 三组都已激活且全部单元 inactive;验证包已 `install-verify`;`preflight.sh` 退出 0
  - `hft-core --config /etc/hft/hft-core-prod.toml --check` 与 `hft-core --config /etc/hft/hft-core-sim.toml --check` 都退出 0;prod 配置 `instance.feed_env = "prod"`、`instance.order_env = "testnet"`,refdata 只含测试网账户;sim 配置 `instance.name = "sim"`,refdata 只含 Sim 账户
  - journal-archiver 的 prod source 前缀为 `hft-testnet`
  - `hft-secrets --check` 退出 0;以 `hft-sim` 身份读秘密 tmpfs 得到权限拒绝;以非 root 用户连接 IMDS 被拒
- 证据:`ops-log/<UTC 日期>-DRL-51.md`
- 实现:`tests/host/DRL-51.sh`

### DRL-52 测试网 key 的白名单加入新 prod 出口 EIP,sim 与 ENI-B2 的 EIP 不在任何白名单

- 规则:[host-upgrade 前置条件第 8 条](../runbooks/host-upgrade.md#前置条件)、[deploy 出口 IP](../../deploy/README.md#出口-ip)
- 级别:testnet
- 里程碑:M7
- 门槛:⑥
- 环境:新实例;各所测试网账户后台
- 授权:`[需授权]`(改白名单)
- 步骤:按前置条件第 8 条逐 key 加白名单;再按 DRL-3 的方法从新机的三个出口地址各发一次签名只读请求
- 通过标准:
  - 每把测试网 key 的白名单含新机 prod EIP(以及 dev2 的 prod 地址,直到第 13 步删除);逐 key 打勾记录
  - 从新机 prod 出口的请求成功;从 sim 出口与 ENI-B2 出口的请求被交易所拒绝
- 证据:`ops-log/<UTC 日期>-DRL-52.md`(按 key ID 的清单与请求结果)
- 实现:`tests/host/DRL-52.sh`

### DRL-53 dev2 的 sim 以 Drain 停用,新机 sim 只含 Sim 账户、不持 key、从 ENI-C 出站

- 规则:[host-upgrade 第 1–4 步](../runbooks/host-upgrade.md#步骤)、[hft-core 实例角色](../../apps/hft-core/README.md#实例角色prod-与-sim)
- 级别:host
- 里程碑:M7
- 环境:dev2 与新实例
- 授权:`[需授权]`
- 步骤:按 [host-upgrade 第 1–4 步](../runbooks/host-upgrade.md#步骤)
- 通过标准:
  - 第 1 步的记录含每个模拟账户的期望模式、有效模式、生效 cap 与参数、覆盖项版本
  - 第 2 步:`hft-imds-lock`、`hft-cred-broker` 先于其他冷进程 active,`hft-kill-switch` 先于任何 hft-core active
  - 第 3 步:dev2 sim journal 末尾是 Drain 快照
  - 第 4 步:新机 sim 的 `Note{StartupStep}` 步 1–8 完成;refdata 只有 Sim 账户;没有下单连接,进程没有打开秘密文件;`ss -tnp` 中 sim 的交易所连接源地址全部在 `egress.toml` 的 `sim` 列表(线程落核与建连自检分别由 CORE-170、CORE-171 验证)
- 证据:`ops-log/<UTC 日期>-DRL-53.md`
- 实现:`tests/host/DRL-53.sh`

### DRL-54 新机上的模拟实例按记录的参数版本恢复、预热后切 Live 并出现成交

- 规则:[host-upgrade 第 5 步](../runbooks/host-upgrade.md#步骤)
- 级别:host
- 里程碑:M7
- 环境:新机 sim 实例
- 授权:无
- 步骤:按 [host-upgrade 第 5 步](../runbooks/host-upgrade.md#步骤)
- 通过标准:
  - 每个模拟实例生效的 `StrategyParams` 版本与第 1 步记录的相同;`overrides/<模拟实例>.toml` 内容与 dev2 上的相同(SHA-256)
  - 每个模拟实例的预热满足 DRL-16 的通过标准
  - 切 Live 后 24 h 内 `/sim` 出现成交,下一个 UTC 日日报生成
- 证据:`ops-log/<UTC 日期>-DRL-54.md`
- 实现:`tests/host/DRL-54.sh`

### DRL-55 新机 prod(测试网账户)首次启动通过第一分钟验收,连接从 ENI-B 出站

- 规则:[host-upgrade 第 6 步](../runbooks/host-upgrade.md#步骤)、[release-and-deploy 第 16 步](../runbooks/release-and-deploy.md#d-启动与第一分钟验收)
- 级别:testnet
- 里程碑:M7
- 门槛:⑥
- 环境:新机 `prod`
- 授权:`[需授权]`
- 步骤:按 [host-upgrade 第 6 步](../runbooks/host-upgrade.md#步骤)
- 通过标准:
  - 第 16 步的各项满足;每个测试网账户每个 venue 的 `Note{StartupGate}` released = 1,`IDENTITY` 经新机出口地址通过
  - `HostProfile` 无不符项;心跳在推进
  - `ss -tnp` 中 prod 的交易所连接源地址全部在 `egress.toml` 的 `prod` 列表
- 证据:`ops-log/<UTC 日期>-DRL-55.md`
- 实现:`tests/host/DRL-55.sh`

### DRL-56 门槛 ⑩ 测量窗口:标志挡住验证包,窗口前做 A/A,窗口长度与延长不超过 14 天

- 规则:[host-upgrade 第 7、11 步](../runbooks/host-upgrade.md#步骤)、[testing §8.2 第 1、7 条](../testing.md#82-延迟验收的测法)、[deploy 验证包](../../deploy/README.md#验证包与-hft-verify)
- 级别:host
- 里程碑:M7
- 门槛:⑩
- 环境:统一升级主机,`prod`(`instance.feed_env = "prod"`、`instance.order_env = "testnet"`)与 `sim` 同机
- 授权:`[需授权]`
- 前置:watchtower `interference.max_halfwidth = 0.03`、`interference.funding_hours_utc = [0, 8, 16]`、`interference.block_hours = 2`
- 步骤:按 [host-upgrade 第 7 步](../runbooks/host-upgrade.md#步骤):写测量窗口标志 → A/A(sim 全关 1–2 天,`watchtower interference --aa`)→ 按 A/A 定窗口长度 → 窗口(排程见 DRL-57)→ 需要时按 2 天一段延长 → 第 11 步删除标志
- 通过标准:
  - 标志存在期间(A/A 与窗口全程,含其间的主机重启)`hft-verify` 运行次数为 0,每次定时器到点 `journalctl` 记录 `ExecCondition` 跳过
  - A/A 报告给出每组 R 的区间与半宽,区间包含 1;不包含时窗口开始前已有查明的原因与处理记录
  - 窗口初始长度等于按 testing §8.2 第 7 条由 A/A 半宽与 `max_halfwidth = 0.03` 算出的整天数(7–14),计算过程写进证据
  - 每次延长恰好 2 天、按同一配平规则排程;窗口总长 ≤ 14 天;到 14 天仍区间过宽的组在验收记录中标"只记录"并有 Admin 签字
  - 删除标志后 20 min 内出现一次新的 `hft-verify` 运行记录
  - ⑩ 本身的判定由 CORE-407 记录,本用例只核对测量过程
- 证据:`ops-log/<UTC 日期>-DRL-56.md`(标志写入与删除时刻、A/A 报告、窗口与延长段的起止)
- 实现:`tests/host/DRL-56.sh`

### DRL-57 同机干扰的开关排程按种子与偶数整点执行,两类块各自配平,结束后 sim 在运行

- 规则:[host-upgrade 第 8 步](../runbooks/host-upgrade.md#步骤)、[testing §8.2 第 7 条](../testing.md#82-延迟验收的测法)、[deploy systemd 单元](../../deploy/README.md#systemd-单元)
- 级别:host
- 里程碑:M7
- 门槛:⑩
- 环境:同 DRL-56
- 授权:`[需授权]`
- 前置:同 DRL-56;种子 S 与窗口起点 T0(UTC 偶数整点)记入 ops-log
- 步骤:按 [host-upgrade 第 8 步](../runbooks/host-upgrade.md#步骤) 写 `/etc/hft/measure.toml`,`systemctl enable --now hft-sim-alternate.timer`;窗口结束后 `disable --now`
- 通过标准:
  - `measure.toml` 的 `start` 是 UTC 偶数整点、分秒为 0;`seed` 与 ops-log 记录相同
  - `watchtower interference` 的块表中每个非 `Mixed` 块恰好一小时开、一小时关;`Mixed` 块数记入报告
  - 每一段(初始窗口与每次延长)内,资金费块(含 00:00、08:00、16:00 UTC 整点)与普通块各自"先开"与"先关"的块数相差 ≤ 1
  - 窗口内若有主机重启,重启后下一个整点 sim 的状态与排程一致(`Persistent=true`)
  - 结束后 `systemctl is-enabled hft-sim-alternate.timer` = disabled,`hft-core@sim` active
- 证据:`ops-log/<UTC 日期>-DRL-57.md`(块表、配平计数、种子)
- 实现:`tests/host/DRL-57.sh`

### DRL-58 并发存储测量:批处理、回放与 sim 检查点同时进行 1 h,prod 的 persist 滞后 p99 不超过禁开门限的一半

- 规则:[host-upgrade 第 11 步](../runbooks/host-upgrade.md#步骤)、[hft-core 配置文件字段](../../apps/hft-core/README.md#配置文件字段)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机,⑩ 窗口之外
- 授权:无
- 前置:`storage.persist_lag_block_ms = 1000`
- 步骤:同时让 journal-archiver 补传积压、`hft-replay-verify` 运行、sim 写检查点,持续 ≥ 1 h;记录 `iostat -x 1`
- 通过标准:
  - prod 的 persist 滞后 p99 ≤ 500 ms;超过时按第 11 步调整清单 `[storage]` 或 I/O 带宽上限后重测,直到满足,每次的值都记入证据
  - 测量期间没有 `Storage` cap 的 `Note{CapSet}`
- 证据:`ops-log/<UTC 日期>-DRL-58.md`(直方图与 `iostat` 原始文件)
- 实现:`tests/host/DRL-58.sh`

### DRL-59 门槛 ⑦ 满规模段:升级主机 sim 满规模连续 ≥ 7 天并以最终参数重跑压力用例

- 规则:[host-upgrade 第 11 步](../runbooks/host-upgrade.md#步骤)、[launch C 第 18 步](../runbooks/launch.md#c-统一升级主机m7)、[testing §8.1](../testing.md#81-模拟账户长跑的计时与证据)
- 级别:host
- 里程碑:M7
- 门槛:⑦
- 环境:统一升级主机 `sim`,研究策略的全部模拟实例与 group;与 ⑩ 窗口不重叠
- 授权:无
- 步骤:按 [host-upgrade 第 11 步](../runbooks/host-upgrade.md#步骤)
- 通过标准:
  - 满规模、连续 ≥ 7 天,开始与结束时刻都在 ⑩ 窗口之外;暂停与重新计时按 testing §8.1
  - 每日标准同 DRL-19(三元组、日报、P0 = 0、P1 有解释、每日回放)
  - 结束时以当时的三元组重跑压力用例:MS-2 `liquidations = 0`,MS-4 依次出现 L1、L2、L3 且 `liquidations = 0`
  - 在 launch D 第 33 步之前完成
- 证据:`ops-log/<UTC 日期>-DRL-59.md`
- 实现:`tests/host/DRL-59.sh`

### DRL-60 换 AZ 恢复演练:主机停机后在第二 AZ 用待命网卡、新卷与 `restore` 恢复 prod,再迁回

- 规则:[host-upgrade 第 11 步](../runbooks/host-upgrade.md#步骤)、[incident-response S](../runbooks/incident-response.md#s-交易主机失联)、[journal-archiver 从 S3 恢复](../../apps/journal-archiver/README.md#从-s3-恢复restore)、[hft-core 崩溃隔离与从快照恢复](../../apps/hft-core/README.md#崩溃隔离与从快照恢复)
- 级别:drill
- 里程碑:M7
- 门槛:⑤、⑪
- 环境:统一升级主机(只含测试网账户的 `prod` 与 `sim`);第二 AZ 的待命 ENI-B/ENI-C 与待命清单 `tokyo-cex-1.standby`;最近一个通过验证的 AMI;⑩ 窗口之外、⑦ 满规模段开始之前
- 授权:`[需授权]`(停实例、开实例、挂网卡、改 EIP 关联、建卷、改看门狗目标)
- 前置:journal-archiver `restore.epoch_gap = 64`;值班已被告知本次 P0 是演练
- 步骤:
  1. `aws ec2 stop-instances` 停原实例(不终止),不做维护静默
  2. 按 [incident-response S](../runbooks/incident-response.md#s-交易主机失联) 第 1 步确认,第 2 步由网页兜底人在各测试网账户网页撤单并记录,第 3.1–3.6 步在第二 AZ 重建(冷副本用新卷与 `journal-archiver restore`,prod 与 sim 各一次;sim 最后启动)
  3. 迁回:同样按第 3 步在原 AZ 恢复——原实例换一块新的冷副本卷、`restore`、EIP 关联回原 ENI-B/ENI-C、`manifest = "tokyo-cex-1"`、看门狗目标改回;之后终止第二 AZ 的实例,待命网卡回到未挂载状态
- 通过标准:
  - 停机后看门狗的"交易主机失联"告警(Telegram 与电话)在 ≤ 75 s 内发出;第 1 步确认在告警后 5 min 内完成
  - 第二 AZ 实例安装时 `host_expect` 校验通过,`preflight.sh` 与 `hostprobe verify` 退出 0
  - prod 的 EIP 不变:第二 AZ 上 `ss -tnp` 中 prod 连接的源地址是待命 ENI-B 的私有 IP,其关联的 EIP 与原 prod EIP 相同;每个测试网账户每个 venue `IDENTITY` 通过
  - `restore` 后每段 SHA-256 与 manifest 一致;写入的 `engine_epoch` = max(S3 上的值, 最新快照的值) + 64;原实例未提交的部分小时列入 `orphans.json`;新主机之后上传的对象键带 `.r1`,archiver 零 `Conflict`
  - `--recover-from-snapshot` 启动后全部非终态订单为 `Unknown`、每个账户带 `Recovery`;`Unknown` 收敛、对账连续两轮 `Ignore` 后 `/resume recovery <a>` 成功;再次重启时没有新的 `RecoverFromSnapshot` 引导块(参数已移除)
  - 改看门狗目标后其日志出现对新地址的成功探测与一条恢复通知
  - 迁回满足同样各项(对象键恢复代为 `.r2`、零 `Conflict`、EIP 回到原 ENI-B、看门狗目标为原地址);第二 AZ 实例已终止,待命网卡为未挂载
  - 证据中有三段耗时:停机 → 告警;告警 → 第二 AZ 上全部测试网账户闸门放行;迁回总耗时
- 证据:`ops-log/<UTC 日期>-DRL-60.md`(时间线、网页撤单清单、`restore` 输出与 `orphans.json`、看门狗日志);postmortem 模板的演练版
- 实现:[host-upgrade 第 11 步](../runbooks/host-upgrade.md#步骤)(人工,`tests/host/DRL-60.sh` 只读核对 restore 与对象键)

### DRL-61 dev2 收尾:sim 不再自启,看门狗与验证包留在 dev2;卸载时旧系统回到基线

- 规则:[host-upgrade 第 12–15 步](../runbooks/host-upgrade.md#步骤)、[host-upgrade 验收](../runbooks/host-upgrade.md#验收)
- 级别:host
- 里程碑:M7
- 环境:dev2
- 授权:`[需授权]`
- 步骤:按 [host-upgrade 第 12–15 步](../runbooks/host-upgrade.md#步骤)
- 通过标准:
  - `systemctl is-enabled hft-core@sim` = disabled;`hft-offbox-watchdog` 与 `hft-verify.timer` active;`hft-window start` 仍能启动测试网实例(下一次联调窗口时核对)
  - 主机清单、runbook 中的"当前主机"、ops_bot 与 watchtower 的实例标签都指向新主机(PR 链接)
  - 删除 dev2 地址的白名单变更逐 key ID 记录
  - 只在正式实盘开始之前或不再需要看门狗与验证包时卸载:`releasectl uninstall` 后 `systemctl list-units 'hft-*'` 为空,旧系统的进程未重启,`system.slice` 的 PSI 与 `MemAvailable` 的周均值回到 hft 安装前一周的基线 ± 10%
- 证据:`ops-log/<UTC 日期>-DRL-61.md`
- 实现:`tests/host/DRL-61.sh`

## 5. 主机调优(host-tuning)

### DRL-62 内核启动参数与大页在重启后逐项等于发布内生成物

- 规则:[host-tuning B](../runbooks/host-tuning.md#b-内核启动参数)、[deploy GRUB](../../deploy/README.md#grub)
- 级别:host
- 里程碑:M7
- 环境:`c4`/`c8`/`c16`/`c32`/`metal` 主机类的新主机或新 AMI(dev2 不适用)
- 授权:`[需授权]`(`update-grub` 与重启)
- 步骤:按 [host-tuning 第 4–6 步](../runbooks/host-tuning.md#b-内核启动参数)
- 通过标准:
  - `/etc/default/grub.d/50-hft.cfg` 与 `/opt/hft/current-core/deploy/grub/` 中的文件 `diff` 为空
  - `/proc/cmdline` 含 deploy GRUB 节列出的每一项;`irqaffinity` = hk 核与 IRQ 专用核;`rcu_nocbs` 含全部隔离核与 IRQ 专用核;aarch64 上 `transparent_hugepage=madvise`
  - `/sys/devices/system/cpu/isolated` 等于生成的隔离集合;`nohz_full` 只有 sequencer 核;x86 `metal` 上 `smt/active` = 0
  - `HugePages_Total` 与清单一致;hft-core 未运行时 1 GiB 与 2 MiB 页的 `free_hugepages` 等于清单数量;`/dev/hugepages`(pagesize=1G)与 `/dev/hugepages2M` 已挂载
  - 重启前执行了 `hft-maint start 30`,其间看门狗只发 Telegram
- 证据:`ops-log/<UTC 日期>-DRL-62.md`
- 实现:`tests/host/DRL-62.sh`

### DRL-63 运行期调优逐项相符,`hostprobe verify` 退出 0 且 `pending_reboot = false`

- 规则:[host-tuning C](../runbooks/host-tuning.md#c-运行期调优)、[deploy 运行期调优](../../deploy/README.md#运行期调优)、[deploy HostProfile](../../deploy/README.md#hostprofile)
- 级别:host
- 里程碑:M7
- 环境:同 DRL-62
- 授权:无
- 步骤:按 [host-tuning 第 7–15 步](../runbooks/host-tuning.md#c-运行期调优)
- 通过标准:
  - `hft-host-tune`、`hft-eni-attach`、`hft-net-tune`、`hft-resctrl` 都是 `active (exited)`、退出码 0
  - ENI-A 的 IRQ 只在核 0;ENI-B 全部队列只在 IRQ 专用核且该核上没有线程(`ps -eL -o psr,comm`);ENI-C、ENI-B2 的队列只在各自的核;NVMe managed IRQ 的 `effective_affinity_list` 不含隔离核与 IRQ 专用核;`irqbalance` inactive
  - workqueue cpumask 去重后只有 hk 掩码
  - 每块交易网卡:`ethtool -l` combined = 清单 `queues`;adaptive-rx off、rx-usecs 0、tx-usecs 0;gro/lro/tso/gso/sg off;rx 4096、tx 1024;`napi_defer_hard_irqs` = 0、`gro_flush_timeout` = 0;ENI-B 的 XPS 与 `irq-plan.json` 一致
  - sysctl 与 `90-hft.conf` 逐行一致,`net.core.busy_poll = 0`、`net.core.busy_read = 0`
  - chrony 的 Reference ID 为 PHC、`Leap status: Normal`;resctrl 与清单一致或清单写 `resctrl.enabled = false`
  - 系统杂项服务清单为空,`ec2-instance-connect` 未安装,`amazon-ssm-agent` 的亲和只含非隔离核且不含 IRQ 专用核;`hft-imds-lock` active、IMDS 程序挂在根 cgroup、`nf_conntrack` 未加载
  - `hostprobe verify --expected … --out /run/hft/hostprofile.json` 退出 0,`pending_reboot = false`
- 证据:`ops-log/<UTC 日期>-DRL-63.md`(各命令输出与 hostprobe JSON 的 SHA-256)
- 实现:`tests/host/DRL-63.sh`

### DRL-64 验收测量:定时器超调、网卡落点、到交易所的 RTT 与 PHC 误差界达到验收表

- 规则:[host-tuning D 与验收](../runbooks/host-tuning.md#验收)
- 级别:host
- 里程碑:M7
- 环境:同 DRL-62;hft-core 运行 24 h(prod 只含测试网账户,`sim` 同机)
- 授权:`[需授权]`(启动 hft-core)
- 步骤:按 [host-tuning 第 16–19 步](../runbooks/host-tuning.md#d-验收测量)
- 通过标准:
  - 隔离核 `osnoise` 24 h 由 TOOL-112 判定;2 个隔离核上 `timerlat` 1 h 的 p99 < 10 µs
  - `kernel_rx`、`nic_to_kernel`、`kernel_to_user` 的直方图已记录(不判定)
  - prod 的连接 100% 属于 ENI-B(启用时批量连接属于 ENI-B2),sim 的连接 100% 属于 ENI-C;停 sim 期间 ENI-C 的队列计数不再增长,IRQ 专用核的 `NET_RX` softirq 增速不变
  - 到 Binance 的 `minrtt` 比 az2 基线降 ≥ 0.5 ms;每连接 ping 直方图的 p99 抖动 < 100 µs
  - PHC 误差界 < 40 µs
- 证据:`ops-log/<UTC 日期>-DRL-64.md`;数值同时贴进 M7 延迟验收记录
- 实现:`tests/host/DRL-64.sh`

### DRL-65 调优改动一次一项,24 h 对比不劣化;劣化的项回退并写入 rejected Agent Note

- 规则:[host-tuning 验收](../runbooks/host-tuning.md#验收)、[host-tuning 回退](../runbooks/host-tuning.md#回退)、[host-tuning A](../runbooks/host-tuning.md#a-基线只在改动前执行只读)
- 级别:host
- 里程碑:M7
- 环境:升级主机,一次调优改动(deploy 下 GRUB、sysctl、网卡、IRQ、hugepages、resctrl、tuned、chrony 任一项)
- 授权:`[需授权]`
- 步骤:按 [host-tuning A](../runbooks/host-tuning.md#a-基线只在改动前执行只读) 记 24 h 基线 → 发布只含这一项改动的版本 → 24 h 后对比
- 通过标准:
  - 改动版本与上一版本之间 `deploy/` 的差异只涉及一个调优项
  - 改动后 24 h 的目标分段 p99 ≤ 基线 p99;声称改善时附 p50/p99/p99.9/max 与样本数的对比
  - 劣化时:该项已回退(回滚版本或重跑第 7–15 步),`.agents/notes/rejected/tuning/` 有一条记录劣化证据的 Agent Note
- 证据:`ops-log/<UTC 日期>-DRL-65.md`(两份直方图与差异)
- 实现:`tests/host/DRL-65.sh`

### DRL-66 季度复验与 AWS 维护或实例替换之后的复验

- 规则:[host-tuning 适用范围](../runbooks/host-tuning.md#适用范围)
- 级别:host
- 里程碑:M7
- 环境:升级主机;每季度一次,以及 AWS 维护或实例替换后
- 授权:无(需要重启时按 DRL-62)
- 步骤:重做 DRL-63 与 DRL-64
- 通过标准:
  - DRL-63、DRL-64 的通过标准全部满足
  - 相邻两次季度复验的间隔 ≤ 92 天;每次 AWS 维护或实例替换之后,hft-core 恢复运行之前有一次复验
- 证据:`ops-log/<UTC 日期>-DRL-66.md`
- 实现:`tests/host/DRL-66.sh`

### DRL-67 M3 末满订阅探针按正式版与探针清单安装,结束后撤销白名单并退租

- 规则:[host-tuning 适用范围](../runbooks/host-tuning.md#适用范围)、[deploy 主机清单](../../deploy/README.md#主机清单)
- 级别:host
- 里程碑:M3
- 环境:按需租用的 `c8g.4xlarge`(`probe-c16`)与 `c8g.8xlarge`(`probe-c32`),开在 az-probe 选定的 AZ
- 授权:`[需授权]`(租用、改白名单、退租)
- 步骤:按 [host-tuning 适用范围第 3 条](../runbooks/host-tuning.md#适用范围)
- 通过标准:
  - 安装时 `host_expect` 校验通过,B、C 节(DRL-62、DRL-63 的标准)满足;kill-switch、prod(测试网账户)与 sim 都启动并闸门放行
  - 结束后:所用测试网 key 的白名单中没有探针 EIP(逐 key 核对);两台实例已终止
  - 结论写进探针的 Agent Note(链接)
- 证据:`ops-log/<UTC 日期>-DRL-67.md`
- 实现:`tests/host/DRL-67.sh`

### DRL-68 M1.5 探针与参考机的租用实例只读核对并记录网卡与 IRQ 事实

- 规则:[host-tuning 适用范围](../runbooks/host-tuning.md#适用范围)、[development 参考机](../development.md#参考机)
- 级别:host
- 里程碑:M1.5
- 环境:按需租用的同型实例(不安装生产归档)
- 授权:`[需授权]`(租用)
- 步骤:按 [host-tuning C 节](../runbooks/host-tuning.md#c-运行期调优) 只读核对
- 通过标准:
  - 记录了 ENA 队列数、IRQ 布局、managed IRQ 落点、NAPI 参数、XPS 是否可写、HW RX 时间戳能否用,六项都有值
  - 实例上没有 `/opt/hft/current-core`(未安装生产归档)
  - 结论写进对应探针的 Agent Note(链接)
- 证据:`ops-log/<UTC 日期>-DRL-68.md`
- 实现:`tests/host/DRL-68.sh`

## 6. AZ 探测(az-probe)

### DRL-69 首次 AZ 探测给出胜出的 (AZ-ID, CPG)、第二 AZ 与第三 AZ 看门狗结论,探针清理完毕

- 规则:[az-probe 步骤 A–E](../runbooks/az-probe.md#d-判定)、[az-probe 验收](../runbooks/az-probe.md#验收)
- 级别:host
- 里程碑:M1.5
- 环境:apne1-az1、az2、az4 各 3–5 个 CPG 中的 Spot 探针
- 授权:`[需授权]`(建 CPG、开探针、删除)
- 步骤:按 [az-probe A–E](../runbooks/az-probe.md#步骤)(工具报告的覆盖由 TOOL-098 判定)
- 通过标准:
  - AZ 名到 AZ-ID 的映射已记录;三个 AZ 各 ≥ 3 个 CPG,每台探针 ≥ 1 h 且包含一个活跃时段
  - 报告给出:Binance 所在 AZ 的结论(存在约 0.3 ms 级节点的 AZ,或三 AZ 都没有及其解释);胜出 (AZ-ID, CPG)(core 交易所握手 minRTT 之和最小,差 < 20 µs 视为相同时按 optional 交易所决胜);第二 AZ ≠ 胜出 AZ;dev2 的 AZ-ID 与"需要/不需要第三 AZ 看门狗"
  - 探针实例全部终止、未使用的 CPG 已删除,只保留 1 台最小探针或写明按周临时启动
- 证据:`ops-log/<date>-az-probe.md`(文件名同时含 `DRL-69`)
- 实现:`tests/host/DRL-69.sh`

### DRL-70 新主机落在胜出 AZ 后到 Binance 的 minrtt 达到验收,否则留在原 AZ

- 规则:[az-probe E](../runbooks/az-probe.md#e-落地与清理)、[az-probe 验收](../runbooks/az-probe.md#验收)、[az-probe 回退](../runbooks/az-probe.md#回退)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机(胜出 CPG)
- 授权:`[需授权]`(Capacity Reservation、预建待命网卡)
- 步骤:按 [az-probe 第 10–13 步](../runbooks/az-probe.md#e-落地与清理)
- 通过标准:
  - 两份主清单写入胜出的 AZ-ID 与 CPG,两份待命清单写入第二 AZ;On-Demand Capacity Reservation 存在
  - 新主机到 Binance 的 `minrtt` 比 az2 现状降 ≥ 0.5 ms,p99 RTT 抖动 < 100 µs;不满足 → 记录改在原 AZ 开机的决定,现有系统未改动
- 证据:`ops-log/<UTC 日期>-DRL-70.md`
- 实现:`tests/host/DRL-70.sh`

### DRL-71 每周复测在生产 CPG 探针与生产机上运行,变化超阈值时按级别告警

- 规则:[az-probe 每周复测](../runbooks/az-probe.md#每周复测)
- 级别:host
- 里程碑:M7
- 环境:生产 CPG 的探针与生产主机(不在 dev2);周一 UTC 00:00–02:00 以外的活跃时段
- 授权:无
- 步骤:按 [az-probe 每周复测](../runbooks/az-probe.md#每周复测)(生产机只跑 probe2 与 `ss -ti`;资源占用由 TOOL-099 判定)
- 通过标准:
  - 结果追加到 `ops-log/az-probe-weekly.md`,相邻两次间隔 ≤ 7 天
  - 任一交易所最小 RTT 比上周变化 > 0.2 ms → 同一周内有 P2 与周报分析;Binance 同 AZ 节点消失 → 有 P1,并在同一周内启动 A–D 的完整探测
  - 运行记录中没有 dev2 的主机名
- 证据:`ops-log/az-probe-weekly.md` 中该周一节(标注 `DRL-71`)
- 实现:`tests/host/DRL-71.sh`

## 7. 发布与部署(release-and-deploy)

### DRL-72 改动交易所适配的版本先在 dev2 测试网冒烟,冒烟不过不在升级主机激活

- 规则:[release-and-deploy 前置条件第 5 条](../runbooks/release-and-deploy.md#前置条件)
- 级别:testnet
- 里程碑:M7
- 门槛:⑥
- 环境:dev2 联调窗口;待发布的同一个 `vX.Y.Z`(其 PR 列表改动了 `venue-*`、`hft-venue`、`hft-net` 或 `hft-oms`)
- 授权:`[需授权]`(dev2 安装与联调窗口)
- 前置:latency-probe 参数同 DRL-9;OMS `unknown_alarm_ms = 120000`
- 步骤:在 dev2 安装并激活该版本 → `hft-window start` → 对每个下单 venue 各做一次下单/撤单/成交、断连与重同步(DRL-10 的两步)、kill 与恢复(DRL-88、DRL-90)、交易所死手(有死手的 venue:挂一张远离市价的探测单后 `Drain{cancel_resting = false}`)
- 通过标准:
  - 每个下单 venue 的每一项都有 journal 证据(订单号与 `seq`);死手撤销时刻 − hft-core 退出时刻 ≤ 该所死手时限
  - 冒烟期间 `ignorable_user_events` 之外的未知用户流事件计数为 0
  - 冒烟不通过时,升级主机的 `/opt/hft/deployments/` 中没有该版本 `core` 组的激活记录
- 证据:`ops-log/<UTC 日期>-DRL-72.md`(版本号、PR 列表中命中的路径、各项结果)
- 实现:`tests/host/DRL-72.sh`

### DRL-73 正式版的获取与校验:CI 结论齐全、服务器直接拉取、`verify` 不影响运行中的系统

- 规则:[release-and-deploy A](../runbooks/release-and-deploy.md#a-构建与获取)、[deploy 发布归档与 releasectl](../../deploy/README.md#发布归档与-releasectl)
- 级别:host
- 里程碑:M3
- 环境:dev2(M7 起另在升级主机各做一次)
- 授权:无(安装与激活不在本用例)
- 步骤:按 [release-and-deploy 第 1–5 步](../runbooks/release-and-deploy.md#a-构建与获取)
- 通过标准:
  - 发布工作流的 `preflight` 通过;CI 每个 job 的结论为 success,没有被跳过的下游 job
  - Release 资产含 `hft-<v>-aarch64.tar.zst` 与 `.sha256`(正式版另有验证包与基准制品);`RELEASE.json.channel` 与预期一致
  - 归档由服务器上的 `fetch-release.sh` 拉取(服务器审计日志有该次下载)
  - `releasectl verify` 退出 0;其前后 `/opt/hft/current-*` 链接目标不变、运行中单元的 MainPID 不变
  - `preflight.sh` 退出 0(含主机上无编译工具链、不存在 `/opt/hft/current` 与 `/opt/hft/venv`;dev2 另含护栏各项)
- 证据:`ops-log/<UTC 日期>-DRL-73.md`
- 实现:`tests/host/DRL-73.sh`

### DRL-74 新主机首次安装引导:从暂存目录完成第一次安装与 `core` 组激活

- 规则:[release-and-deploy 第 3 步](../runbooks/release-and-deploy.md#a-构建与获取)、[deploy 单元组与分组激活](../../deploy/README.md#单元组与分组激活)
- 级别:host
- 里程碑:M3
- 环境:还没有 `/opt/hft/current-core` 的新主机(M3 末探针;M7 升级主机;换 AZ 演练的新实例)
- 授权:`[需授权]`
- 步骤:按 [release-and-deploy 第 3 步](../runbooks/release-and-deploy.md#a-构建与获取) 的"新主机首次安装"
- 通过标准:
  - `/etc/hft/host-local.toml` 的 `manifest` 在安装之前写好
  - 归档与 `.sha256` 在 `/opt/hft/staging/` 中,`sha256sum -c` 通过;用暂存目录中的 `releasectl.sh install` 与 `activate --group core` 都退出 0
  - 之后 `/opt/hft/current-core` 存在,`fetch-release.sh --version <v> --dry-run` 成功
- 证据:`ops-log/<UTC 日期>-DRL-74.md`
- 实现:`tests/host/DRL-74.sh`

### DRL-75 行为变更的版本先做离线回放与模拟先行,再激活 `core`

- 规则:[release-and-deploy B](../runbooks/release-and-deploy.md#b-行为变更的验证回放--模拟账户)、[replayer verify](../../apps/replayer/README.md#verify)
- 级别:host
- 里程碑:M4
- 环境:dev2(M7 起在升级主机);改动了策略、执行层、风控、OMS 或 sequencer 的版本
- 授权:无(激活按 DRL-76)
- 步骤:按 [release-and-deploy 第 6–8 步](../runbooks/release-and-deploy.md#b-行为变更的验证回放--模拟账户)
- 通过标准:
  - 新版本已 `install` 未激活;用 `/opt/hft/releases/<v>/bin/replayer verify --journal <冷副本>` 以 `systemd-run --slice=hftbatch.slice` 逐日校验最近 7 天,每天有结论,每条分歧在 PR 中有解释
  - dev2 上校验期间 `/var/lib/hft/guard/tripped` 出现时该次运行中止,资源空闲后续跑,中止与续跑都记录
  - 先只激活 `sim` 组:`current-sim → releases/<v>` 而 `current-core` 仍是旧版本;受影响策略的模拟账户以 Live 运行 ≥ 24 h,新旧版本的 `/sim` 报告差异有解释;之后才激活 `core`
- 证据:`ops-log/<UTC 日期>-DRL-75.md`
- 实现:`tests/host/DRL-75.sh`

### DRL-76 冷激活与第一分钟验收:组隔离、启动顺序与第 16 步逐项满足

- 规则:[release-and-deploy C、D](../runbooks/release-and-deploy.md#d-启动与第一分钟验收)、[hft-core 启动顺序](../../apps/hft-core/README.md#启动顺序)
- 级别:host
- 里程碑:M5
- 环境:dev2(`core` 组在联调窗口内;`cold` 与 `sim` 组);M7 起在升级主机重复
- 授权:`[需授权]`
- 前置:`shutdown.cancel_open_orders = true`
- 步骤:按 [release-and-deploy 第 9–17 步](../runbooks/release-and-deploy.md#c-激活)
- 通过标准:
  - 第 9 步的记录含每个 (实例, group, 账户) 的期望模式与有效模式
  - 第 10 步:journal 依次有撤单、Drain、快照,段已封存,`HeartbeatBlock.state = Stopped`,kill-switch 无撤单动作
  - 第 12 步:输出 `{"status":"activated_stopped", …}`;`current-<group>` 指向新版本、`previous-<group>` 指向旧版本;本组单元 inactive,其他组与 dev2 守护仍 active;`/run/hft/hostprofile.json` 的修改时刻晚于激活时刻
  - 第 15 步:`hft-imds-lock`(与装有它的主机上的 `hft-cred-broker`)先于冷进程 active,`hft-kill-switch` 先于 `hft-core@prod` active;冷进程按列出的顺序启动
  - 第 16 步每一项满足,`Note{StartupStep}` 总耗时 < 30 s
  - 第 17 步:15 min 内无 P0/P1;persist 滞后 p99 在 hft-journal 给本主机冷盘类型的界内
- 证据:`ops-log/<UTC 日期>-DRL-76.md`
- 实现:`tests/host/DRL-76.sh`

### DRL-77 升级主机激活 `cold` 组前开维护静默,`pending_reboot` 时重启后 hostprobe 通过

- 规则:[release-and-deploy 第 11–14 步](../runbooks/release-and-deploy.md#c-激活)、[deploy 机外看门狗](../../deploy/README.md#机外看门狗)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机
- 授权:`[需授权]`
- 步骤:`sudo hft-maint start 30` → 等看门狗读到 → `releasectl activate --version <v> --group cold` → 需要时重启 → `hft-maint stop`
- 通过标准:
  - 看门狗在激活开始之前读到 `maintenance_until`(看门狗日志时刻早于激活时刻);激活与重启期间只有 Telegram,没有电话
  - `pending_reboot = true` 时:重启后 `hft-hostprobe` 退出 0 才启动任何 hft 单元
  - hostprobe 有其他不符项时没有启动 hft 单元;全部配置中没有 `on_mismatch = degrade`
  - 结束后 `hft-maint status` 显示无静默
- 证据:`ops-log/<UTC 日期>-DRL-77.md`
- 实现:`tests/host/DRL-77.sh`

### DRL-78 发版后的冷启动不自动解除 `Recovery`,交易员逐账户解除后有效模式与记录一致

- 规则:[release-and-deploy 第 16 步](../runbooks/release-and-deploy.md#d-启动与第一分钟验收)、[hft-risk 干净 Drain 重启自动解除 Recovery](../../crates/hft-risk/README.md#干净-drain-重启自动解除-recovery)
- 级别:testnet
- 里程碑:M5
- 门槛:⑤
- 环境:dev2 联调窗口内的测试网 `prod`(版本变化的重启;sim 实例的自动解除规则不同,不在本用例)
- 授权:`[需授权]`(`/resume recovery` 需确认)
- 步骤:激活新版本并启动;闸门全 1 后观察 5 min;交易员对每个账户 `/resume recovery <a>`
- 通过标准:
  - 闸门全 1 后 5 min 内没有 `Recovery` 的 `Note{CapCleared}`
  - 每个 `/resume recovery <a>` 经确认后 `CmdResult` ok 并出现 `Note{CapCleared}`(Recovery)
  - 解除后每个 cell 的期望模式与有效模式等于第 9 步的记录
- 证据:`ops-log/<UTC 日期>-DRL-78.md`
- 实现:`tests/host/DRL-78.sh`

### DRL-79 部署记录写全字段,只保留被组链接引用的版本目录

- 规则:[release-and-deploy E](../runbooks/release-and-deploy.md#e-记录与清理)
- 级别:host
- 里程碑:M3
- 环境:dev2;M7 起升级主机
- 授权:`[需授权]`(删除版本目录)
- 步骤:按 [release-and-deploy 第 18–20 步](../runbooks/release-and-deploy.md#e-记录与清理)
- 通过标准:
  - `/opt/hft/deployments/<ts>-<v>.json` 存在,含 `channel`、`host_profile_hash`、`refdata_version`
  - `/opt/hft/releases/` 下每个目录都被某个 `current-<group>` 或 `previous-<group>` 指向;开发构建目录 ≤ 3 个
  - ops-log 记录了构建、获取、校验、验证、激活、启动、验收各步的起止时刻
- 证据:`ops-log/<UTC 日期>-DRL-79.md`
- 实现:`tests/host/DRL-79.sh`

### DRL-80 refdata 变更:`ReplaceRefdata` → Drain → 退出 75 → 立即重启 → 新版本生效、闸门全 1

- 规则:[release-and-deploy refdata 变更](../runbooks/release-and-deploy.md#refdata-变更)、[hft-core refdata 变更](../../apps/hft-core/README.md#refdata-变更drain--重启)
- 级别:testnet
- 里程碑:M5
- 环境:dev2 联调窗口内的测试网 `prod`(`instance.config_source = "journal"`);变更内容:新增一个测试网账户的一个 inst
- 授权:`[需授权]`(Admin 经运维 CLI,确认)
- 步骤:按 [release-and-deploy refdata 变更第 1–4 步](../runbooks/release-and-deploy.md#refdata-变更)
- 通过标准:
  - `hft-cfg` 差异报告中没有被删除的有持仓 inst;预览列出每个账户的挂单与 Live cell
  - `CmdResult{Applied}`;journal 中随后是 `ConfigBlob`、`Drain{cancel_resting = false}` 与快照;`systemctl show` 中该次退出状态为 75,`NRestarts` 加 1
  - 重启后 `ConfigBlob` 中 refdata `version` 为新值;冷启动 < 30 s;每个账户每个 venue 闸门全 1;同版本、尾部为空 → 闸门放行后 `Recovery` 自动解除
  - 有效模式与第 2 步的记录一致
- 证据:`ops-log/<UTC 日期>-DRL-80.md`(差异报告、预览截图、journal 摘录)
- 实现:`tests/host/DRL-80.sh`

### DRL-81 refdata 装配校验不通过时 gateway 直接拒绝,不进 Drain

- 规则:[release-and-deploy refdata 变更第 5 步](../runbooks/release-and-deploy.md#refdata-变更)、[hft-core refdata 变更](../../apps/hft-core/README.md#refdata-变更drain--重启)
- 级别:host
- 里程碑:M5
- 环境:dev2 `sim` 实例;新 blob 给某 Sim 账户加了一个 sim 实例线程计划未覆盖的 venue
- 授权:`[需授权]`
- 步骤:Admin 发 `ReplaceRefdata{blob, version}` 并确认
- 通过标准:
  - 运维 CLI 收到 `Rejected{RefdataAssembly}`,arg 指出失败项;journal 中没有该命令的 `CmdResult`、没有 `Draining` cap
  - `hft-core@sim` 的 MainPID 不变;生效的 refdata `version` 不变
  - 修正 blob 后重发按 DRL-80 生效
- 证据:`ops-log/<UTC 日期>-DRL-81.md`
- 实现:`tests/host/DRL-81.sh`

### DRL-82 refdata 的 sequencer 复核不通过时 `CmdResult{Rejected}`,不 Drain、继续运行

- 规则:[release-and-deploy refdata 变更第 5 步](../runbooks/release-and-deploy.md#refdata-变更)、[hft-core refdata 变更](../../apps/hft-core/README.md#refdata-变更drain--重启)
- 级别:host
- 里程碑:M5
- 环境:dev2 `sim` 实例;新 blob 删除了一个 Sim 账户持有仓位的 inst
- 授权:`[需授权]`
- 步骤:Admin 发 `ReplaceRefdata{blob, version}` 并确认
- 通过标准:
  - `CmdResult{Rejected, detail = RefdataRecheck}`;没有 `ConfigBlob`、没有 `Draining` cap
  - `hft-core@sim` 的 MainPID 不变;生效的 refdata `version` 不变;该 Sim 账户的仓位不变
- 证据:`ops-log/<UTC 日期>-DRL-82.md`
- 实现:`tests/host/DRL-82.sh`

### DRL-83 验证包安装不停任何单元,定时器运行后账本出现该版本的运行记录

- 规则:[release-and-deploy F](../runbooks/release-and-deploy.md#f-验证包)、[deploy 验证包与 hft-verify](../../deploy/README.md#验证包与-hft-verify)、[tools hft-verify](../../tools/README.md#hft-verify)
- 级别:host
- 里程碑:M3
- 环境:dev2;M7 期间另在升级主机
- 授权:`[需授权]`
- 步骤:按 [release-and-deploy 第 21、22 步](../runbooks/release-and-deploy.md#f-验证包)
- 通过标准:
  - `releasectl install-verify <v>` 退出 0;`/opt/hft/verify/current → /opt/hft/verify/<v>`;前后各单元 MainPID 与 `current-<group>` 链接不变
  - `systemctl list-timers hft-verify.timer` 显示下一次运行
  - 第一次运行结束后 `hft-verify ledger show` 中有版本 `<v>` 的运行记录,每个目标组有 `path_digest`
- 证据:`ops-log/<UTC 日期>-DRL-83.md`
- 实现:`tests/host/DRL-83.sh`

### DRL-84 拉取 token 轮换后每台服务器能拉取,旧 token 失效

- 规则:[release-and-deploy G](../runbooks/release-and-deploy.md#g-拉取-token-轮换)
- 级别:host
- 里程碑:M3
- 环境:全部服务器(dev2、升级主机)
- 授权:`[需授权]`(建与撤销 token)
- 步骤:按 [release-and-deploy 第 23–25 步](../runbooks/release-and-deploy.md#g-拉取-token-轮换)
- 通过标准:
  - 新 token 只授权仓库 `Fiora-gwx/hft` 的 Contents 读,到期日 ≤ 创建日 + 1 年,记入 ops-log
  - 每台服务器 `/etc/hft/github-release-token` 为 `0600 root:root`,`fetch-release.sh --version <当前版本> --dry-run` 成功
  - 撤销后用旧 token 的同一请求返回 401
  - 下一次轮换提醒设在新到期日之前 30 天
- 证据:`ops-log/<UTC 日期>-DRL-84.md`(不含 token 内容)
- 实现:`tests/host/DRL-84.sh`

### DRL-85 干净回滚:Drain 后以旧版本冷激活,第一分钟验收通过

- 规则:[release-and-deploy 回退](../runbooks/release-and-deploy.md#回退)
- 级别:host
- 里程碑:M5
- 门槛:⑤
- 环境:dev2(联调窗口内的测试网 `prod`)
- 授权:`[需授权]`
- 步骤:按 [release-and-deploy 回退第 1–4 步](../runbooks/release-and-deploy.md#回退)
- 通过标准:
  - 回滚前 journal 末尾是 Drain 快照;`releasectl rollback --version <previous> --group core` 退出 0,`current-core` 指向 `<previous>`
  - 旧版本从新版本写的 Drain 快照起步,第 16 步各项满足;`Recovery` 按 DRL-78 由人解除
  - 回滚的原因写进部署记录与 ops-log
- 证据:`ops-log/<UTC 日期>-DRL-85.md`
- 实现:`tests/host/DRL-85.sh`

### DRL-86 崩溃后回滚:旧版本只能带 `--recover-from-snapshot` 起步,按 `tail_lost` 处理后人工解除

- 规则:[release-and-deploy 回退](../runbooks/release-and-deploy.md#回退)、[incident-response D 第 3 步](../runbooks/incident-response.md#d-hft-core-崩溃重启风暴拒绝启动)、[hft-core 崩溃隔离与从快照恢复](../../apps/hft-core/README.md#崩溃隔离与从快照恢复)
- 级别:drill
- 里程碑:M5
- 门槛:⑤
- 环境:dev2 联调窗口内的测试网 `prod`;一个只用于演练的开发构建作为"新版本"
- 授权:`[需授权]`(负责人决定回滚)
- 步骤:
  1. 激活演练版本并启动,挂 2 张远离市价的探测单后 `kill -9` hft-core
  2. 按"崩溃后回滚"第 1–4 步;第 3 步之前先不带参数启动一次旧版本
- 通过标准:
  - 不带参数启动的旧版本拒绝从该尾部起步(非零退出,`journalctl` 给出版本变化的原因);热段与冷副本文件的 SHA-256 前后相同
  - 带 `--recover-from-snapshot latest --reason "<文本>"` 启动成功;新段带 `RecoverFromSnapshot` 引导块;全部非终态订单为 `Unknown`、每个账户带 `Recovery`、`RECONCILED` 严格,闸门放行后 `Recovery` 不自动解除(参数移除由 CORE-288 验证)
  - 快照之后的差异逐项以 `AdoptExchange` 或 `SetBalanceBaseline` 处置、`Unknown` 全部收敛后 `/resume recovery <a>` 成功
  - postmortem(演练版)已写
- 证据:`ops-log/<UTC 日期>-DRL-86.md`
- 实现:[release-and-deploy 回退](../runbooks/release-and-deploy.md#回退)(人工,`tests/host/DRL-86.sh` 只读核对文件哈希与 journal)

## 8. 事故响应(incident-response)

### DRL-87 通用首五分钟:ack、由小到大降级、只读核查与时间线

- 规则:[incident-response 通用首五分钟](../runbooks/incident-response.md#通用首五分钟)、[分级与响应时限](../runbooks/incident-response.md#分级与响应时限)
- 级别:drill
- 里程碑:M5
- 门槛:⑪
- 环境:dev2 联调窗口;由 DRL-10 的断连注入产生的一条真实告警;值班交易员
- 授权:无
- 步骤:按 [通用首五分钟](../runbooks/incident-response.md#通用首五分钟) 第 1–5 步;降级依次执行 `/mode <实例> <a> closeonly` → `/kill venue <v>` → `/kill account <a>`,随后逐级恢复
- 通过标准:
  - 告警后 5 min 内有 `/ack`;告警群中有"我在处理"
  - 三个降级命令都没有确认步骤,各自 `CmdResult` ok 并出现对应的 cap 或模式变化
  - `/status`、`/accounts`、`/p`、`/risk`、`/targets`、`/alerts 20` 都返回内容
  - ops-log 的时间线含告警时刻、ack 时刻、每个动作与其 `CmdResult`
- 证据:`ops-log/<UTC 日期>-DRL-87.md`
- 实现:[incident-response 通用首五分钟](../runbooks/incident-response.md#通用首五分钟)(人工)

### DRL-88 kill 演练:按账户 flatten 撤单并平仓,其他账户不受影响

- 规则:[incident-response kill 演练第 1、2 步](../runbooks/incident-response.md#kill-演练)、[hft-risk Kill](../../crates/hft-risk/README.md#kill)、[hft-risk 清仓锁](../../crates/hft-risk/README.md#清仓锁flatten_lock)
- 级别:testnet
- 里程碑:M5
- 门槛:⑪
- 环境:dev2 联调窗口(M7 在升级主机重做,见 DRL-25);演练账户 d 与另一个测试网账户 o
- 授权:无(Trader + 确认)
- 前置:d 有 2 张远离市价的小挂单与一笔最小名义的永续仓位;o 有 1 张挂单
- 步骤:按 [kill 演练第 1、2 步](../runbooks/incident-response.md#kill-演练):`/kill account d flatten`
- 通过标准:
  - `CmdResult` ok;journal 有 `Note{CapSet}`(Kill,作用域账户 d,`params[0]` 含 `FLATTEN`)
  - d 在交易所的挂单数为 0;d 的仓位在 10 min 内归零,期间 d 没有任何 Open 出环
  - o 的有效模式与挂单不变(`/accounts` 与交易所只读接口)
- 证据:`ops-log/<UTC 日期>-DRL-88.md`;[kill 演练记录](../runbooks/incident-response.md#kill-演练记录) 增加一行
- 实现:`tests/host/DRL-88.sh`

### DRL-89 kill 期间开仓意图被风控以 `KILLED` 拒绝,gw 的 kill 字为 1

- 规则:[incident-response kill 演练第 3 步](../runbooks/incident-response.md#kill-演练)、[hft-risk 检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[hft-risk kill 字](../../crates/hft-risk/README.md#kill-字唯一的跨线程共享可变量)
- 级别:testnet
- 里程碑:M5
- 门槛:⑪
- 环境:同 DRL-88,d 处于 Kill 中
- 授权:无
- 步骤:让 d 上的 latency-probe 产生一次开仓意图(`quote` 组到期)
- 通过标准:
  - 该意图被风控拒绝,`RejectBits` 含 `KILLED`;没有订单出环(交易所订单历史无新单)
  - `/status` 中 d 在每个 gw 的 kill 位为 1(gw 对绕过风控的单以 `KillBlocked` 拒绝,由 hft-venue 的用例验证)
- 证据:`ops-log/<UTC 日期>-DRL-89.md`
- 实现:`tests/host/DRL-89.sh`

### DRL-90 `/resume kill` 解除后 kill 字清零、期望模式不变、不产生补腿

- 规则:[incident-response kill 演练第 4 步](../runbooks/incident-response.md#kill-演练)、[hft-risk Kill](../../crates/hft-risk/README.md#kill)
- 级别:testnet
- 里程碑:M5
- 门槛:⑪
- 环境:同 DRL-88
- 授权:无(Trader + 确认)
- 步骤:`/resume kill d`
- 通过标准:
  - `CmdResult` 列出解除后仍生效的 cap,其中没有 Kill
  - journal 有 `Note{CapCleared}`(Kill);`/status` 中 d 在每个 gw 的 kill 位为 0;清仓锁随之结束
  - d 的期望模式与 kill 之前相同
  - 解除之后 60 s 内没有 d 的 HedgeFix 类 `ExecDecision`
- 证据:`ops-log/<UTC 日期>-DRL-90.md`
- 实现:`tests/host/DRL-90.sh`

### DRL-91 按账户 HARD:平仓单也被挡住,撤单与死手刷新照常,其他账户不受影响

- 规则:[incident-response kill 演练第 5 步](../runbooks/incident-response.md#kill-演练)、[incident-response L](../runbooks/incident-response.md#l-按账户-hard)
- 级别:testnet
- 里程碑:M5
- 门槛:⑪
- 环境:同 DRL-88
- 授权:无(Trader,私聊)
- 前置:同 DRL-88
- 步骤:重复 DRL-88 的准备,私聊发 `/kill account d hard`;再私聊发 `/kill account d flatten`;之后按 L 第 3 步解除
- 通过标准:
  - 群聊里发 `/kill account d hard` 得到"请私聊"的提示,没有命令发出
  - HARD 置位即 P0,告警列出 d 的未对冲量;d 的挂单被撤;之后 d 没有任何 Place/Amend 出环,含 flatten 产生的平仓单(交易所订单历史无新单)
  - `/status` 显示 d 的死手刷新仍在进行;o 的有效模式与挂单不变
  - 解除:新一轮对账满足 B 第 5 条后 `/resume kill d` 成功
- 证据:`ops-log/<UTC 日期>-DRL-91.md`;kill 演练记录增加一行
- 实现:`tests/host/DRL-91.sh`

### DRL-92 老板 `/stop`:预览后本人确认,停开仓、撤挂单、不平仓,确认不受中间状态变化影响

- 规则:[incident-response kill 演练第 6 步](../runbooks/incident-response.md#kill-演练)、[incident-response R](../runbooks/incident-response.md#r-老板紧急停止stop之后)、[ops_bot 紧急停止](../../python/ops_bot/README.md#紧急停止stop)
- 级别:drill
- 里程碑:M5
- 门槛:⑪
- 环境:dev2 联调窗口;老板(Viewer)、另一名 Trader;演练账户 d 有挂单与仓位
- 授权:无
- 步骤:老板 `/stop d`;预览之后另一名 Trader 对 d 发 `/mode … shadow`;老板在 60 s 内确认;交易员按 R 第 4 步恢复
- 通过标准:
  - 预览含"紧急停止"、范围 d 与"不会平仓"提示
  - 确认有效(中间的 `/mode` 不使其失效);10 s 内 `CmdResult` ok;`Kill{Account(d)}` 非 HARD、无 flatten,issuer 为 `tg:<老板>`
  - d 的挂单被撤、仓位不变;全部 Trader 收到推送(P1)
  - 交易员 `/resume kill d` 后有效模式恢复,没有残留的 Kill cap
- 证据:`ops-log/<UTC 日期>-DRL-92.md`;kill 演练记录增加一行
- 实现:[kill 演练第 6 步](../runbooks/incident-response.md#kill-演练)(人工)

### DRL-93 老板 `/stop` 的失败路径:预览过期、他人确认与越权命令都不执行

- 规则:[incident-response kill 演练第 6 步](../runbooks/incident-response.md#kill-演练)、[ops_bot 角色与鉴权](../../python/ops_bot/README.md#角色与鉴权)、[ops_bot 审计](../../python/ops_bot/README.md#审计)
- 级别:drill
- 里程碑:M5
- 门槛:⑪
- 环境:同 DRL-92
- 授权:无
- 步骤:
  1. 老板 `/stop d`,61 s 后确认
  2. 老板 `/stop d`,由一名 Trader 点老板预览的确认
  3. 老板依次发 `/resume kill d`、`/kill account d flatten`、`/kill account d hard`
- 通过标准:
  - 步骤 1:ops_bot 提示预览已过期、需重新 `/stop`;journal 中没有新的 Kill
  - 步骤 2:不执行,journal 中没有新的 Kill
  - 步骤 3:三条都被 ops_bot 本地拒绝,gateway 没有收到请求(journal 无对应 `ControlCmd`);ops_bot 审计表各有一行拒绝记录
- 证据:`ops-log/<UTC 日期>-DRL-93.md`
- 实现:[kill 演练第 6 步](../runbooks/incident-response.md#kill-演练)(人工)

### DRL-94 `stop_core` 兜底:ctl 不可达时撤单、写停机标记并停 hft-core,重启后带 Kill 起步、待补 Kill 先补发

- 规则:[incident-response kill 演练第 7 步](../runbooks/incident-response.md#kill-演练)、[incident-response R 第 6 步](../runbooks/incident-response.md#r-老板紧急停止stop之后)、[kill-switch 停止 hft-core](../../apps/kill-switch/README.md#停止-hft-corestop_core)、[hft-core 启动顺序](../../apps/hft-core/README.md#启动顺序)
- 级别:testnet
- 里程碑:M5
- 门槛:⑤、⑪
- 环境:dev2 联调窗口;演练账户 d 有 1–2 张远离市价的挂单,无未完成的链接与清仓
- 授权:`[需授权]`(之后启动 `hft-core@prod`)
- 前置:演练用 ops_bot 配置中的 ctl socket 路径指向不存在的文件
- 步骤:按 [kill 演练第 7 步](../runbooks/incident-response.md#kill-演练) 的主流程
- 通过标准:
  - 确认后约 10 s:`/var/lib/hft/ops_bot/pending_kill.json` 有 scope = d 的 `pending` 条目;kill-switch `actions.jsonl` 有 `stop_core`,调用方 uid = ops_bot 的服务用户
  - `/var/lib/hft/kill-switch/stop-marker.json` 写出,`scope` = d;d 的挂单在 kill-switch 核验中为 0;回执 `core_stop = Stopped`,prod journal 末尾是 Drain 快照;值班交易员收到 P0
  - 恢复 socket 路径后启动 `hft-core@prod`:journal 中有一条 issuer `cli:systemd`、非 HARD、无 flatten 的 `Kill{Account(d)}` Input;标记改名为 `stop-marker.json.consumed`
  - ops_bot 补发 Kill 之前 `/resume kill d` 被本地拒绝;补发后推送"待补 Kill 已生效"(P1),`pending_kill.json` 中该条目移除;之后交易员 `/resume kill d` 成功
- 证据:`ops-log/<UTC 日期>-DRL-94.md`;kill 演练记录增加一行
- 实现:`tests/host/DRL-94.sh`

### DRL-95 进程冻结时 `/stop` 只撤单并提示,老板再确认 `/stop … core` 后经 kill-switch 停机

- 规则:[incident-response kill 演练第 7 步](../runbooks/incident-response.md#kill-演练)、[incident-response R 第 8 步](../runbooks/incident-response.md#r-老板紧急停止stop之后)
- 级别:testnet
- 里程碑:M5
- 门槛:⑤、⑪
- 环境:同 DRL-94
- 授权:`[需授权]`
- 步骤:`kill -STOP <hft-core@prod 的 pid>`;老板 `/stop d` 并确认;收到 P0 后老板 `/stop d core` 并确认;之后按 DRL-94 恢复
- 通过标准:
  - 第一次 `/stop`:10 s 无结果且心跳过期 → ops_bot 只发 `cancel`(R2b),d 的挂单为 0,hft-core 进程仍存在;P0 文案提示 `/stop … core`
  - `/stop d core` 预览后本人确认 → `stop_core`;进程在 `TimeoutStopSec` 后被 systemd 终止,回执 `core_stop = Inactive`,停机标记保留(未改名为 `.aborted`)
  - 重启后与 DRL-94 的重启各项相同
- 证据:`ops-log/<UTC 日期>-DRL-95.md`;kill 演练记录增加一行(场景 SIGSTOP)
- 实现:`tests/host/DRL-95.sh`

### DRL-96 清仓未完成时 `/stop` 兜底只撤单,hft-core 不停

- 规则:[incident-response kill 演练第 7 步](../runbooks/incident-response.md#kill-演练)、[incident-response R 第 8 步](../runbooks/incident-response.md#r-老板紧急停止stop之后)、[ops_bot 紧急停止](../../python/ops_bot/README.md#紧急停止stop)
- 级别:testnet
- 里程碑:M5
- 门槛:⑪
- 环境:同 DRL-94;d 有一笔最小名义的永续仓位
- 授权:无
- 前置:先 `/kill account d flatten`,再私聊 `/kill account d hard`(HARD 挡住清仓单,清仓目标不能到达);之后让 ctl 不可达
- 步骤:老板 `/stop d` 并确认
- 通过标准:
  - ops_bot 只发 `cancel`(R2b),kill-switch 回执 d 的挂单为 0;`hft-core@prod` 仍 active
  - P0 文案列出原因"清仓未完成"并提示可用 `/stop … core`
  - 恢复 ctl 后按 L 第 3 步与 DRL-90 解除 HARD 与 Kill
- 证据:`ops-log/<UTC 日期>-DRL-96.md`
- 实现:`tests/host/DRL-96.sh`

### DRL-97 `stop_core` 停机失败时停机标记作废为 `.aborted` 并 P0,人工停止后启动不带 Kill

- 规则:[incident-response R 第 6 步](../runbooks/incident-response.md#r-老板紧急停止stop之后)、[kill-switch 停止 hft-core](../../apps/kill-switch/README.md#停止-hft-corestop_core)
- 级别:testnet
- 里程碑:M5
- 门槛:⑪
- 环境:同 DRL-94
- 授权:`[需授权]`(临时移走允许 `hft-ks` 停止 `hft-core@prod` 的 polkit 规则文件;演练后放回)
- 步骤:移走 polkit 规则 → 让 ctl 不可达 → 老板 `/stop d` 并确认 → 按 R 第 6.1 步处置:`systemctl stop hft-core@prod` → 放回规则 → 启动 → 立即对 d 发 Kill → 恢复
- 通过标准:
  - 回执 `core_stop = Failed`(systemctl 返回非 0 一律 `Failed`;`Denied` 待 dev2 核实 stderr 文本后才启用,规则归 kill-switch)、`Done.marker = false`;`/var/lib/hft/kill-switch/` 下有 `stop-marker.json.aborted`、没有 `stop-marker.json`;P0 详情含"停机标记已作废"
  - ops_bot 的 P0 文案含"hft-core 进程仍在运行"
  - 人工停止后再启动:journal 中没有 issuer `cli:systemd` 的 Kill;值班随后手动发的 Kill 生效
  - `kill-switch status` 显示已作废的标记
- 证据:`ops-log/<UTC 日期>-DRL-97.md`
- 实现:`tests/host/DRL-97.sh`

### DRL-98 kill 演练后审计记录完整、可逐条对应

- 规则:[incident-response kill 演练第 8 步](../runbooks/incident-response.md#kill-演练)、[ops_bot 审计](../../python/ops_bot/README.md#审计)
- 级别:drill
- 里程碑:M5
- 门槛:⑪
- 环境:DRL-88–DRL-97 执行后的 dev2
- 授权:无
- 步骤:导出 ops_bot 审计表、journal 中的 `ControlCmd`/`CmdResult`/审计 Note 与 kill-switch `actions.jsonl`
- 通过标准:
  - 每条到达 gateway 的命令在 ops_bot 审计表中恰好一行,在 journal 中有 `ControlCmd` 与 `CmdResult`;审计 Note 带 `view_account` 与 `chat_type`
  - 每次本地触发(`cancel`、`stop_core`、被拒的连接)在 `actions.jsonl` 中恰好一行
  - 本地拒绝的命令只在 ops_bot 审计表中出现
- 证据:`ops-log/<UTC 日期>-DRL-98.md`(对应表)
- 实现:`tests/host/DRL-98.sh`

### DRL-99 正式账户每月零成交冒烟:CloseOnly 下冒烟单经 kill-switch 撤掉,不做 SIGSTOP、flatten 与 HARD

- 规则:[incident-response kill 演练](../runbooks/incident-response.md#kill-演练)、[launch D 第 30 步](../runbooks/launch.md#d-正式实盘m8)
- 级别:drill
- 里程碑:M8
- 环境:升级主机正式 `prod`,维护窗口内;每月一次
- 授权:`[需授权]`
- 前置:`smoke_far_bp = 300 bp`;第 1 小步的冒烟单 `ttl_ms = 10000`,第 3 小步的 `ttl_ms = 60000`
- 步骤:该实例 `/mode … closeonly` → 按 launch D 第 30 步第 1、3、4 小步 → 恢复原模式;`kill-switch selftest` 与 `kill-switch dry-run`
- 通过标准:
  - 每个 (正式账户, venue) 满足 DRL-38 与 DRL-40 的通过标准;之后对账连续两轮 `Ignore`
  - `kill-switch selftest` 退出 0;`dry-run` 列出的挂单与交易所一致
  - 窗口内 journal 中没有带 HARD 或 flatten 的 Kill,hft-core 没有重启(MainPID 不变)
  - 结束后各 cell 的期望模式恢复为窗口前的值
- 证据:`ops-log/<UTC 日期>-DRL-99.md`;kill 演练记录增加一行(场景:正式账户零成交冒烟)
- 实现:`tests/host/DRL-99.sh`

### DRL-100 kill 演练按月执行:正式实盘后每月有一次测试网完整演练与一次正式账户冒烟

- 规则:[incident-response kill 演练](../runbooks/incident-response.md#kill-演练)
- 级别:drill
- 里程碑:M8
- 环境:kill 演练记录表
- 授权:无
- 步骤:每月核对记录表
- 通过标准:
  - 记录表中相邻两次测试网完整演练(dev2 联调窗口,同一发布版本)的日期间隔 ≤ 31 天,相邻两次正式账户冒烟(DRL-99)的间隔 ≤ 31 天
  - 每一行的各列(主机/实例/账户、场景、T0、各所挂单清零时间、结论、执行人)都已填写
- 证据:[kill 演练记录](../runbooks/incident-response.md#kill-演练记录);`ops-log/<UTC 日期>-DRL-100.md`
- 实现:`tests/host/DRL-100.sh`

### DRL-101 控制面不可用时人工 `kill-switch fire` 撤单,再 `systemctl stop` 停止仍在下单的 hft-core

- 规则:[incident-response G](../runbooks/incident-response.md#g-控制面不可用需要撤单但命令发不进去)、[kill-switch 撤单动作](../../apps/kill-switch/README.md#撤单动作)
- 级别:testnet
- 里程碑:M5
- 门槛:⑪
- 环境:dev2 联调窗口;演练账户 d 上 latency-probe `quote` 组运行
- 授权:`[需授权]`
- 前置:`shutdown.cancel_open_orders = true`;运维 CLI 的 ctl socket 路径指向不存在的文件
- 步骤:按 [G 第 1、3 步](../runbooks/incident-response.md#g-控制面不可用需要撤单但命令发不进去):`kill-switch fire --account d --venue all --reason "G 演练" --yes` → 观察 30 s → `systemctl stop hft-core@prod`
- 通过标准:
  - `kill-switch fire` 的输出中 d 每个 venue 撤单后为 0
  - 观察期内 hft-core 仍可能发出新的探测单(记录条数);`systemctl stop` 后 journal 末尾是 Drain 快照,d 在交易所的挂单为 0
- 证据:`ops-log/<UTC 日期>-DRL-101.md`
- 实现:`tests/host/DRL-101.sh`

### DRL-102 hft-core 被 `kill -9` 后死手与 kill-switch 撤单,重启后由证据收敛、人工解除 Recovery

- 规则:[incident-response D 第 5 步](../runbooks/incident-response.md#d-hft-core-崩溃重启风暴拒绝启动)、[incident-response C](../runbooks/incident-response.md#c-unknown-订单unknownage)、[kill-switch 判活与触发](../../apps/kill-switch/README.md#判活与触发)
- 级别:testnet
- 里程碑:M5
- 门槛:⑤
- 环境:dev2 联调窗口;演练账户 d 在有死手与无死手的 venue 上各有挂单
- 授权:`[需授权]`
- 步骤:`kill -9 <hft-core@prod 的 pid>`;等待 systemd 重启;按 D 第 5 步与 C 核对;对账后 `/resume recovery d`
- 通过标准:
  - 有死手的 venue:挂单在该所死手时限内被撤(交易所订单历史)
  - kill-switch 判活 `Fire`,对 d 的每个 venue 撤单,`actions.jsonl` 有记录,核验为 0
  - systemd 自动重启 hft-core;重启后没有 `Note{JournalTailLost}`;重启前的非终态槽在恢复后由查单或对账收敛,没有人工改状态
  - `Recovery` 在闸门放行后不自动解除;`/resume recovery d` 成功
  - 崩溃前的 journal 片段 `replayer verify --journal <dir>` 通过
- 证据:`ops-log/<UTC 日期>-DRL-102.md`;kill 演练记录增加一行(场景 SIGKILL)
- 实现:`tests/host/DRL-102.sh`

### DRL-103 sequencer 停摆:进程被冻结后 systemd watchdog 结束并重启它,kill-switch 撤单

- 规则:[incident-response E](../runbooks/incident-response.md#e-sequencer-停摆sequencerstall)、[hft-core 命令行与退出码](../../apps/hft-core/README.md#命令行与退出码)
- 级别:testnet
- 里程碑:M5
- 门槛:⑤
- 环境:dev2 联调窗口;演练账户 d 有挂单
- 授权:`[需授权]`
- 前置:dev2 的 `hft-core@prod` 单元 `WatchdogSec = 10s`
- 步骤:`kill -STOP <hft-core@prod 的 pid>`,不做其他操作
- 通过标准:
  - 告警群收到 `SequencerStall`;kill-switch 判活 `Fire` 并撤掉 d 的挂单(核验为 0)
  - `journalctl` 记录该单元的 watchdog 超时,进程被结束后自动重启;重启后各项同 DRL-102
- 证据:`ops-log/<UTC 日期>-DRL-103.md`;kill 演练记录增加一行(场景 SIGSTOP)
- 实现:`tests/host/DRL-103.sh`

### DRL-104 配置错误使 hft-core 以退出码 2 拒绝启动,回退配置后 `--check` 通过再启动

- 规则:[incident-response D 第 1 步](../runbooks/incident-response.md#d-hft-core-崩溃重启风暴拒绝启动)、[hft-core 命令行与退出码](../../apps/hft-core/README.md#命令行与退出码)
- 级别:host
- 里程碑:M5
- 环境:dev2 `sim` 实例
- 授权:`[需授权]`
- 步骤:在 `/etc/hft/hft-core-sim.toml` 加一个不存在的键 → `systemctl restart hft-core@sim` → 按 D 第 1 步:回退文件 → `hft-core --config /etc/hft/hft-core-sim.toml --check` → 启动
- 通过标准:
  - 重启后单元的退出状态为 2,`NRestarts` 不再增加(systemd 不重启);`journalctl` 打印该键名
  - 对错误文件 `--check` 同样退出 2 并打印同一行;回退后 `--check` 退出 0
  - 回退后启动成功,`Note{StartupStep}` 完成
- 证据:`ops-log/<UTC 日期>-DRL-104.md`
- 实现:`tests/host/DRL-104.sh`

### DRL-105 人工网页成交被识别为 `ExternalFill{Manual}`,置 `manual_hold`,策略接手后解除

- 规则:[incident-response N 第 3 步](../runbooks/incident-response.md#n-外部成交强平adl人工交割externalfill)、[hft-exec 人工保持与恢复期抑制](../../crates/hft-exec/README.md#人工保持与恢复期抑制)、[latency-probe 事件处理](../../strategies/latency-probe/README.md#事件处理)
- 级别:testnet
- 里程碑:M5
- 环境:dev2 联调窗口;latency-probe 账户 d 的 `pair` 组 g 处于 `Idle`(两腿净量为 0)
- 授权:`[需授权]`(在交易所网页下单)
- 步骤:按 [B2 第 1、2 步](../runbooks/incident-response.md#b2-人工在交易所网页操作之后adoptexchange):`/note` 写明 → 在交易所网页对 g 的 A 腿 inst 下一笔最小数量的市价单
- 通过标准:
  - journal 中出现该成交的 `ExternalFill`(cause = Manual)、`Note{ExecManualExternal}`(P1)与 `Note{ExecManualHold}`(置位,P1);告警为 P1,没有 P0
  - latency-probe 对该腿发 IOC 平仓后出现 `Note{ExecManualHold}`(解除,原因 Strategy);g 两腿净量回到 0
  - 下一轮对账 d 全部 `Ignore`;ops-log 中 `/note` 的时刻早于网页成交
- 证据:`ops-log/<UTC 日期>-DRL-105.md`(网页订单号、journal 摘录)
- 实现:`tests/host/DRL-105.sh`

### DRL-106 用户流断开期间的人工网页加仓:交易所补推则正常入账,不补推则以 `AdoptExchange` 收敛

- 规则:[incident-response B](../runbooks/incident-response.md#b-对账偏差reconcilemismatch)、[incident-response B2](../runbooks/incident-response.md#b2-人工在交易所网页操作之后adoptexchange)、[incident-response N 第 4 步](../runbooks/incident-response.md#n-外部成交强平adl人工交割externalfill)
- 级别:testnet
- 里程碑:M5
- 环境:dev2 联调窗口;latency-probe 账户 d 的永续 venue v,d 在 v 上无仓位
- 授权:`[需授权]`(临时 nft 规则只匹配 hft-core 服务用户;在交易所网页下单)
- 步骤:丢弃 hft-core 到 v 用户流端点的报文 → 网页对 v 的永续 inst 开一笔最小数量的仓位 → 60 s 后删除规则 → 等两轮对账 → 需要时按 B2 第 3–5 步处置
- 通过标准:满足以下二者之一,结论写进 venue README 的开发备忘(断流期间是否补推)
  - 交易所在用户流恢复后补推了该成交:journal 有该成交的 `ExternalFill`(cause = Manual),之后对账全部 `Ignore`,没有 `Recon` cap
  - 没有补推:连续两轮对账出现 v 的仓位差,`Recon` cap 生效、告警 P0;journal 中没有带 `AUTO_RECON` 的自动入账(加仓不满足自动入账条件);`/adopt d v <inst> <report_id> <entry_px>` 的预览中引擎值、交易所值与调整量与网页成交一致,确认后连续两轮 `Ignore`,`/resume recon d v` 成功
- 证据:`ops-log/<UTC 日期>-DRL-106.md`
- 实现:`tests/host/DRL-106.sh`

### DRL-107 冷盘可用空间低于禁开门限:`Storage` cap 只挡开仓,空间恢复后自动解除,不进 Drain

- 规则:[incident-response H](../runbooks/incident-response.md#h-存储退化persist-滞后磁盘热段内存)、[hft-risk 存储降级](../../crates/hft-risk/README.md#存储降级storage)
- 级别:host
- 里程碑:M7
- 门槛:⑤
- 环境:统一升级主机,只含测试网账户的 `prod`;⑩ 窗口之外;latency-probe `quote` 组运行
- 授权:`[需授权]`(在冷副本卷上建占位文件)
- 前置:`storage.disk_free_block_pct = 15`、`storage.disk_free_drain_pct = 5`
- 步骤:在冷副本卷上用 `fallocate` 建占位文件,使可用空间降到 10%;保持 10 min;删除占位文件
- 通过标准:
  - 出现 `Timer{Sys, DiskLow}`(档位 1)与 `Note{CapSet}`(`Storage`);告警 `StorageDegraded`
  - 保持期间交易所订单历史中没有新的探测单;没有 `Draining` cap,hft-core 一直运行
  - 删除后出现 `Timer{Sys, StorageOk}` 与 `Note{CapCleared}`(`Storage`),探测单恢复
  - 全程没有手工删除任何 journal 文件
- 证据:`ops-log/<UTC 日期>-DRL-107.md`
- 实现:`tests/host/DRL-107.sh`

### DRL-108 测试网 key 泄露演练:禁用 key、HARD、轮换后每个进程的 UID 核对通过

- 规则:[incident-response K](../runbooks/incident-response.md#k-秘密泄露或可疑访问)、[tools hft-secrets](../../tools/README.md#hft-secrets)
- 级别:testnet
- 里程碑:M5
- 环境:dev2 联调窗口;演练账户 d 的一把交易 key
- 授权:`[需授权]`(禁用与新建 key、写 Secrets Manager)
- 步骤:按 [K 第 1–4 步](../runbooks/incident-response.md#k-秘密泄露或可疑访问)
- 通过标准:
  - 交易所后台禁用该 key 后,用它的签名只读请求被拒;`/kill account d hard` 后 d 没有任何 Place/Amend 出环
  - 新 key 的权限与白名单与 DRL-2 的标准相同;Secrets Manager 中旧值已替换
  - 重启使用该秘密的单元后:d 的 `IDENTITY` 通过、`kill-switch selftest` 退出 0、reconciler 启动日志 UID 核对通过
  - 按新旧 key 的前 8 个字符检索 journal 与日志,命中 0 次
- 证据:`ops-log/<UTC 日期>-DRL-108.md`(只记 key ID)
- 实现:`tests/host/DRL-108.sh`

### DRL-109 下架公告设 `Delisting` cap:该 inst 只平不开,撤回后解除

- 规则:[incident-response O](../runbooks/incident-response.md#o-交易所公告下架资金费结算周期变化维护)、[hft-risk 下架与公告](../../crates/hft-risk/README.md#下架与公告venuenotice)
- 级别:drill
- 里程碑:M5
- 环境:dev2 `sim` 实例;Sim 账户在 inst x 上有一张开仓挂单与一笔仓位
- 授权:无(`Resume{Delisting}` 需确认)
- 步骤:Trader 经 `/notice` 发 `VenueNotice{Delisting{at}}`(x);观察 5 min;`/resume delisting` 撤回
- 通过标准:
  - 回执列出受影响的账户、group 与持仓;出现 `Note{CapSet}`(`Delisting`,作用域 venue + x),无确认步骤
  - x 上的开仓挂单被撤;之后 x 的 Open 被拒(`CLOSE_ONLY`),减仓照常
  - 下一轮 pipeline 的产出把该资产标为只平不开
  - 撤回经确认后出现 `Note{CapCleared}`(`Delisting`),x 恢复可开仓
- 证据:`ops-log/<UTC 日期>-DRL-109.md`
- 实现:`tests/host/DRL-109.sh`

## 9. CI 应急出包(development)

### DRL-110 arm runner 不可用路径的季度演练:交叉编译的开发构建在 dev2 `sim` 组通过第一分钟验收

- 规则:[development 应急交叉编译](../development.md#应急交叉编译release-x86-crossyml)
- 级别:host
- 里程碑:M4
- 环境:GitHub `release-x86-cross.yml`;dev2 `sim` 组
- 授权:`[需授权]`(负责人)
- 步骤:对 `main` 上一个已有标签的提交运行 `workflow_dispatch(sha, version, dev_n)` → dev2 安装到 `sim` 组 → 第一分钟验收;在同一工作流中试一次 `cargo fuzz build --target aarch64-unknown-linux-gnu`
- 通过标准:
  - 产出 `channel = dev` 的归档与 `.sha256`;`RELEASE.json.ci_run_id` 指向本次 x86 运行,发布说明注明交叉编译
  - dev2 上 `releasectl verify` 与 `hft-core --config /etc/hft/hft-core-sim.toml --check` 都退出 0;`sim` 组激活后 DRL-76 第 16 步的 sim 相关各项满足
  - `cargo fuzz build` 交叉编译的成败与原因记入 ops-log,并把结论写回 development 应急交叉编译一节
  - 相邻两次演练间隔 ≤ 92 天
- 证据:`ops-log/<UTC 日期>-DRL-110.md`(工作流运行链接)
- 实现:`tests/host/DRL-110.sh`

### DRL-111 额度耗尽路径的季度演练:镜像仓库出的开发构建发布到组织仓库并在 dev2 通过验收

已删除:负责人决定 hft 改放个人账户的私有仓库 `Fiora-gwx/hft`(2026-10-07),原"镜像到个人账户出包"的应急路径不再成立——额度与 Trading 共享,耗尽后到月初前任何仓库都无法在 CI 出包,没有可演练的替代路径;预防靠 quota-gate 与每周对账,紧急时由负责人决定是否临时设置付费上限([development 免费额度](../development.md#免费额度))。

## 10. 通用

### DRL-112 每页 runbook 在首次用于正式账户之前已用测试网或模拟账户走过一遍

- 规则:[runbooks 测试与验收](../runbooks/README.md#测试与验收)
- 级别:drill
- 里程碑:M8
- 环境:全部 `scope: core` 的 runbook 页
- 授权:无
- 步骤:launch D 第 32 步核对表中按页列出走查记录
- 通过标准:
  - release-and-deploy、incident-response、host-upgrade、host-tuning 各页在本页的用例都有早于 launch D 第 30 步的通过记录(dev2 或 M7 升级主机上,测试网或模拟账户;条件用例记"不适用"的附依据)
  - launch 的 A–C 用例早于 D 第 21 步通过;D 的步骤以本页 DRL-30–DRL-48 的记录为准
  - az-probe 不适用(它不在 dev2 上运行,首次执行即在探针上)
- 证据:上线记录中的走查表;`ops-log/<UTC 日期>-DRL-112.md` 链接它
- 实现:[launch D 第 32 步](../runbooks/launch.md#d-正式实盘m8)(人工)

## 11. 验收

- 每个里程碑结束时,`里程碑` ≤ 该里程碑的用例都有按 `证据` 归档的通过记录;条件型用例(DRL-6、DRL-28 等)的"不适用"附依据。
- launch D 第 33 步之前:带 `门槛` 字段的用例全部有通过记录,并在 DRL-44 的核对表中按门槛编号列出。
- 切 Live 之后:DRL-46–DRL-48、DRL-99、DRL-100 按各自频率持续有记录;任一不通过按 incident-response 处理并在同一天修正 runbook 或本页。
