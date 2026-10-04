---
description: "发布与部署:从合入到 tag、CI 构建、服务器拉取、校验、冷安装与分组激活(core/cold/sim;没有 /opt/hft/current)、新主机的首次安装引导、维护静默、模拟账户验证、改动交易所适配的版本先在 dev2 测试网冒烟、refdata 变更、授权启动与第一分钟验收、回滚(含崩溃后回滚)、验证包安装、拉取 token 轮换;开发构建在 dev2 上的联调安装"
scope: core
---

# 发布与部署

## 适用范围

- hft 仓库的代码、协议、单元文件、调优与 dev2 护栏文件的任何变更进入服务器。策略参数、名单、模式**不走本页**(控制命令,不触发构建)。
- 纪律沿用 Trading AGENTS.md(本仓库之外:`Trading/AGENTS.md`):不在服务器编译、不可变归档 + SHA-256 + `RELEASE.json`、冷生产、保留当前与恰好一个已验证回滚版本、不移动已发布 tag。归档格式、两类制品(正式版与开发构建)与脚本见 [deploy/README](../../deploy/README.md#发布归档与-releasectl);仓库与 CI 见 [development](../development.md#仓库与-ci)。
- **开发构建**(`vX.Y.Z-dev.N` 的 pre-release,由 CI 全绿的 PR 头提交经 `workflow_dispatch` 产出,或在 `main` 提交上打标签,[development](../development.md#日常流程))只装在 `dev2`,只用于 `sim` 实例与测试网联调用的 `prod` 角色实例;步骤与正式版相同,跳过 B 节。
- **验证包**(`hft-verify-<ver>-aarch64.tar.zst`,随正式版发布)单独安装,不属于任何单元组,不影响生产版本的激活(F 节)。指向正式端点的 Live 账户会拒绝启动开发构建;dev2 上本来也没有正式账户([deploy 秘密与实例角色](../../deploy/README.md#秘密与实例角色))。
- 每台主机上有两个 hft-core 实例:`prod`(`hft-core@prod.service`,`core` 组)与只含模拟账户的 `sim`(`hft-core@sim.service`,`sim` 组,[deploy 模拟实例](../../deploy/README.md#模拟实例sim))。dev2 上 `prod` 只在测试网联调窗口运行;升级主机上两者同机常驻。两组各自激活、各自回滚。

## 前置条件

1. 变更已经 PR 合入 `main`,CI 在合入后的 `main` 提交上通过一次;不因打 tag 再跑一遍普通套件。
2. 若改动涉及 sequencer/策略/风控/OMS/执行层:`replayer verify` 对最近 7 天生产 journal 在**当前生产版本**上一致(证据是 `hft-replay-verify` 每日写的 `/var/lib/hft/replay-verify/<instance>/<date>.json`,[deploy](../../deploy/README.md#systemd-单元));新版本对同一 journal 的分歧均为预期(PR 中逐条说明)。
3. 预估维护窗口:hft-core 冷启动 < 30 s,但激活是冷的(会停本组应用单元);避开资金费结算前后 ±5 min 与高波动时段。
4. `dev2`:`hft-dev2-guard` active,旧系统正式账户状态正常(`systemctl is-active hft-dev2-guard`;守护最近无触发)。
5. **改动了交易所适配的版本**(`venue-*`、`hft-venue`、`hft-net`、`hft-oms` 任一有改动,看 PR 列表):在升级主机上激活 `core` 组之前,先把**同一个** `vX.Y.Z` 装到 dev2,开一次测试网联调窗口(`hft-window start`)冒烟:每个下单 venue 下单/撤单/成交、断连与重同步、kill 与死手各一次,用户流解码无未知事件突增;结果写进 ops-log。冒烟不过 → 不在升级主机上激活该版本。核对:DRL-72。

## 步骤

### A. 构建与获取

1. 打 tag `v<X.Y.Z>`(开发构建:打 `v<X.Y.Z>-dev.<N>`,或对 PR 头提交运行 `workflow_dispatch(sha, dev_n)`);发布工作流的 `preflight` 只核验该 SHA 的 `ci.yml` 全绿与 `decoupling-gate` 通过(正式版另要求 SHA 在 `main` 上、最近一次每夜通过),不重跑质量门,然后在 `ubuntu-24.04-arm` 上构建([development 发布工作流](../development.md#发布工作流release-aarch64yml))。`preflight` 失败 → 先让 CI 跑绿,不绕过。
2. 核对 CI 每个 job 的结论(被跳过的上游 job 会让下游静默跳过);确认 release 资产包含 `hft-<v>-aarch64.tar.zst` 与 `.sha256`(正式版另有验证包与基准制品),`RELEASE.json.channel` 与预期一致。
3. 在服务器上拉取(不经操作员电脑中转):`sudo /opt/hft/current-core/deploy/release/fetch-release.sh --version <v> [--expected-sha256 <hash>]`(开发构建用 `--prerelease <v>`)。没有 `/opt/hft/current`:发布工具与主机级脚本一律经 `core` 组的链接 `current-core` 调用([deploy 单元组](../../deploy/README.md#单元组与分组激活))。**新主机首次安装**(还没有 `current-core`):`[需授权]` 用 `/etc/hft/github-release-token` 把归档与 `.sha256` 手工下载到 `/opt/hft/staging/`,`sha256sum -c` 通过后解包到暂存目录,用其中的 `deploy/release/releasectl.sh install` 与 `activate --group core` 完成第一次安装;此前写好 `/etc/hft/host-local.toml` 的 `manifest`(DRL-74)。
4. `sudo releasectl verify --archive /tmp/hft-<v>-aarch64.tar.zst --sha256-file …`(在临时根目录走完整安装路径,不影响运行中的系统;应急工作流 `release-x86-cross.yml` 出的版本没有 aarch64 一致性集,这一步与第 16 步的 `hft-core --check` 是它的原生校验,[development](../development.md#应急交叉编译release-x86-crossyml))。
5. `sudo /opt/hft/current-core/deploy/release/preflight.sh`:全部检查通过(含"主机上无编译工具链"、不存在 `/opt/hft/current` 与 `/opt/hft/venv`;`dev2` 另含护栏检查,见 [deploy 预检](../../deploy/README.md#发布归档与-releasectl))。第 1–5 步的核对:DRL-73。

### B. 行为变更的验证(回放 + 模拟账户)

新版本验证 = 回放 verify + 模拟账户运行。`sim` 实例可以先于 `prod` 跑新版本(dev2 与升级主机都如此)。

6. `sudo releasectl install --archive … --sha256-file …` 安装到 `releases/<v>`(不激活)。
7. 离线回放:用第 6 步已安装(未激活)的新版本二进制 `/opt/hft/releases/<v>/bin/replayer verify`,在本机以与 `hft-replay-verify` 相同的低优先级(`systemd-run --slice=hftbatch.slice`,加上 hft-core 给 `hft-replay-verify` 的 `Nice` 与冷盘 I/O 带宽上限)逐日校验最近 7 天的冷副本,分歧逐条解释;dev2 上 `/var/lib/hft/guard/tripped` 存在或护栏触发即中止,改在资源空闲时分批续跑。不在服务器上编译,也不把回放放进 CI。
8. 改动了策略代码或执行层时:激活后先让受影响策略的**模拟账户**以 Live 模式运行 ≥ 24 h,再恢复实盘账户的 Live;比较模拟账户在新旧版本下的报告(`/sim`),差异有解释。先只激活 `sim` 组(prod 留在旧版本),模拟观察通过后再激活 `core` 组。新策略的首次上线按 [launch](launch.md)。
   Model 检查点不随发版失效(检查点的键与失效条件归 [hft-core 检查点表](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属));策略改了状态布局版本时检查点会被丢弃(`Note{ModelCheckpointDiscarded}`),激活后按 [launch Model 预热](launch.md#model-预热) 预热。第 6–8 步的核对:DRL-75。

### C. 激活

9. `[需授权]` 用 `/accounts` 记下每个 (实例, group, 账户) 当前的期望模式与有效模式(重启后核对)。
10. `[需授权]` 优雅停止 hft-core:`/shutdown`(`Drain`,Trader/Admin,降级,不需确认)或 `sudo systemctl stop hft-core@prod`;期望 journal 中依次出现撤单(若 `shutdown.cancel_open_orders = true`)、`Drain`(等待条件归 [hft-sequencer](../../crates/hft-sequencer/README.md):全部链接、IOC 与 HedgeRetry 结束,且未完成的 MarginStress/Hard 引擎减仓已到达或超时)与快照,段已封存,`HeartbeatBlock.state = Stopped`,kill-switch 无动作。
11. 只激活本次发布涉及的单元组([deploy 分组激活](../../deploy/README.md#单元组与分组激活));只激活 `sim` 组时不停 prod,步骤 10 改为对 sim 实例 `systemctl stop hft-core@sim`。升级主机上激活 `cold` 组(会停 watchtower 与 kill-switch)或预计要重启主机时,先 `sudo hft-maint start <分钟>`(≤ 30;机外看门狗在截止前只发 Telegram、不打电话,[deploy 机外看门狗](../../deploy/README.md#机外看门狗)),等看门狗读到一次(≤ 15 s)再继续;结束后 `sudo hft-maint stop`。
12. `sudo releasectl activate --version <v> --group core`(或 `cold`、`sim`):期望 `{"status":"activated_stopped", …}`;`current-<group> → releases/<v>`、`previous-<group> → 旧版本`;本组单元 inactive,其他组与 dev2 守护照常运行;`/run/hft/hostprofile.json` 已更新。
13. `pending_reboot = true`(只会出现在 `c4`/`c8`/`c16`/`c32`/`metal`):`[需授权]` 重启主机,开机后 `hft-hostprobe` 退出码 0 再继续。
14. `hostprobe` 有其他不符 → 停止,按 [host-tuning](host-tuning.md)(或 dev2 护栏配置)处理;不得以 `on_mismatch = degrade` 绕过。第 9–14 步的核对:DRL-76;升级主机上的维护静默与重启:DRL-77。

### D. 启动与第一分钟验收

15. `[需授权]` 启动本次激活的组;`cold` 组按 `hft-kill-switch` → `hft-watchtower` → `hft-reconciler` → `hft-journal-archiver` → `hft-ops-bot` → `hft-pipeline` → `hft-treasury` 的顺序,kill-switch 必须先于 hft-core 处于 active(`hft-imds-lock` 与装有它的主机上的 `hft-cred-broker` 先于冷进程 active);`core` 组 = `hft-core`(`dev2` 上只在测试网联调窗口经 `hft-window start` 启动,守护未运行时无法启动);`sim` 组 = `hft-core@sim`(不依赖 kill-switch,可在任何时刻单独启动;dev2 上同样要求守护在运行)。
16. 第一分钟验收(逐项,全部满足才报告"已上线"):
    - `systemctl is-active` 全部 active;`journalctl -u hft-core@prod -n 50` 无错误;
    - `Note{StartupStep}` 显示启动各步完成,总耗时 < 30 s([hft-core 启动顺序](../../apps/hft-core/README.md#启动顺序));
    - seq journal 在增长;
    - 每个 venue 行情 `ConnState{Up}`,每 (venue, Live 账户) 下单连接 `Up`;
    - 启动闸门:每个账户的 `Note{StartupGate}` 全部位为 1(含 `IDENTITY`——每把交易 key 分别核 UID、`ACCOUNT_CFG`、`RECONCILED`;位清单见 [hft-risk](../../crates/hft-risk/README.md));`kill-switch selftest` 与 reconciler 启动日志中各自 key 的 UID 核对通过;
    - 冷启动后每个账户带 `Recovery` cap。**干净 Drain 重启**(同版本、尾部为空、无 `Unknown`/`Broken`、无 `tail_lost`、这一轮对账全部 `Ignore` 或差异已处理)在闸门全 1 后自动解除;**发版是版本变化,不属于干净重启**——`[需授权]` 闸门全 1 后交易员逐账户 `/resume recovery <a>`(Trader + 确认),再核对各 cell 的有效模式与步骤 9 的记录一致(DRL-78);
    - `HostProfile` 事件存在且无不符;watchtower 有样本;kill-switch `status` 为 `Watching`;`dev2` 上守护 active 且未触发;
    - 停机前未对冲的链接:闸门放行后先重试一次再按截止判断(对冲截止只计引擎运行时间),此时不要人工补腿;
    - 每个策略实例的 Model 检查点已载入(没有 `Note{ModelCheckpointDiscarded}`;有则按 [launch Model 预热](launch.md#model-预热) 处理)。
17. 15 分钟后复查:无 P0/P1;persist 滞后 p99 在 [hft-journal 延迟与资源契约](../../crates/hft-journal/README.md#延迟与资源契约) 给本主机冷盘类型的界内;升级主机上直方图与上一版本同时段对比无劣化(`dev2` 不比较延迟)。第 15–17 步的核对:DRL-76。

### E. 记录与清理

18. `sudo releasectl record --version <v> --acceptance passed --report <path>`;记录写入 `/opt/hft/deployments/<ts>-<v>.json`(含 `channel`、`host_profile_hash`、`refdata_version`)。
19. 通过验收后,删除不被任何组的 `current-<group>` 或 `previous-<group>` 链接指向的版本目录 `[需授权]`(各组可能停在不同版本,例如 `cold` 组仍用较早版本的 venv);开发构建在服务器上只保留最近 3 个。
20. 写 `ops-log/<date>-release-<v>.md`:各步起止时间(构建、获取、校验、回放/模拟验证、激活、启动、验收)与关键路径时间。第 18–20 步的核对:DRL-79。

## refdata 变更

refdata 不热替换:名单、模式、阈值变化走 `SetParams`;**只有交易宇宙或账户变化**(新 inst、费率档、限速表、`VenueCaps` 覆盖、新增账户或 `SimAccountCfg` 回写)才需要本流程,规则见 [hft-core refdata 变更](../../apps/hft-core/README.md)。

1. 选窗口:同上;预计交易暂停 < 1 min(Drain + 冷启动 < 30 s + 对账闸门)。
2. `[只读]` Admin 用 [`tools/hft-cfg`](../../tools/README.md#hft-cfg) 合并输入(含 pipeline 产出的交易宇宙候选 `out/universe-<date>.toml`)生成 refdata blob 与差异报告;确认没有删除有持仓的 inst(否则 sequencer 会拒绝)。记下当前各 cell 的模式。
3. `[需授权]` Admin 经运维 CLI 发 `ReplaceRefdata{blob, version}`(PREVIEW 预览列出每个账户的挂单与 Live cell,确认后带 `expect_state_version` 提交;pipeline 从不发这条命令);期望 `CmdResult{Applied}`,随后 hft-core 进入 `Drain`,以退出码 75 退出并被 systemd 立即重启。
4. 验收:同步骤 16(闸门全 1、`ConfigBlob` 中 refdata `version` 为新值;新增账户核对 `IDENTITY` 与 `ACCOUNT_CFG`);同版本、尾部为空的 refdata 重启属于干净 Drain 重启;核对有效模式与第 2 步的记录一致。
5. 失败回退(各路径的判定归 [hft-core refdata 变更](../../apps/hft-core/README.md#refdata-变更drain--重启)):装配校验失败 → gateway 直接回 `Rejected{RefdataAssembly}`(没有 `CmdResult`,不进 Drain);Drain 前的复核失败 → `CmdResult{Rejected}`(`detail` 给出复核失败项),hft-core 不 Drain、不退出,继续用旧 refdata 运行;两者都是修正后重发。Drain 末的复核失败(`Note{RefdataRecheckFailed}`,P1)→ hft-core 以退出码 0 退出、systemd 不重启,journal 末尾是 Drain 快照、新 refdata 未生效:修正后人工启动(仍是旧 refdata)再重发。新 refdata 生效后启动失败(退出 2)→ 用上一版本 refdata blob 再发一次 `ReplaceRefdata`。`ReplaceRefdata` 只在 `instance.config_source = "journal"` 时允许,不要为回退改成 `file`。

用例:成功路径 DRL-80;装配校验不通过 DRL-81;sequencer 复核不通过 DRL-82;Drain 末复核不通过与新 refdata 启动失败的处置由 CORE-324、CORE-225 验证。

### F. 验证包

上线门槛 ③④ 的累计在服务器上运行验证包([deploy 验证包](../../deploy/README.md#验证包与-hft-verify));每个正式版发布后安装,开发构建不带验证包。

21. 在 dev2(M7 期间也在升级主机上)`sudo releasectl install-verify <v>`:校验 `.sha256` 与 `VERIFY.json`,解包到 `/opt/hft/verify/<v>/` 并切换 `/opt/hft/verify/current`;期望不停任何单元、`current-<group>` 不变。
22. `systemctl list-timers hft-verify.timer` 显示下一次运行;首次运行结束后 `hft-verify ledger show` 出现该版本的运行记录(含各目标组的 `path_digest`,[tools](../../tools/README.md#hft-verify))。升级主机上 ⑩ 的测量窗口内定时器与服务都停止;M8 起正式实盘主机的清单为 `verify = false`,不安装这两个单元。开正式实盘所用版本的验证包须按 [testing §6.1](../testing.md#61-服务器上的累计运行) 补跑。核对:DRL-83。

### G. 拉取 token 轮换

服务器拉取用的只读 token(`/etc/hft/github-release-token`;归属与权限见 [deploy 发布归档](../../deploy/README.md#发布归档与-releasectl))有效期 ≤ 1 年,到期前 30 天轮换:

23. `[需授权]` 负责人在组织 `<org>` 下新建 fine-grained token(只授权仓库 `<org>/hft`、Contents 读),记下到期日。
24. 在每台服务器上 `sudo install -m 0600 -o root -g root <新 token 文件> /etc/hft/github-release-token`;`fetch-release.sh --version <当前版本> --dry-run` 成功。
25. 撤销旧 token;ops-log 记录新到期日并设下一次提醒。核对:DRL-84。

## 验收

- 步骤 16 全部满足;步骤 17 无回归。
- 发布门槛:`replayer verify` 通过;M7 起,参考机上 `bench-gate --reference` 纯计算分段 p99 相对基线上涨 ≤ 10%([benchmarks](../../benchmarks/README.md)、[09 §2](../../design/09-可观测性、风控与控制面.md));改动交易所适配的版本有 dev2 测试网冒烟记录(前置条件 5)。

## 回退

1. `[需授权]` 优雅停止 hft-core(同步骤 10)。
2. `sudo releasectl rollback --version <previous> --group <g>`(冷,等价于以旧版本激活)。
3. 若本次发布改了调优文件并已重启:回滚后 hostprobe 可能报 `pending_reboot`,按步骤 13 处理。
4. 按步骤 15–17 启动与验收。
5. 回滚后的 journal:新版本写入的事件只追加、旧读者忽略未知字段([event-model 演进规则](../subsystems/event-model.md#演进规则));若新版本引入了旧版本恢复时拒绝的新 kind,此类发布必须在 PR 中声明"不可回滚到 <版本>",并先发布一个只增加读取能力的中间版本。

核对:DRL-85。

**崩溃后回滚**(新版本崩溃、journal 末尾不是 Drain 快照):旧版本不能从新版本留下的非 Drain 尾部起步(版本变化规则)。步骤:

1. 保全热段与冷副本(不删除任何 journal 文件)。
2. `[需授权]` 负责人决定回滚;`releasectl rollback` 到旧版本(不启动)。
3. `[需授权]` 用旧版本二进制一次性带参启动:`hft-core --recover-from-snapshot <latest|SEQ> --reason "<文本>"`(经单元 drop-in 或 `systemd-run` 传参,成功启动后立即移除该参数);它从本二进制能解码的最新快照(通常是升级前旧版本写的 Drain 快照)起步,按 `tail_lost` 处理:全部非终态订单 `Unknown`、每账户 `Recovery`、`RECONCILED` 严格、不自动解除(规则见 [hft-core 崩溃隔离与从快照恢复](../../apps/hft-core/README.md#崩溃隔离与从快照恢复))。
4. 快照之后的仓位与余额变化以对账差异出现,按 [incident-response](incident-response.md) B、B2、C 逐项以 `AdoptExchange`(仓位、余额)或 `SetBalanceBaseline`(非策略资产)处置、收敛 Unknown(资金费差异由账本自动校正,不需处置),之后 `/resume recovery <a>`;写 postmortem。

演练:DRL-86(一次性参数的移除:CORE-288)。

## 记录

每次发布与回滚都写部署记录与 ops-log;回滚必须附原因与 postmortem 链接(若构成事故)。
