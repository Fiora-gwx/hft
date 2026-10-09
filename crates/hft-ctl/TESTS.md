---
component: hft-ctl
prefix: CTL
spec: README.md
---

# hft-ctl 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。本文件覆盖 ctl-gateway(鉴权、处理顺序、限速、命令校验、`SetParams` 解码与 Mailbox、`GetSchema`、写环与审计、`CmdResult` 回传、去重与过期)与 Rust/Python 客户端;另以端到端用例(CTL-200–CTL-215,真实 sequencer)验证 [control-plane](../../docs/subsystems/control-plane.md) 每条命令经客户端可见的每种结果码——gateway 只透传,码由 sequencer 产生;sequencer 内部的判定步骤由 hft-sequencer 的用例覆盖。帧、模式、规范化与权限矩阵在 [hft-ctl-proto 的测试规格](../hft-ctl-proto/TESTS.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | CTL-121、CTL-160、CTL-300–CTL-303 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| gateway(warm 任务,`poll_once` 形态) | CTL-001–CTL-014、CTL-020、CTL-055、CTL-060、CTL-082、CTL-085、CTL-086、CTL-100、CTL-103、CTL-104、CTL-120、CTL-121、CTL-200–CTL-213 |
| gateway 侧的命令校验 | CTL-060–CTL-069、CTL-080–CTL-084、CTL-307 |
| `GetSchema`(只读,本地应答) | CTL-090–CTL-093 |
| 客户端 | CTL-130–CTL-142 |
| 实现说明 | 不适用:见子节 |
| 线协议 | CTL-033、CTL-115、CTL-214 |
| 鉴权 | CTL-030–CTL-040、CTL-050–CTL-055、CTL-069、CTL-101、CTL-102、CTL-205、CTL-303、CTL-304–CTL-306 |
| 权限表 | CTL-032、CTL-034–CTL-038 |
| chain-core 的 ctl 会话 | 不适用:里程碑外(optional 的 chain-core 接入时补写) |
| CmdResult 回传 | CTL-013、CTL-014、CTL-110–CTL-114、CTL-132、CTL-215 |
| 规划源文件 | CTL-140、CTL-141 |
| 延迟与资源契约 | CTL-086、CTL-150–CTL-156 |
| 测试与验收 | 不适用:非规范(README 中只保留指向本文件的链接) |
| 依赖 | CTL-161 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | CTL-003 |
| 开发备忘 | CTL-101、CTL-303 |

## 用例

**基线 `G0`**(`crates/hft-ctl/tests/common/mod.rs::g0()`;用例 `前置` 写"G0"时以下取值都显式生效,用例另列的取值覆盖它们;不依赖任何默认值):

- 策略集:hft-testkit 的 `TestStrategies`(`probe-taker` kind_id 0xFF01、`probe-target` kind_id 0xFF02;testkit 提供之前用 `tests/common/set.rs` 手写的同名种类,另有 `non_prod_only` 的 `probe-np` 0xFF03;字段 `win`(结构)、`thr`、`bad`(`validate` 拒绝非 0)、`mem`、`ckpt`,命令描述符 `op 1`/`op 2` 同 CTL-067);用例需要 `non_prod_only`、声明的 `mem_bytes`/`snapshot_bytes`、结构字段或命令描述符(`CmdDesc`)时,在 `前置` 写明所用 testkit 测试种类的这些属性。
- refdata:与 [hft-ctl-proto 基线 P0](../hft-ctl-proto/TESTS.md#用例) 相同(`fixtures/refdata/ctl-p0.blob`:账户 `acct-a`(1,Live,venue `binance-usdm` + `bitget-spot`,绑定 `s1-main`)、`acct-b`(2,Live,venue `binance-spot` + `bitget-usdt-futures`,绑定 `s2-main`)、`sim-a`(3,Sim,venue `binance-usdm` + `bitget-spot`,绑定 `s9-sim`(`StrategyId 3`));inst `binance-usdm:BTCUSDT` = 1、`binance-spot:BTCUSDT` = 2、`bitget-usdt-futures:BTCUSDT` = 3、`bitget-spot:BTCUSDT` = 4;group `g1` = `16777317`、`g2` = `33554533`、`g9` = `50331749`)。
- `GatewayConfig`:`result_wait_ms = 5000`、`write_rate_per_min = 12`、`warmup_rate_per_s = 50`、`dedup_capacity = 4096`、`advisor_mem_budget_mib = 256`、`advisor_ckpt_max_mib = 1024`;`socket_path` = 临时目录下的 `ctl.sock`。
- `principals`:uid 1100 = `hft-opsbot`(issuer `tg:1001` {Viewer}、`tg:1002` {Trader}、`tg:1003` {Admin}、`tg:1004` {Viewer, Trader}、`tg:1005` {Viewer}、`ops_bot`);uid 1101 = `hft-pipeline`(`pipeline`);uid 1102 = `hft-recon`(`reconciler`);uid 1200 = 登录 `alice`(`cli:alice` {Admin});uid 1201 = `tom`(`cli:tom` {Trader});uid 1202 = `vera`(`cli:vera` {Viewer})。
- ctl 环:hft-rt `ByteSpsc` 262 144 B(单条 ≤ 65 536 B);消费端由测试读取。`parsed`/`retired` 为单槽 `Mailbox`。
- 时钟:假 `TscUtc`,1 GHz(1 tick = 1 ns),tsc 0 对应 UTC `1790000000000000000`;连接以 harness 的 `connect_as(uid)` 建立(注入对端凭证,生产路径为 `SO_PEERCRED`)。
- journal:测试 journal + 假 sequencer `FakeSeq`(取 ctl 环、按序赋 seq 写进 journal,并按脚本写 `CmdResult`;缺省在取出后 1 ms 写 `Applied`,可暂停)。
- `active_params`:`(s1-main, acct-a)` 版本 41(blob `B41`,含 `g1`)、`(s2-main, acct-b)` 版本 7、`(s9-sim, sim-a)` 版本 3;`refdata_check` 为脚本函数(缺省通过)。
- 请求:按 hft-ctl-proto 基线的信封约定,每条请求新 `cmd_id`,`issued_at_ns` = 当前 UTC、`expires_at_ns` = `issued_at_ns` + 30 s;`tg:*` 请求带 `chat_type = "private"`,另注明群聊。
- "审计 Note"指写进 ctl 环的 `Note{source = CtlAudit}`(`v[]` 布局归 hft-event,本 crate 的填法见[鉴权](README.md#鉴权)第 5 条)。

**端到端基线 `E2E`**(`crates/hft-ctl/tests/e2e/mod.rs::e2e()`):G0 的 gateway 换掉 `FakeSeq`,接 hft-testkit DST 世界里真实的 `Sequencer<TestStrategies>`(`SeqActor`)、Live 账户的 `MockVenue` 与 `sim-a` 的 `SimActor`;客户端为本 crate 的 `CtlClient`。**稳态 `S0`**:全部账户启动闸门已放行;cell `(s1-main, acct-a, g1)`、`(s2-main, acct-b, g2)`、`(s9-sim, sim-a, g9)` 期望模式 Live;无 cap;全部 BBO 连接健康;此刻的 `ctl_state_version` 记为 `V0`。"两步确认"= 先 `preview()`,再以预览回带的 `ctl_state_version` 调 `confirm()`。

### CTL-001 GatewayConfig 各字段的默认值

- 规则:[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)
- 级别:table
- 里程碑:M2
- 前置:`GatewayConfig::new(socket_path, principals, 256, 1024, 8)`(没有默认值的三项作参数,其余取默认值)
- 实现:`crates/hft-ctl/tests/config.rs::ctl_001_config_defaults`

| 行 | 字段 | 期望 |
|---|---|---|
| CTL-001.1 | `result_wait_ms` | `5000` |
| CTL-001.2 | `write_rate_per_min` | `12` |
| CTL-001.3 | `warmup_rate_per_s` | `50` |
| CTL-001.4 | `dedup_capacity` | `4096` |
| CTL-001.5 | `advisor_mem_budget_mib`、`advisor_ckpt_max_mib`、`max_groups_per_instance` | 没有默认值:由装配从 ECF1 读入,是 `new` 的参数 |

### CTL-002 Gateway::new 拒绝容量不足的 ctl 环与不可用的 socket 路径

- 规则:[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)
- 级别:table
- 里程碑:M2
- 前置:G0,只改列出的一项
- 实现:`crates/hft-ctl/tests/config.rs::ctl_002_new_validation`

| 行 | 改动 | 期望 |
|---|---|---|
| CTL-002.1 | 无 | `Ok(Gateway)`;`socket_path` 处出现 Unix socket 文件,模式 `0660` |
| CTL-002.2 | ctl 环 262 143 B(256 KiB − 1) | `Err(GatewayError)`,不创建 socket 文件 |
| CTL-002.3 | ctl 环 524 288 B | `Ok` |
| CTL-002.4 | `socket_path` 的父目录不存在 | `Err(GatewayError)` |

### CTL-003 只监听本机 Unix socket,不开任何 TCP 端口

- 规则:[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)、[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:unit
- 里程碑:M2
- 前置:G0
- 步骤:
  1. `Gateway::new` 后运行 100 次 `poll_once`
  2. 枚举本进程打开的 socket(`/proc/self/net/tcp`、`tcp6` 与 fd 表)
- 期望:
  - 本进程没有任何 TCP 监听或连接;gateway 拥有的 socket 只有一个 `AF_UNIX` 监听者与它接受的连接
- 实现:`crates/hft-ctl/tests/config.rs::ctl_003_unix_socket_only`

### CTL-004 生产主机上 ctl socket 的属组与权限

- 规则:[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)
- 级别:host
- 里程碑:M5
- 环境:dev2,`hft-core@sim` 以 deploy 生成的单元运行
- 授权:无
- 步骤:
  1. `stat -c '%U:%G %a' /run/hft/core-sim/ctl.sock`
  2. 以不在 `hft-ctl` 组的用户(`hft-sim` 以外的普通测试用户)`connect` 该 socket
- 通过标准:步骤 1 输出属组 `hft-ctl`、权限 `660`;步骤 2 得到 `EACCES`
- 证据:命令输出存为 `CTL-004-<日期>.txt`,随 launch A 阶段记录归档
- 实现:`tests/host/CTL-004.sh::ctl_004_socket_perms`

### CTL-010 poll_once 在任何输入下不阻塞

- 规则:[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)
- 级别:prop
- 里程碑:M2
- 性质:每次 `poll_once` 的墙钟耗时 ≤ 1 ms(release 构建;debug 构建 ≤ 20 ms;同一输入重放 3 次取逐步最小值),全部 fd 非阻塞(慢连接、半帧、中途关闭不让它等待);返回值为真当且仅当本次处理了至少一个事件
- 生成:0–64 个连接;每个连接的字节流随机切成 1–4 096 字节的片,片间随机停顿 0–50 ms;其中 20% 的连接只发帧头不发体、10% 的连接中途关闭、10% 的连接不读响应
- 参考:每次调用的计时与系统调用记录
- 例数:PR 200
- 实现:`crates/hft-ctl/tests/task.rs::ctl_010_poll_once_never_blocks`

### CTL-011 每个连接一请求一响应后关闭

- 规则:[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)
- 级别:unit
- 里程碑:M2
- 前置:G0;`cli:tom` 的连接
- 步骤:
  1. 发一帧合法的 `HedgeFix{group g1}`,`account acct-a`
  2. 读响应后在同一连接上再发一帧同样的请求
- 期望:
  - 步骤 1 收到恰好一帧响应,随后读到 EOF
  - 步骤 2 写入失败(`EPIPE`)或被对端忽略;ctl 环中只有一条 `ControlCmd`
- 实现:`crates/hft-ctl/tests/task.rs::ctl_011_one_request_per_connection`

### CTL-012 wait_fds 覆盖监听者与全部连接

- 规则:[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)
- 级别:unit
- 里程碑:M2
- 前置:G0;`FakeSeq` 暂停(请求停在挂起状态)
- 步骤:
  1. 建立 3 个连接并各发一条合法请求,运行 `poll_once` 直到三条都写环
  2. 调 `wait_fds`
  3. 恢复 `FakeSeq`,三条都得到响应并关闭后再调 `wait_fds`
- 期望:
  - 步骤 2 的 `FdSet` 恰含监听 fd 与 3 个连接 fd
  - 步骤 3 的 `FdSet` 只含监听 fd
- 实现:`crates/hft-ctl/tests/task.rs::ctl_012_wait_fds`

### CTL-013 next_deadline 是最早的挂起超时

- 规则:[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)、[CmdResult 回传](README.md#cmdresult-回传)
- 级别:unit
- 里程碑:M2
- 前置:G0;`FakeSeq` 暂停;`result_wait_ms = 5000`
- 步骤:
  1. t = 0 收到请求 A 并写环;t = 1 000 ms 收到请求 B 并写环
  2. 调 `next_deadline`
  3. 恢复 `FakeSeq` 只为 A 写 `CmdResult`,A 得到响应后调 `next_deadline`
  4. B 也得到响应后调 `next_deadline`
- 期望:
  - 步骤 2 返回 `Some(tsc(5 000 ms))`;步骤 3 返回 `Some(tsc(6 000 ms))`;步骤 4 返回 `None`
- 实现:`crates/hft-ctl/tests/task.rs::ctl_013_next_deadline`

### CTL-014 挂起表满 64 时新请求回 busy

- 规则:[CmdResult 回传](README.md#cmdresult-回传)
- 级别:table
- 里程碑:M2
- 前置:G0;`FakeSeq` 暂停;请求都是 `cli:tom` 的 `HedgeFix`(各自不同 `cmd_id`)
- 实现:`crates/hft-ctl/tests/task.rs::ctl_014_pending_capacity`

| 行 | 已挂起数 | 期望 |
|---|---|---|
| CTL-014.1 | 62,再来 1 条 | 写环,挂起数 63 |
| CTL-014.2 | 63,再来 1 条 | 写环,挂起数 64 |
| CTL-014.3 | 64,再来 1 条 | `status = busy`、`code = Busy`;该请求不写 `ControlCmd`,只写审计 Note(`result_code = 31`) |
| CTL-014.4 | 64,其中 1 条得到 `CmdResult` 后再来 1 条 | 写环 |

### CTL-020 多处出错时按处理顺序报第一个

- 规则:[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)
- 级别:table
- 里程碑:M2
- 前置:G0;每行的请求同时具备两处问题;"环满"= ctl 环剩余空间小于该请求的全部记录;"限速已满"= 该 issuer 在最近 60 s 内已计入 12 条
- 实现:`crates/hft-ctl/tests/order.rs::ctl_020_first_failure_wins`

| 行 | 两处问题 | 期望 |
|---|---|---|
| CTL-020.1 | 未知 uid 9999 + 帧体不是 JSON | `unauthorized`,`cmd_id` 全零 |
| CTL-020.2 | 帧体不是 JSON + 未知 issuer `tg:9999` | `invalid_request` |
| CTL-020.3 | 未知 issuer `tg:9999` + `SetRisk`(经 Telegram 不允许) | `unauthorized` |
| CTL-020.4 | Viewer `tg:1001` 发 `SetMode→Live`(`Forbidden`)+ `confirmed = true` 无 `expect_state_version` | `Forbidden` |
| CTL-020.5 | Trader `SetMode→Live`,`confirmed = true` 无 `expect_state_version` + 已过期 | `invalid_request` |
| CTL-020.6 | 同一 `cmd_id` 的命令已在缓存中得到 `Applied` + 本次重发时已过期 | 返回缓存的 `Applied`,不写环 |
| CTL-020.7 | 已过期 + 限速已满 | `Expired` |
| CTL-020.8 | 限速已满 + 账户名 `nope` | `RateLimited` |
| CTL-020.9 | 账户名 `nope` + 环满 | `InvalidArgs{UnknownName}` |
| CTL-020.10 | `SmokeOrder` 指向 `sim-a`(`NotLive`)+ 环满 | `InvalidArgs{NotLive}` |
| CTL-020.11 | 合法请求 + 环满 | `busy` |

### CTL-030 未知 uid 不读请求即回 unauthorized 并关闭

- 规则:[鉴权](README.md#鉴权)
- 级别:unit
- 里程碑:M2
- 前置:G0;`connect_as(9999)`,连接建立后客户端写入一帧合法请求
- 步骤:
  1. 运行 `poll_once` 直到连接关闭
- 期望:
  - 响应为 `{"v":2,"cmd_id":"00000000-0000-0000-0000-000000000000","status":"unauthorized","code":"Unauthorized", …}`,随后 EOF
  - gateway 对该连接不读请求(未知 uid 的连接在接受时即进入只写状态)
  - ctl 环恰有一条审计 Note:`cmd_id` 全零、`cmd_kind = 0`、`result_code = 10`、`role = 0`、issuer 字节全零;没有 `ControlCmd`
- 实现:`crates/hft-ctl/tests/auth.rs::ctl_030_unknown_uid`

### CTL-031 每个 uid 只接受自己的 issuer 形式

- 规则:[鉴权](README.md#鉴权)、[hft-ctl-proto 主体与角色码](../hft-ctl-proto/README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:G0;每行发 `Annotate{kind OpsNote, text "x"}`,`account "*"`(`tg:*` 带 `chat_type "private"`);"通过"= 越过鉴权第 2 条(之后按权限表处理)
- 实现:`crates/hft-ctl/tests/auth.rs::ctl_031_issuer_forms_per_uid`

| 行 | uid / issuer | 期望 |
|---|---|---|
| CTL-031.1 | 1100 / `tg:1002` | 通过 |
| CTL-031.2 | 1100 / `ops_bot` | 通过 |
| CTL-031.3 | 1100 / `pipeline` | `unauthorized` |
| CTL-031.4 | 1100 / `cli:alice` | `unauthorized` |
| CTL-031.5 | 1100 / `tg:9999`(不在签发者表) | `unauthorized` |
| CTL-031.6 | 1100 / `tg:abc` | `unauthorized` |
| CTL-031.7 | 1101 / `pipeline` | 通过 |
| CTL-031.8 | 1101 / `tg:1002` | `unauthorized` |
| CTL-031.9 | 1102 / `reconciler` | 通过 |
| CTL-031.10 | 1102 / `pipeline` | `unauthorized` |
| CTL-031.11 | 1200 / `cli:alice` | 通过 |
| CTL-031.12 | 1200 / `cli:tom` | `unauthorized`(登录名与 uid 不符) |
| CTL-031.13 | 1201 / `cli:alice` | `unauthorized` |
| CTL-031.14 | 1200 / `cli:systemd` | `unauthorized` |
| CTL-031.15 | 1100 / `cli:systemd` | `unauthorized` |

### CTL-032 有效权限是 uid 上限、issuer 角色与聊天类型的交集

- 规则:[鉴权](README.md#鉴权)、[hft-ctl-proto 权限表](../hft-ctl-proto/README.md#权限表)
- 级别:table
- 里程碑:M2
- 前置:G0;其余字段合法
- 实现:`crates/hft-ctl/tests/auth.rs::ctl_032_effective_permission`

| 行 | issuer(聊天)/ 命令 | 期望 |
|---|---|---|
| CTL-032.1 | `tg:1003`(Admin,私聊)/ `SetRisk` | `Forbidden`(uid `hft-opsbot` 的上限不含"仅 `cli:<login>`"的命令) |
| CTL-032.2 | `cli:alice`(Admin)/ `SetRisk` | 越过权限检查,写环 |
| CTL-032.3 | `tg:1002`(群聊)/ `SetMode→Live` | `Forbidden` |
| CTL-032.4 | `tg:1002`(私聊)/ `SetMode→Live`(未确认) | 写环 |
| CTL-032.5 | `tg:1003`(私聊)/ `SetParams` | `Forbidden` |
| CTL-032.6 | `cli:tom`(Trader)/ `SmokeOrder` | `Forbidden` |
| CTL-032.7 | `tg:1003`(群聊)/ `SmokeOrder` | `Forbidden` |
| CTL-032.8 | `tg:1003`(私聊)/ `SmokeOrder`(未确认) | 写环 |

### CTL-033 CmdHdr 由 gateway 填写的字段

- 规则:[hft-ctl-proto 主体与角色码](../hft-ctl-proto/README.md#主体与角色码)、[线协议](README.md#线协议)、[鉴权](README.md#鉴权)
- 级别:table
- 里程碑:M2
- 前置:G0;请求在 gateway UTC = `1790000000000000000`(tsc 0)到达,`expires_at_ns` = 到达 + 30 s;断言写进 ctl 环的 `ControlCmd` 的 `CmdHdr`
- 实现:`crates/hft-ctl/tests/auth.rs::ctl_033_gateway_hdr_fields`

| 行 | issuer / 命令 | 期望 |
|---|---|---|
| CTL-033.1 | `tg:1004`({Viewer, Trader})/ 默认参数 `Kill{Global}` | `role = 2`、`peer_uid = 1100`、`expires_tsc = 30 000 000 000` |
| CTL-033.2 | `tg:1004` / `Annotate{OpsNote}` | `role = 2` |
| CTL-033.3 | `tg:1001`({Viewer})/ 默认参数 `Kill{Account(acct-a)}` | `role = 1` |
| CTL-033.4 | `cli:alice`({Admin})/ `HedgeFix` | `role = 4`、`peer_uid = 1200` |
| CTL-033.5 | `ops_bot` / 默认参数 `Kill{Global}` | `role = 0`、`peer_uid = 1100` |
| CTL-033.6 | `pipeline` / `SetParams` | `role = 0`、`peer_uid = 1101` |
| CTL-033.7 | 任一行 | `cmd_hash` 等于 `hft_ctl_proto::normalize` 对同一请求算出的值;`issuer` 字段等于请求的 issuer |

### CTL-034 Viewer 只能发紧急停止、GetSchema 与两类 Annotate,其余写命令被拒并审计

- 规则:[鉴权](README.md#鉴权)、[hft-ctl-proto 权限表](../hft-ctl-proto/README.md#权限表)、[control-plane 角色](../../docs/subsystems/control-plane.md#角色)
- 级别:table
- 里程碑:M2
- 前置:G0;issuer `tg:1001`(Viewer,私聊);每行一条请求,其余字段合法
- 实现:`crates/hft-ctl/tests/auth.rs::ctl_034_viewer_limits`

| 行 | 命令 | 期望 |
|---|---|---|
| CTL-034.1 | 默认参数 `Kill{Global}`,`preview = true` | 写环(`role = 1`、`hflags = PREVIEW`) |
| CTL-034.2 | 默认参数 `Kill{Account(acct-a)}`,`confirmed = true`、无 `expect_state_version` | 写环(`expect_state_version = 0`) |
| CTL-034.3 | `Annotate{OpsNote}`、`Annotate{LocalWrite}` 各一条 | 写环 |
| CTL-034.4 | `GetSchema{strategy_kind "probe-taker"}` | `Applied`,带 schema `body` |
| CTL-034.5 | `Kill{Account(acct-a), hard}` | `Forbidden` |
| CTL-034.6 | `Kill{Account(acct-a), flatten}` | `Forbidden` |
| CTL-034.7 | `Kill{Global}` 且 `cancel_resting = false` | `Forbidden` |
| CTL-034.8 | 默认参数 `Kill{Venue(bitget-spot)}` | `Forbidden` |
| CTL-034.9 | `Resume{Kill}` | `Forbidden` |
| CTL-034.10 | `SetMode→CloseOnly` | `Forbidden` |
| CTL-034.11 | `CancelAll` | `Forbidden` |
| CTL-034.12 | `StrategyCmd{reduce_only true}` | `Forbidden` |
| CTL-034.13 | `Annotate{Incident}` | `Forbidden` |
| CTL-034.14 | 第 5–13 行的每一条 | 不写 `ControlCmd`;各写一条审计 Note,`result_code = 11`、`role = 1` |

### CTL-035 ops_bot 自身只能发默认参数的 Kill{Global | Account} 与 Annotate{OpsNote}

- 规则:[鉴权](README.md#鉴权)、[hft-ctl-proto 主体与角色码](../hft-ctl-proto/README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:G0;uid 1100,issuer `ops_bot`
- 实现:`crates/hft-ctl/tests/auth.rs::ctl_035_ops_bot_self`

| 行 | 命令 | 期望 |
|---|---|---|
| CTL-035.1 | 默认参数 `Kill{Account(acct-a)}`,不带确认字段 | 写环,`role = 0`、`confirmed = 0` |
| CTL-035.2 | 默认参数 `Kill{Global}` | 写环 |
| CTL-035.3 | `Annotate{OpsNote}` | 写环 |
| CTL-035.4 | `Kill{Account(acct-a), hard}` | `Forbidden` |
| CTL-035.5 | `Kill{Account(acct-a), flatten}` | `Forbidden` |
| CTL-035.6 | `Annotate{LocalWrite}` | `Forbidden` |
| CTL-035.7 | `SetMode→CloseOnly` | `Forbidden` |
| CTL-035.8 | `GetSchema` | `Forbidden` |

### CTL-036 reconciler 只能写 ReconcileReport

- 规则:[鉴权](README.md#鉴权)、[hft-ctl-proto 主体与角色码](../hft-ctl-proto/README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:G0;uid 1102,issuer `reconciler`
- 实现:`crates/hft-ctl/tests/auth.rs::ctl_036_reconciler`

| 行 | 命令 | 期望 |
|---|---|---|
| CTL-036.1 | `ReconcileReport{report_id "7", items [2 条]}` | `Applied`,环中 2 条 `ReconcileReport` |
| CTL-036.2 | 默认参数 `Kill{Global}` | `Forbidden` |
| CTL-036.3 | `SetMode→CloseOnly` | `Forbidden` |
| CTL-036.4 | `Annotate{OpsNote}` | `Forbidden` |

### CTL-037 cli:systemd 的命令只经内部通道到达

- 规则:[鉴权](README.md#鉴权)、[hft-ctl-proto 主体与角色码](../hft-ctl-proto/README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:G0;向 `internal` 推入 `InternalCmd`
- 实现:`crates/hft-ctl/tests/auth.rs::ctl_037_internal_channel`

| 行 | 输入 | 期望 |
|---|---|---|
| CTL-037.1 | `InternalCmd::Drain{cancel_resting: true}` | 环中一条 `ControlCmd{Drain{cancel_resting = 1}}`:`issuer = "cli:systemd"`、`role = 0`、`peer_uid = 0`、`account = 0`;一条审计 Note(`chat_type` 码 0) |
| CTL-037.2 | `InternalCmd::Drain{cancel_resting: false}` | `cancel_resting = 0` |
| CTL-037.3 | `InternalCmd::CancelAll` | 一条 `ControlCmd{CancelAll}`,`scope = {0, 0, 0, 0xFFFF_FFFF, 0}`,`issuer = "cli:systemd"` |
| CTL-037.4 | 经 socket(uid 1200)发 issuer `cli:systemd` 的 `Drain` | `unauthorized`,不写 `ControlCmd` |

### CTL-038 群聊只放行权限表"群聊"列为 ✓ 的命令

- 规则:[鉴权](README.md#鉴权)、[hft-ctl-proto 权限表](../hft-ctl-proto/README.md#权限表)
- 级别:table
- 里程碑:M2
- 前置:G0;每行 `chat_type = "group"`
- 实现:`crates/hft-ctl/tests/auth.rs::ctl_038_group_chat`

| 行 | issuer / 命令 | 期望 |
|---|---|---|
| CTL-038.1 | `tg:1002` / 默认参数 `Kill{Global}` | 写环 |
| CTL-038.2 | `tg:1003` / 默认参数 `Kill{Venue(bitget-spot)}` | 写环 |
| CTL-038.3 | `tg:1002` / `SetMode→CloseOnly`(group `g1`) | 写环 |
| CTL-038.4 | `tg:1001` / 默认参数 `Kill{Account(acct-a)}`,`preview = true` | 写环 |
| CTL-038.5 | `tg:1001` / `GetSchema` | `Applied` |
| CTL-038.6 | `tg:1002` / `Kill{Global, hard}` | `Forbidden` |
| CTL-038.7 | `tg:1002` / `Kill{Account(acct-a), flatten}` | `Forbidden` |
| CTL-038.8 | `tg:1002` / `SetMode→Shadow` | `Forbidden` |
| CTL-038.9 | `tg:1002` / `CancelAll` | `Forbidden` |
| CTL-038.10 | `tg:1002` / `Drain` | `Forbidden` |
| CTL-038.11 | `tg:1001` / `Annotate{OpsNote}` | `Forbidden` |

### CTL-039 check_confirm_fields 在得到有效角色之后执行

- 规则:[鉴权](README.md#鉴权)、[hft-ctl-proto 二次确认](../hft-ctl-proto/README.md#二次确认)
- 级别:table
- 里程碑:M2
- 前置:G0
- 实现:`crates/hft-ctl/tests/auth.rs::ctl_039_confirm_fields_after_role`

| 行 | issuer / 命令 / 确认字段 | 期望 |
|---|---|---|
| CTL-039.1 | `tg:1002` / `SetMode→Live` / `confirmed = true`,无 `expect_state_version` | `invalid_request` |
| CTL-039.2 | `tg:1002` / `SetMode→Live` / `confirmed = true`,`expect_state_version = 0` | `invalid_request` |
| CTL-039.3 | `tg:1002` / `SetMode→Live` / `confirmed = true`,`expect_state_version = 21` | 写环,`expect_state_version = 21` |
| CTL-039.4 | `tg:1001` / 默认参数 `Kill{Account(acct-a)}` / `confirmed = true`,无 `expect_state_version` | 写环,`expect_state_version = 0` |
| CTL-039.5 | `tg:1004`({Viewer, Trader})/ 默认参数 `Kill{Account(acct-a)}` / `confirmed = true`,无 `expect_state_version` | `invalid_request`(有效角色为 Trader,规则为 `Never` 而非 `PreviewBound`) |
| CTL-039.6 | `tg:1002` / 默认参数 `Kill{Global}` / `confirmed = true`,无 `expect_state_version` | `invalid_request` |
| CTL-039.7 | `tg:1001` / `SetMode→Live` / `confirmed = true`,无 `expect_state_version` | `Forbidden`(权限先于确认字段) |

### CTL-040 ctl 的 Admin 角色只来自签发者表

- 规则:[control-plane 角色](../../docs/subsystems/control-plane.md#角色)、[鉴权](README.md#鉴权)
- 级别:unit
- 里程碑:M2
- 前置:G0,但 `cli:alice` 的角色集合改为 {Trader};测试工作目录下 `.github/hft-admins.txt` 含一行 `alice`
- 步骤:
  1. `cli:alice` 发 `SmokeOrder{venue bitget-spot, inst bitget-spot:BTCUSDT, side Buy}`,`account acct-a`
- 期望:
  - `Forbidden`;不写 `ControlCmd`;gateway 的鉴权输入只有构造时给出的签发者表(`.github/hft-admins.txt` 不被读取或修改)
- 实现:`crates/hft-ctl/tests/auth.rs::ctl_040_admin_from_issuer_table_only`

### CTL-050 write_rate_per_min 按 (now − 60 s, now] 的滑动窗口计数

- 规则:[鉴权](README.md#鉴权)
- 级别:table
- 里程碑:M2
- 前置:G0;`write_rate_per_min = 12`;issuer `cli:tom`;在 t = 0、1 s、…、11 s 各发一条合法 `HedgeFix`(计入 12 条);每行在列出的时刻再发一条 `HedgeFix`
- 实现:`crates/hft-ctl/tests/rate.rs::ctl_050_write_rate_window`

| 行 | 时刻 | 期望 |
|---|---|---|
| CTL-050.1 | t = 59 999 ms | `RateLimited`;不写 `ControlCmd`;审计 Note `result_code = 30` |
| CTL-050.2 | t = 60 000 ms | 写环(t = 0 的那条已不在 (0, 60 000] 内) |
| CTL-050.3 | 050.2 之后紧接 t = 60 000 ms 再发一条 | `RateLimited`(窗口内又是 12 条:1 s…11 s 与 60 000 ms) |
| CTL-050.4 | t = 61 000 ms | 写环 |

### CTL-051 降级类、只读、InjectWarmup 与 ReconcileReport 不计入 write_rate_per_min

- 规则:[鉴权](README.md#鉴权)
- 级别:table
- 里程碑:M2
- 前置:G0;`cli:tom` 在最近 60 s 内已计入 12 条(再发一条 `HedgeFix` 会 `RateLimited`);每行由 `cli:tom` 发出
- 实现:`crates/hft-ctl/tests/rate.rs::ctl_051_uncounted_classes`

| 行 | 命令 | 期望 |
|---|---|---|
| CTL-051.1 | 默认参数 `Kill{Global}` | 写环 |
| CTL-051.2 | `CancelAll` | 写环 |
| CTL-051.3 | `SetMode→CloseOnly` | 写环 |
| CTL-051.4 | `VenueNotice{Delisting}` | 写环 |
| CTL-051.5 | `GetSchema` | `Applied` |
| CTL-051.6 | `InjectWarmup{strategy s1-main, …}` | 写环 |
| CTL-051.7 | 第 1–6 行之后再发 `HedgeFix` | `RateLimited`(上面各行没有改变计数) |

### CTL-052 被后续步骤拒绝的请求也计数,RateLimited 本身不计数

- 规则:[鉴权](README.md#鉴权)
- 级别:unit
- 里程碑:M2
- 前置:G0;issuer `cli:tom`
- 步骤:
  1. t = 0 起 12 条 `HedgeFix`,`account "nope"`(规范化失败 `UnknownName`)
  2. t = 20 s 发一条合法 `HedgeFix`
  3. t = 30 s 再发一条合法 `HedgeFix`
  4. t = 60 001 ms 发一条合法 `HedgeFix`
- 期望:
  - 步骤 1 的 12 条都回 `InvalidArgs{UnknownName}`
  - 步骤 2、3 都回 `RateLimited`(步骤 2 的被拒不延长窗口)
  - 步骤 4 写环
- 实现:`crates/hft-ctl/tests/rate.rs::ctl_052_rejected_still_counted`

### CTL-053 preview 请求计入写命令限速

- 规则:[鉴权](README.md#鉴权)
- 级别:unit
- 里程碑:M2
- 前置:G0;issuer `tg:1002`
- 步骤:
  1. 60 s 内 6 次"预览 + 确认"`SetMode→Live`(共 12 条请求)
  2. 再发一条 `preview = true` 的 `SetMode→Live`
- 期望:
  - 步骤 1 的 12 条都写环;步骤 2 回 `RateLimited`
- 实现:`crates/hft-ctl/tests/rate.rs::ctl_053_preview_counted`

### CTL-054 限速按 issuer 分开计

- 规则:[鉴权](README.md#鉴权)
- 级别:unit
- 里程碑:M2
- 前置:G0;`tg:1002` 在最近 60 s 内已计入 12 条
- 步骤:
  1. `tg:1003` 发 `HedgeFix`
  2. `tg:1002` 发 `HedgeFix`
- 期望:
  - 步骤 1 写环(同一 uid 1100 下不同 issuer 不共享计数);步骤 2 `RateLimited`
- 实现:`crates/hft-ctl/tests/rate.rs::ctl_054_per_issuer`

### CTL-055 InjectWarmup 按 warmup_rate_per_s 限速

- 规则:[鉴权](README.md#鉴权)、[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)
- 级别:table
- 里程碑:M2
- 前置:G0;`warmup_rate_per_s = 50`;issuer `pipeline`;t = 0 起每 10 ms 发一块 `InjectWarmup`(4 096 B)
- 实现:`crates/hft-ctl/tests/rate.rs::ctl_055_warmup_rate`

| 行 | 块序号(发出时刻) | 期望 |
|---|---|---|
| CTL-055.1 | 第 1–50 块(0–490 ms) | 全部写环 |
| CTL-055.2 | 第 51 块(500 ms) | `RateLimited`,不写环 |
| CTL-055.3 | 第 101 块(1 000 ms,窗口 (0, 1 000] 内已计入第 2–50 块共 49 条) | 写环 |
| CTL-055.4 | 第 1–50 块期间 `pipeline` 另发 `SetParams` | 写环(InjectWarmup 不占 `write_rate_per_min`) |

### CTL-060 SetRisk 的 gateway 校验与 Mailbox 交接

- 规则:[gateway 侧的命令校验](README.md#gateway-侧的命令校验)、[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)
- 级别:table
- 里程碑:M2
- 前置:G0;issuer `cli:alice`;`RiskParams` 由 hft-risk 测试构建器生成,基于当前生效版本 7
- 实现:`crates/hft-ctl/tests/validate.rs::ctl_060_set_risk`

| 行 | 请求 | 期望 |
|---|---|---|
| CTL-060.1 | `account acct-a`,只改 `accounts[acct-a]` 的字段,版本 8 | 写环(`ConfigBlob` 分段在 `ControlCmd{SetRisk}` 之前);`parsed` Mailbox 中有 `ParsedConfig::Risk`,其 sha256 与 `BlobRef.sha256` 相同 |
| CTL-060.2 | `account acct-a`,另改了 `accounts[acct-b]` 的字段 | `InvalidArgs{OutOfScope}`;不写 `ControlCmd`、Mailbox 为空 |
| CTL-060.3 | `account acct-a`,另改了全局字段 | `InvalidArgs{OutOfScope}` |
| CTL-060.4 | `account "*"`,改全局字段 | 写环 |
| CTL-060.5 | blob 不是 `RiskParams` 编码(随机 2 048 字节) | `InvalidArgs{BadValue}` |

### CTL-061 SimReset 与 SimTransfer 只对模拟账户,venue 与资产须在账户内

- 规则:[gateway 侧的命令校验](README.md#gateway-侧的命令校验)
- 级别:table
- 里程碑:M2
- 前置:G0;issuer `cli:tom`
- 实现:`crates/hft-ctl/tests/validate.rs::ctl_061_sim_commands`

| 行 | 请求 | 期望 |
|---|---|---|
| CTL-061.1 | `SimReset`,`account sim-a` | 写环 |
| CTL-061.2 | `SimReset`,`account acct-a` | `InvalidArgs{NotSim}` |
| CTL-061.3 | `SimTransfer{from_venue binance-usdm, to_venue bitget-spot, asset USDT, amount "100"}`,`sim-a` | 写环 |
| CTL-061.4 | 同上,`account acct-a` | `InvalidArgs{NotSim}` |
| CTL-061.5 | `SimTransfer{from_venue binance-spot, …}`,`sim-a`(`binance-spot` 不在 `sim-a` 内) | `InvalidArgs{UnknownName}` |
| CTL-061.6 | `SimTransfer{…, to_venue bitget-usdt-futures}`,`sim-a` | `InvalidArgs{UnknownName}` |
| CTL-061.7 | `SimFault{venue binance-usdm, fault RejectNew}`,`sim-a`,issuer `cli:alice` | 写环 |
| CTL-061.8 | 同上,`account acct-a` | `InvalidArgs{NotSim}` |
| CTL-061.9 | `SimFault{venue okx-swap, fault RejectNew}`,`sim-a`(`okx-swap` 不在 `sim-a` 内),issuer `cli:alice` | `InvalidArgs{UnknownName}` |

### CTL-062 VenueNotice 的 venue、inst、结算周期与维护窗口校验

- 规则:[gateway 侧的命令校验](README.md#gateway-侧的命令校验)
- 级别:table
- 里程碑:M2
- 前置:G0;issuer `cli:tom`;`account "*"`
- 实现:`crates/hft-ctl/tests/validate.rs::ctl_062_venue_notice`

| 行 | 请求 | 期望 |
|---|---|---|
| CTL-062.1 | `{venue binance-usdm, inst binance-usdm:BTCUSDT, kind Delisting, at_ns "1790100000000000000"}` | 写环 |
| CTL-062.2 | `{venue binance-usdm, inst bitget-spot:BTCUSDT, kind Delisting, …}` | `InvalidArgs{UnknownName}`(inst 不属于该 venue) |
| CTL-062.3 | `kind FundingInterval`,`new_s` 依次为 `"3600"`、`"7200"`、`"14400"`、`"28800"` | 四条都写环 |
| CTL-062.4 | `new_s "3599"` | `InvalidArgs{BadValue}` |
| CTL-062.5 | `new_s "3601"` | `InvalidArgs{BadValue}` |
| CTL-062.6 | `new_s "1800"` | `InvalidArgs{BadValue}` |
| CTL-062.7 | `new_s "86400"` | `InvalidArgs{BadValue}` |
| CTL-062.8 | `kind Maintenance, from_ns "1790100000000000000", to_ns "1790100000000000001"` | 写环 |
| CTL-062.9 | `from_ns = to_ns` | `InvalidArgs{BadValue}` |
| CTL-062.10 | `from_ns = to_ns + 1` | `InvalidArgs{BadValue}` |

### CTL-063 AdoptExchange 余额条目的资产与 group 校验

- 规则:[gateway 侧的命令校验](README.md#gateway-侧的命令校验)
- 级别:table
- 里程碑:M2
- 前置:G0;issuer `cli:tom`;`account acct-a`;`venue bitget-spot`、`report_id "7"`
- 实现:`crates/hft-ctl/tests/validate.rs::ctl_063_adopt_exchange`

| 行 | 请求 | 期望 |
|---|---|---|
| CTL-063.1 | `item Balance, asset BTC, group "16777317"` | 写环 |
| CTL-063.2 | `item Balance, asset BTC`(无 group) | 写环,载荷 `group = 0` |
| CTL-063.3 | `item Balance, asset BTC, group "33554533"`(高 8 位是 `s2-main`,不是 `acct-a` 绑定的实例) | `InvalidArgs{BadGroup}` |
| CTL-063.4 | `item Balance, asset XYZ` | `InvalidArgs{UnknownName}` |
| CTL-063.5 | `item Position, venue binance-usdm, inst binance-usdm:BTCUSDT` | 写环 |

### CTL-064 SmokeOrder 只对 Live 账户、venue 与 inst 须匹配、ttl_ms 在范围内

- 规则:[gateway 侧的命令校验](README.md#gateway-侧的命令校验)、[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)
- 级别:table
- 里程碑:M2
- 前置:G0;issuer `cli:alice`(Admin);基准请求 `SmokeOrder{venue bitget-spot, inst bitget-spot:BTCUSDT, side Buy}`,`account acct-a`
- 实现:`crates/hft-ctl/tests/validate.rs::ctl_064_smoke_order`

| 行 | 改动 | 期望 |
|---|---|---|
| CTL-064.1 | 无 | 写环,`ttl_ms = 10 000` |
| CTL-064.2 | `account sim-a`(venue、inst 同为 `sim-a` 内的 `bitget-spot`) | `InvalidArgs{NotLive}` |
| CTL-064.3 | `venue binance-spot, inst binance-spot:BTCUSDT`(`binance-spot` 不在 `acct-a` 内) | `InvalidArgs{UnknownName}` |
| CTL-064.4 | `inst binance-usdm:BTCUSDT`(不属于 `bitget-spot`) | `InvalidArgs{UnknownName}` |
| CTL-064.5 | `ttl_ms "0"` | `InvalidArgs{BadValue}` |
| CTL-064.6 | `ttl_ms "60001"` | `InvalidArgs{BadValue}` |
| CTL-064.7 | `ttl_ms "60000"` | 写环 |

### CTL-065 WriteOffResidual 的 group 必须是该账户绑定实例的具体 group

- 规则:[gateway 侧的命令校验](README.md#gateway-侧的命令校验)
- 级别:table
- 里程碑:M2
- 前置:G0;issuer `cli:alice`;`account acct-a`
- 实现:`crates/hft-ctl/tests/validate.rs::ctl_065_write_off_residual`

| 行 | `group` | 期望 |
|---|---|---|
| CTL-065.1 | `"16777317"` | 写环 |
| CTL-065.2 | `"33554533"`(属 `s2-main`) | `InvalidArgs{BadGroup}` |
| CTL-065.3 | `"*"` | `InvalidArgs{BadGroup}` |
| CTL-065.4 | `"0"` | `InvalidArgs{BadGroup}` |

### CTL-066 ReplaceRefdata 的装配校验失败回 Rejected{RefdataAssembly},不写环

- 规则:[gateway 侧的命令校验](README.md#gateway-侧的命令校验)、[control-plane 结果码](../../docs/subsystems/control-plane.md#结果码)
- 级别:table
- 里程碑:M2
- 前置:G0;issuer `cli:alice`;`account acct-a`;新 refdata blob 版本 2(当前 1);`refdata_check` 脚本按行返回失败项
- 实现:`crates/hft-ctl/tests/validate.rs::ctl_066_refdata_assembly`

| 行 | `refdata_check` 的结果 | 期望 |
|---|---|---|
| CTL-066.1 | 账户名前缀不符 | `status = ok`、`code = Rejected`、`detail = {"name":"RefdataAssembly","arg":"AccountNamePrefix"}`(arg 1) |
| CTL-066.2 | Live 账户缺交易 key | `arg = "MissingCredential"`(2) |
| CTL-066.3 | 模拟撮合配置干校验失败 | `arg = "SimConfig"`(3) |
| CTL-066.4 | 线程计划未覆盖全部 venue | `arg = "ThreadPlan"`(4) |
| CTL-066.5 | 某账户尘埃线低于其交易腿的最小名义 | `arg = "DustBelowMinNotional"`(5) |
| CTL-066.6 | 第 1–5 行的每一行 | 环中没有 `ConfigBlob` 与 `ControlCmd`、Mailbox 为空;只有一条审计 Note,`result_code = 27`、`cmd_kind = 0x0009`;journal 中没有对应的 `CmdResult` |
| CTL-066.7 | 通过 | 写环(`ConfigBlob` 分段在 `ControlCmd{ReplaceRefdata}` 之前);Mailbox 中有 `ParsedConfig::Refdata`,内容为 `RefData::load(新 blob, 当前)` 的结果 |
| CTL-066.8 | 通过,但 `RefData::load` 返回 `VersionNotIncreasing` | 同 066.7:写环,Mailbox 中的结果为该错误(由 sequencer 复核后回 `Rejected{RefdataRecheck}`) |

### CTL-067 StrategyCmd 的实例、绑定、group 与载荷描述符校验

- 规则:[gateway 侧的命令校验](README.md#gateway-侧的命令校验)
- 级别:table
- 里程碑:M2
- 前置:G0;issuer `cli:tom`;`s1-main` 的种类声明命令描述符 `op 1`(`reduce_only = false`,字段 `qty: u32`,范围 1..=100)与 `op 2`(`reduce_only = true`,无字段);载荷由 SDK `params::encode_cmd` 生成
- 实现:`crates/hft-ctl/tests/validate.rs::ctl_067_strategy_cmd`

| 行 | 请求 | 期望 |
|---|---|---|
| CTL-067.1 | `strategy s1-main, account acct-a, group g1, reduce_only false`,载荷 `op 1, qty 5` | 写环,不放 Mailbox |
| CTL-067.2 | 同上,`group "none"` | 写环,载荷 `group = 0` |
| CTL-067.3 | `account acct-b` | `InvalidArgs{AccountBound}` |
| CTL-067.4 | `group "*"` | `InvalidArgs{BadGroup}` |
| CTL-067.5 | 载荷 `op 9`(未声明) | `InvalidArgs{ParamInvalid}`,`arg = "Strategy"` |
| CTL-067.6 | 载荷 `op 1, qty 101` | `InvalidArgs{ParamInvalid}`,`arg` 为 `"Strategy:<qty 的字段下标>"` |
| CTL-067.7 | 载荷 `op 2`,`reduce_only false` | `InvalidArgs{ParamInvalid}`,`arg = "Strategy"` |
| CTL-067.8 | 载荷 `op 2`,`reduce_only true` | 写环 |
| CTL-067.9 | 载荷 257 字节 | `InvalidArgs{TooLong}` |

### CTL-068 InjectWarmup 的实例、账户与长度校验

- 规则:[gateway 侧的命令校验](README.md#gateway-侧的命令校验)
- 级别:table
- 里程碑:M2
- 前置:G0;issuer `pipeline`
- 实现:`crates/hft-ctl/tests/validate.rs::ctl_068_inject_warmup`

| 行 | 请求 | 期望 |
|---|---|---|
| CTL-068.1 | `strategy s2-main, local_kind "3", chunk_no "0", last false`,载荷 4 096 字节,无 `account` | 写环,`CmdHdr.account = 0` |
| CTL-068.2 | 同上,`account acct-b` | `InvalidArgs{AccountNotAllowed}` |
| CTL-068.3 | `strategy nope` | `InvalidArgs{UnknownName}` |
| CTL-068.4 | 载荷 4 097 字节 | `InvalidArgs{TooLong}` |

### CTL-069 gateway 拒绝的请求不写命令、不放 Mailbox,只写一条审计 Note

- 规则:[gateway 侧的命令校验](README.md#gateway-侧的命令校验)、[鉴权](README.md#鉴权)
- 级别:prop
- 里程碑:M2
- 性质:任一请求若得到 gateway 产生的拒绝(`unauthorized`、`invalid_request`、`Forbidden`、`Expired`、`RateLimited`、gateway 的 `InvalidArgs`、`Rejected{RefdataAssembly}`、Mailbox 未取走或挂起表满的 `busy`),则 ctl 环中该请求只有一条审计 Note(`result_code` 等于响应的码),没有 `ControlCmd`、`ConfigBlob`、`ReconcileReport`,Mailbox 状态不变;环满的 `busy` 连审计 Note 也没有
- 生成:G0;每个样本 1–20 条请求,命令、主体、聊天类型、账户、group、确认字段、过期时刻与载荷长度从合法值与越界值中随机抽取;随机穿插环满与 Mailbox 占用
- 参考:按 README 处理顺序写的参考判定函数
- 例数:PR 500
- 实现:`crates/hft-ctl/tests/prop.rs::ctl_069_rejections_write_audit_only`

### CTL-080 SetParams 容器、绑定、group 与 schema 校验的每个失败码

- 规则:[gateway 侧的命令校验](README.md#gateway-侧的命令校验)、[control-plane 结果码](../../docs/subsystems/control-plane.md#结果码)
- 级别:table
- 里程碑:M2
- 前置:G0;issuer `cli:tom`;`strategy s1-main, account acct-a`;基准 blob = 版本 42 的合法 SPB1(`g1` 两腿 `binance-usdm:BTCUSDT` + `bitget-spot:BTCUSDT`);每行只改一处
- 实现:`crates/hft-ctl/tests/set_params.rs::ctl_080_container_and_binding`

| 行 | 改动 | 期望 |
|---|---|---|
| CTL-080.1 | 无 | 写环,Mailbox 中有 `ParsedConfig::StrategyParams` |
| CTL-080.2 | 某 `GroupSpec` 的腿数为 0 | `InvalidArgs{BadGroup}` |
| CTL-080.3 | `GroupSpec.id` 的高 8 位为 2 | `InvalidArgs{BadGroup}` |
| CTL-080.4 | 头部 `kind_id = 0xFF02`(实例 `s1-main` 的种类是 0xFF01) | `InvalidArgs{KindMismatch}` |
| CTL-080.5 | 第二条腿改为 `binance-spot:BTCUSDT`(不在 `acct-a` 内) | `InvalidArgs{GroupCheck}`,`arg = "LegVenueNotInAccount"` |
| CTL-080.6 | 两条腿都是 `binance-usdm:BTCUSDT` | `InvalidArgs{SameExchangeLegs}` |
| CTL-080.7 | 第一条腿的 inst 状态为下架(非 `Trading`/`ReduceOnly`) | `InvalidArgs{GroupCheck}`,`arg = "LegNotTradable"` |
| CTL-080.8 | blob 内两个 `GroupSpec` 的 id 都是 `g1` | `InvalidArgs{GroupIdDup}` |
| CTL-080.9 | 头部 `kind_id = 0xFF7F`(未编译进来的种类) | `InvalidArgs{SchemaUnsupported}` |
| CTL-080.10 | `schema_ver` = 编译进来的版本 + 1 | `InvalidArgs{SchemaUnsupported}` |
| CTL-080.11 | `schema_digest` 末字节改 1 | `InvalidArgs{SchemaMismatch}` |
| CTL-080.12 | 第 2–11 行的每一行 | 不写 `ConfigBlob`/`ControlCmd`,Mailbox 为空,审计 Note `result_code = 20` |

### CTL-081 SetParams body 解码、non_prod_only 与 Model 预算的边界

- 规则:[gateway 侧的命令校验](README.md#gateway-侧的命令校验)、[control-plane 结果码](../../docs/subsystems/control-plane.md#结果码)
- 级别:table
- 里程碑:M2
- 前置:G0;`advisor_mem_budget_mib = 256`(268 435 456 B)、`advisor_ckpt_max_mib = 1024`(1 073 741 824 B);所用测试种类的 `model_mem_bytes`/`model_snapshot_bytes` 按参数字段 `mem`、`ckpt` 原样返回;`max_groups_per_instance = 8`;issuer `cli:tom`
- 实现:`crates/hft-ctl/tests/set_params.rs::ctl_081_body_and_budget`

| 行 | 输入 | 期望 |
|---|---|---|
| CTL-081.1 | body 少一个字段(截短 8 字节,`body_len` 随之更新,容器仍合法) | `InvalidArgs{ParamInvalid}`,`arg = "Strategy"` |
| CTL-081.2 | `validate` 拒绝第 2 个字段 | `InvalidArgs{ParamInvalid}`,`arg = "Strategy:2"` |
| CTL-081.3 | 9 个 group | `InvalidArgs{ParamInvalid}`,`arg = "Strategy"`(SDK `TooManyGroups`) |
| CTL-081.4 | 8 个 group | 写环 |
| CTL-081.5 | 种类 `non_prod_only = true`,目标账户 `is_prod = true`(refdata 另加账户 `acct-p` 与实例 `s3-np` 互相绑定) | `InvalidArgs{ParamInvalid}`,`arg = "NonProdOnly"` |
| CTL-081.6 | 同上,账户 `is_prod = false` | 写环 |
| CTL-081.7 | `mem = 268 435 456` | 写环,`ParsedConfig.model_mem = 268 435 456` |
| CTL-081.8 | `mem = 268 435 457` | `InvalidArgs{ParamInvalid}`,`arg = "ModelMemBudget"` |
| CTL-081.9 | `mem = 268 435 455` | 写环 |
| CTL-081.10 | `ckpt = 536 870 912`(2 × = 1 073 741 824) | 写环 |
| CTL-081.11 | `ckpt = 536 870 913` | `InvalidArgs{ParamInvalid}`,`arg = "ModelMemBudget"` |

### CTL-082 SetParams 通过后的写环顺序与 Mailbox 内容

- 规则:[gateway 侧的命令校验](README.md#gateway-侧的命令校验)、[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)
- 级别:unit
- 里程碑:M2
- 前置:G0;issuer `cli:tom`;版本 42 的合法 blob,180 000 字节
- 步骤:
  1. 发送 `SetParams{strategy s1-main, blob_b64, base_version "41"}`,`account acct-a`
  2. 读出 ctl 环与 `parsed` Mailbox
- 期望:
  - 环中依序:若干条 `ConfigBlob`(`PART` 分段,拼接 = blob,每条 ≤ 65 536 B),然后一条 `ControlCmd{SetParams}`(`BlobRef.sha256` = SHA-256(blob)、`base_version = 41`);另有一条审计 Note(`result_code = 0`)
  - Mailbox 中 `ParsedConfig::StrategyParams` 带解码后的参数、`GroupTable`(含 `g1`)、`model_mem`、`model_ckpt`、`structural_changed`、`reset_preview`,其 sha256 与 `BlobRef` 相同
- 实现:`crates/hft-ctl/tests/set_params.rs::ctl_082_write_order_and_mailbox`

### CTL-083 structural_changed 与 reset_preview 按生效副本计算

- 规则:[gateway 侧的命令校验](README.md#gateway-侧的命令校验)
- 级别:table
- 里程碑:M2
- 前置:G0;`(s1-main, acct-a)` 的生效副本为版本 41;所用测试种类有结构字段 `win`(`FieldDesc.structural = true`)与非结构字段 `thr`
- 实现:`crates/hft-ctl/tests/set_params.rs::ctl_083_structural_changed`

| 行 | 新参数相对版本 41 | 期望 `ParsedConfig` |
|---|---|---|
| CTL-083.1 | 只改 `thr` | `structural_changed = false`;`reset_preview` 为 SDK `S::reset_preview(当前, 新)` 的返回值(0 个 group) |
| CTL-083.2 | 改 `win` | `structural_changed = true`;`reset_preview.reset_groups` = SDK 给出的受影响 group 数 |
| CTL-083.3 | `win` 改回与版本 41 相同的值、只改 `thr` | `structural_changed = false` |

### CTL-084 比较基准随本 gateway 写出的 SetParams 的 Applied 更新

- 规则:[gateway 侧的命令校验](README.md#gateway-侧的命令校验)
- 级别:unit
- 里程碑:M2
- 前置:G0;`FakeSeq` 取走 Mailbox 对象后暂不写 `CmdResult`;所用测试种类同 CTL-083
- 步骤:
  1. `SetParams` #1:`(s1-main, acct-a)` 版本 42,改 `win`
  2. #1 的结果未到时,`SetParams` #2:`(s1-main, acct-a)` 版本 43
  3. 同时 `SetParams` #3:`(s2-main, acct-b)` 版本 8
  4. `FakeSeq` 为 #1 写 `CmdResult{Applied}`,gateway 尾随读到后,发 #4:`(s1-main, acct-a)` 版本 43,`win` 与 #1 相同、只改 `thr`
  5. 另一轮:#5 `(s1-main, acct-a)` 版本 44 改 `win`,`FakeSeq` 回 `CmdResult{InvalidArgs}`;之后发 #6 版本 44,`win` 与 #4 相同
- 期望:
  - 步骤 2 回 `busy`(`code = Busy`),不写环
  - 步骤 3 写环
  - 步骤 4 写环,`structural_changed = false`(基准已换成 #1)
  - 步骤 5 的 #6 `structural_changed = false`(#5 未 `Applied`,基准仍是 #4 之后的副本)
- 实现:`crates/hft-ctl/tests/set_params.rs::ctl_084_baseline_tracks_applied`

### CTL-085 Mailbox 未取走时下一个 SetParams 或 SetRisk 回 busy

- 规则:[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)
- 级别:table
- 里程碑:M2
- 前置:G0;`FakeSeq` 暂停(不取 Mailbox)
- 实现:`crates/hft-ctl/tests/set_params.rs::ctl_085_mailbox_single_slot`

| 行 | 步骤 | 期望 |
|---|---|---|
| CTL-085.1 | `SetParams` `(s1-main, acct-a)` | 写环,Mailbox 占用 |
| CTL-085.2 | 紧接 `SetParams` `(s2-main, acct-b)` | `busy`,不写环;审计 Note `result_code = 31` |
| CTL-085.3 | 紧接 `SetRisk` `account acct-b` | `busy` |
| CTL-085.4 | 紧接 `HedgeFix` | 写环(不用 Mailbox) |
| CTL-085.5 | `FakeSeq` 取走对象后再发 085.2 的请求 | 写环 |

### CTL-086 sequencer 交还的旧对象在 gateway 任务里释放

- 规则:[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M2
- 前置:G0;测试全局分配器按线程记录释放
- 步骤:
  1. `FakeSeq` 向 `retired` 推入 3 个 `ParsedConfig`(在 gateway 线程分配)
  2. 运行一次 `poll_once`
- 期望:
  - 3 个对象在 gateway 线程被释放(释放记录的线程 id = gateway 线程);`FakeSeq` 线程的释放计数为 0
- 实现:`crates/hft-ctl/tests/set_params.rs::ctl_086_retired_freed_in_gateway`

### CTL-090 GetSchema 由 gateway 本地应答,只写审计 Note

- 规则:[`GetSchema`(只读,本地应答)](README.md#getschema只读本地应答)、[control-plane 只读请求](../../docs/subsystems/control-plane.md#只读请求getschema)
- 级别:unit
- 里程碑:M2
- 前置:G0;issuer `cli:vera`(Viewer)
- 步骤:
  1. 发 `GetSchema{strategy_kind "probe-taker"}`
- 期望:
  - `status = ok`、`code = Applied`、没有 `cmd_hash`;`body` 与 SDK `params` 对 `probe-taker` 描述生成的 schema JSON 逐字节相同
  - ctl 环只有一条审计 Note(`cmd_kind = 0`、`result_code = 0`、`role = 1`、`chat_type` 码 3);没有 `ControlCmd`;journal 中没有 `CmdResult`
- 实现:`crates/hft-ctl/tests/schema.rs::ctl_090_get_schema_local`

### CTL-091 GetSchema 按 kind_id 查询与未知种类

- 规则:[`GetSchema`(只读,本地应答)](README.md#getschema只读本地应答)、[hft-ctl-proto 请求与响应](../hft-ctl-proto/README.md#请求与响应)
- 级别:table
- 里程碑:M2
- 前置:G0;issuer `cli:tom`
- 实现:`crates/hft-ctl/tests/schema.rs::ctl_091_get_schema_lookup`

| 行 | 请求 | 期望 |
|---|---|---|
| CTL-091.1 | `kind_id "65281"` | `body` 与 CTL-090 相同 |
| CTL-091.2 | `kind_id "65282"` | `probe-target` 的 schema |
| CTL-091.3 | `strategy_kind "nope"` | `InvalidArgs{UnknownName}` |
| CTL-091.4 | `kind_id "1"`(未编译进来) | `InvalidArgs{UnknownName}` |

### CTL-092 GetSchema 对全部白名单身份可用且不限速

- 规则:[`GetSchema`(只读,本地应答)](README.md#getschema只读本地应答)、[鉴权](README.md#鉴权)
- 级别:table
- 里程碑:M2
- 前置:G0;`cli:tom` 在最近 60 s 已计入 12 条写命令
- 实现:`crates/hft-ctl/tests/schema.rs::ctl_092_get_schema_access`

| 行 | issuer | 期望 |
|---|---|---|
| CTL-092.1 | `cli:tom`(写命令已限速) | `Applied` |
| CTL-092.2 | `cli:tom`,1 s 内连续 200 次 | 200 次都 `Applied` |
| CTL-092.3 | `tg:1001`(Viewer,群聊) | `Applied` |
| CTL-092.4 | `pipeline` | `Applied` |
| CTL-092.5 | `ops_bot` | `Forbidden` |
| CTL-092.6 | `reconciler` | `Forbidden` |

### CTL-093 GetSchema 的应答与 schema 导出文件逐字节相同

- 规则:[`GetSchema`(只读,本地应答)](README.md#getschema只读本地应答)
- 级别:unit
- 里程碑:M2
- 前置:G0;用 SDK 的 schema 导出函数(与 `hft-core schemas --out` 调用的同一函数)对 `TestStrategies` 导出到临时目录
- 步骤:
  1. 对 `TestStrategies` 的每个种类各发一次 `GetSchema`
- 期望:
  - 每个应答的 `body` 字节与导出目录中 `<kind_id>-<name>.schema.json` 的内容逐字节相同
- 实现:`crates/hft-ctl/tests/schema.rs::ctl_093_get_schema_matches_export`

### CTL-100 一个请求的全部记录一次写入或一条不写

- 规则:[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)
- 级别:table
- 里程碑:M2
- 前置:G0;请求为 CTL-082 的 `SetParams`(全部记录合计 `T` 字节,含审计 Note);测试让 ctl 环的剩余空间为列出的值
- 实现:`crates/hft-ctl/tests/ring.rs::ctl_100_all_or_nothing`

| 行 | 剩余空间 | 期望 |
|---|---|---|
| CTL-100.1 | `T` | 全部记录写入;响应在 `CmdResult` 到达后返回 |
| CTL-100.2 | `T − 1` | `busy`;环中没有任何新记录(含审计 Note);Mailbox 为空 |
| CTL-100.3 | `T + 1` | 全部记录写入 |

### CTL-101 审计 Note 的字段填法

- 规则:[鉴权](README.md#鉴权)、[hft-event Note 源表](../hft-event/README.md#控制与系统0x04xx0x05xx)
- 级别:table
- 里程碑:M2
- 前置:G0;每行断言该请求写出的那条审计 Note:`source = CtlAudit`(6)、`level = Info`(4);`v[2]` 按 `cmd_kind | result_code << 16 | role << 24 | view_account << 32 | chat_type << 40 | target_account << 48` 拆解;`v[3].to_le_bytes()` = issuer 前 8 字节;`v[4..7]` = `cmd_hash` 前 24 字节;`v[7]` = 对端 uid
- 实现:`crates/hft-ctl/tests/audit.rs::ctl_101_audit_note_fields`

| 行 | 请求 | 期望 |
|---|---|---|
| CTL-101.1 | `tg:1002` 私聊,`view_account acct-b`,`SetMode→Live{s1-main, g1}`,`account acct-a`,写环 | `cmd_kind = 0x0001`、`result_code = 0`、`role = 2`、`view_account = 2`、`chat_type = 1`、`target_account = 1`;`v[3]` = `"tg:1002\0"`;`v[0..2]` = `cmd_id`;`v[4..7]` = 响应里 `cmd_hash` 的前 24 字节;`v[7] = 1100`;`ControlCmd` 的 `CmdHdr.peer_uid = 1100` |
| CTL-101.2 | `tg:1001` 群聊,`SetMode→CloseOnly`(`Forbidden`) | `cmd_kind = 0x0001`、`result_code = 11`、`role = 1`、`chat_type = 2` |
| CTL-101.3 | 未知 uid 9999 | `cmd_id` 全零、`cmd_kind = 0`、`result_code = 10`、`role = 0`、`v[3] = 0`、`chat_type = 0`、`target_account = 0`、`v[4..7]` 全零、`v[7] = 9999` |
| CTL-101.4 | `cli:vera` 的 `GetSchema` | `cmd_kind = 0`、`result_code = 0`、`role = 1`、`chat_type = 3` |
| CTL-101.5 | `pipeline` 的 `SetParams`,写环 | `cmd_kind = 0x0002`、`role = 0`、`chat_type = 4` |
| CTL-101.6 | `ops_bot` 的默认参数 `Kill{Global}` | `role = 0`、`chat_type = 0`、`target_account = 0` |
| CTL-101.7 | 帧体不是 JSON | `cmd_id` 全零、`cmd_kind = 0`、`result_code = 20` |
| CTL-101.8 | Mailbox 占用时的 `SetParams`(`busy`) | `result_code = 31` |
| CTL-101.9 | `tg:1002` 私聊,`view_account "zzz"`(解析不出) | `view_account = 0`,请求照常处理 |
| CTL-101.10 | `cli:tom`(uid 1201)的 `Annotate{OpsNote}` | `v[7] = 1201` |
| CTL-101.11 | SIGTERM 路径经 `internal` 通道的 `cli:systemd` `Drain` | `v[7] = 0` |

### CTL-102 view_account 与 chat_type 只进审计 Note

- 规则:[鉴权](README.md#鉴权)
- 级别:unit
- 里程碑:M2
- 前置:G0;issuer `tg:1002`
- 步骤:
  1. 发 `HedgeFix{g1}`,`account acct-a`,`chat_type private`,无 `view_account`
  2. 再发内容相同(新 `cmd_id`)但 `view_account acct-b` 的请求
- 期望:
  - 两条 `ControlCmd` 除 `cmd_id` 外逐字节相同(含 `cmd_hash`、`CmdHdr.account = 1`)
  - 两条审计 Note 只在 `v[0..2]` 与 `v[2]` 的 `view_account` 位段不同
- 实现:`crates/hft-ctl/tests/audit.rs::ctl_102_view_account_audit_only`

### CTL-103 ReconcileReport 整组写入后回 Applied,不进去重缓存

- 规则:[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)、[control-plane 非命令输入](../../docs/subsystems/control-plane.md#非命令输入reconcilereport)
- 级别:unit
- 里程碑:M2
- 前置:G0;issuer `reconciler`
- 步骤:
  1. 发 3 个条目的 `ReconcileReport{report_id "7"}`
  2. 以同一 `cmd_id` 原样重发
- 期望:
  - 步骤 1:环中依序 3 条 `ReconcileReport`(`idx` 0、1、2,`count` 3)与一条审计 Note(`cmd_kind = 0`、`result_code = 0`);响应 `status = ok`、`code = Applied`、没有 `cmd_hash`,不等待 journal
  - 步骤 2:再写 3 条(不查去重缓存);响应同步骤 1
- 实现:`crates/hft-ctl/tests/ring.rs::ctl_103_reconcile_report`

### CTL-104 ReconcileReport 放不下时 busy、一条不写

- 规则:[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)
- 级别:unit
- 里程碑:M2
- 前置:G0;issuer `reconciler`;10 个条目合计 `T` 字节;环剩余 `T − 1` 字节
- 步骤:
  1. 发该报告
- 期望:
  - `busy`;环中没有新的 `ReconcileReport` 与审计 Note
- 实现:`crates/hft-ctl/tests/ring.rs::ctl_104_reconcile_report_busy`

### CTL-110 读到 CmdResult 后回 ok 并关闭

- 规则:[CmdResult 回传](README.md#cmdresult-回传)、[hft-ctl-proto 请求与响应](../hft-ctl-proto/README.md#请求与响应)
- 级别:unit
- 里程碑:M2
- 前置:G0;`FakeSeq` 把 `ControlCmd` 记为 seq 900,1 ms 后写 `CmdResult{cmd_id, cmd_seq = 900, status = Applied, ctl_state_version = 21, engine_epoch = 17}`
- 步骤:
  1. `cli:tom` 发 `HedgeFix{g1}`,`account acct-a`
- 期望:
  - 响应 `status = ok`、`code = Applied`、`seq = 900`、`ctl_state_version = 21`、`engine.epoch = 17`、`cmd_hash` = 该命令的 `cmd_hash`;随后连接关闭
  - 挂起表为空
- 实现:`crates/hft-ctl/tests/result.rs::ctl_110_result_returned`

### CTL-111 result_wait_ms 到期回 accepted,结果到达后进缓存

- 规则:[CmdResult 回传](README.md#cmdresult-回传)
- 级别:table
- 里程碑:M2
- 前置:G0;`result_wait_ms = 5000`;请求在 t = 0 到达并写环;`FakeSeq` 在列出的时刻写 `CmdResult{Applied}`
- 实现:`crates/hft-ctl/tests/result.rs::ctl_111_accepted_after_wait`

| 行 | `CmdResult` 被尾随读到的时刻 | 期望 |
|---|---|---|
| CTL-111.1 | 4 999 ms | `status = ok`、`code = Applied` |
| CTL-111.2 | 5 000 ms 的 `poll_once` 前仍未读到 | t = 5 000 ms 的 `poll_once` 回 `status = accepted`、无 `code` 并关闭 |
| CTL-111.3 | 5 001 ms(已回 `accepted`) | 结果存入去重缓存;之后同一 `cmd_id` 的重发(`result()`)回 `ok`、`Applied`,不写环 |
| CTL-111.4 | 111.2 之后、结果未到前重发同一 `cmd_id` | `accepted`,不写环 |

### CTL-112 重复 cmd_id 的四种情形

- 规则:[CmdResult 回传](README.md#cmdresult-回传)
- 级别:table
- 里程碑:M2
- 前置:G0;issuer `cli:tom`;请求 `HedgeFix{g1}`,`account acct-a`
- 实现:`crates/hft-ctl/tests/result.rs::ctl_112_duplicate_cmd_id`

| 行 | 情形 | 期望 |
|---|---|---|
| CTL-112.1 | 第一次得到 `Applied` 后原样重发 | 回缓存的 `Applied`(同一 `seq`),环中没有第二条 `ControlCmd` |
| CTL-112.2 | 第一次已写环、结果未到时重发 | `accepted`,不写环 |
| CTL-112.3 | 第一次因 `account "nope"` 被 `InvalidArgs{UnknownName}`,改成 `account acct-a`、同一 `cmd_id` 重发 | 重新判定:写环 |
| CTL-112.4 | 第一次 `Applied` 后重建 gateway(缓存清空)再重发 | 写环;`FakeSeq` 按其去重表回 `CmdResult{Duplicate}`,响应 `code = Duplicate` |

### CTL-113 去重缓存满时淘汰最早登记的一项

- 规则:[CmdResult 回传](README.md#cmdresult-回传)
- 级别:table
- 里程碑:M2
- 前置:G0,但 `dedup_capacity = 4`;命令 c1…c5 依次写环并都得到 `Applied`
- 实现:`crates/hft-ctl/tests/result.rs::ctl_113_dedup_eviction`

| 行 | 动作 | 期望 |
|---|---|---|
| CTL-113.1 | 只发 c1…c4 后重发 c1 | 回缓存结果,不写环 |
| CTL-113.2 | 发完 c1…c5 后重发 c1 | 写环(c1 已被淘汰),`FakeSeq` 回 `Duplicate` |
| CTL-113.3 | 发完 c1…c5 后重发 c2 | 回缓存结果,不写环 |

### CTL-114 同一 cmd_id 从两个连接并发到达只写一条命令

- 规则:[CmdResult 回传](README.md#cmdresult-回传)
- 级别:unit
- 里程碑:M2
- 前置:G0;两个 `cli:tom` 连接在同一次 `poll_once` 内送达同一 `cmd_id` 的同一请求
- 步骤:
  1. 运行 `poll_once` 直到两个连接都关闭
- 期望:
  - 环中恰有一条 `ControlCmd`;先被处理的连接得到 `ok`、`Applied`,另一个得到 `accepted`(或在其处理时结果已到则为 `ok`、`Applied`);`FakeSeq` 只收到一次
- 实现:`crates/hft-ctl/tests/result.rs::ctl_114_concurrent_same_cmd_id`

### CTL-115 到达时已过期回 Expired,不写环

- 规则:[线协议](README.md#线协议)、[control-plane 过期](../../docs/subsystems/control-plane.md#过期)
- 级别:table
- 里程碑:M2
- 前置:G0;请求的 `issued_at_ns = 1790000000000000000`、`expires_at_ns = 1790000030000000000`;gateway 的 UTC 按行设定
- 实现:`crates/hft-ctl/tests/expiry.rs::ctl_115_expired_on_arrival`

| 行 | 到达时 gateway UTC | 期望 |
|---|---|---|
| CTL-115.1 | `expires_at_ns − 1` | 写环,`CmdHdr.expires_tsc` = `tsc(expires_at_ns)` = 30 000 000 000 |
| CTL-115.2 | `expires_at_ns` | 写环(相等不算过期) |
| CTL-115.3 | `expires_at_ns + 1` | `status = ok`、`code = Expired`;不写 `ControlCmd`;审计 Note `result_code = 3` |

### CTL-120 preview、confirmed 与 expect_state_version 原样进 CmdHdr

- 规则:[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)
- 级别:table
- 里程碑:M2
- 前置:G0;issuer `tg:1002`(Trader)
- 实现:`crates/hft-ctl/tests/passthrough.rs::ctl_120_confirm_fields_passthrough`

| 行 | 请求的确认字段 | 期望 `CmdHdr` |
|---|---|---|
| CTL-120.1 | `preview = true` | `hflags = 1`、`confirmed = 0`、`expect_state_version = 0` |
| CTL-120.2 | `confirmed = true`、`expect_state_version = 12` | `hflags = 0`、`confirmed = 1`、`expect_state_version = 12` |
| CTL-120.3 | 都不带 | `hflags = 0`、`confirmed = 0`、`expect_state_version = 0` |
| CTL-120.4 | `confirmed = false`、`expect_state_version = 5` | `confirmed = 0`、`expect_state_version = 5` |

### CTL-121 gateway 不因 sequencer 状态拒绝任何命令

- 规则:[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)、[概要](README.md#概要)
- 级别:prop
- 里程碑:M2
- 性质:对任意引擎状态(闸门未放行、Draining、HARD、各种 cap、`ctl_state_version` 任意值),通过 gateway 静态检查的同一请求得到同样的 gateway 处理结果(写环或同样的 gateway 拒绝码);状态类的拒绝(`NeedsConfirm`、`ConfirmInvalid`、`PreconditionFailed`、`Rejected{RefdataRecheck}` 等)只出现在 `CmdResult` 里
- 生成:`E2E`;引擎状态从上述集合随机组合;请求从全部 core 命令的合法请求中抽取
- 参考:同一请求在 `S0` 下的 gateway 处理结果
- 例数:PR 200
- 实现:`crates/hft-ctl/tests/prop.rs::ctl_121_gateway_state_free`

### CTL-130 Rust 客户端 send 发一帧收一帧

- 规则:[客户端](README.md#客户端)
- 级别:unit
- 里程碑:M2
- 前置:G0;`CtlClient{socket = G0 的 socket, issuer = "cli:tom"}`(测试以 uid 1201 连接)
- 步骤:
  1. `send(&HedgeFix 请求, 1 s)`
  2. 让 gateway 停止 `poll`,再 `send(&同类新请求, 200 ms)`
- 期望:
  - 步骤 1 返回 `Ok(CmdResponse{status: ok, code: Some(Applied), …})`;线上恰好一帧请求与一帧响应
  - 步骤 2 在 200 ms 后返回 `Err(ClientError::Timeout)`
- 实现:`crates/hft-ctl/tests/client.rs::ctl_130_send`

### CTL-131 preview 带 PREVIEW 标志,确认用新 cmd_id、同一命令体

- 规则:[客户端](README.md#客户端)、[control-plane 版本与确认](../../docs/subsystems/control-plane.md#版本与确认)
- 级别:unit
- 里程碑:M2
- 前置:G0;`FakeSeq` 对预览回 `CmdResult{Applied, rflags = PREVIEW | NEEDS_CONFIRM, ctl_state_version = 33}`
- 步骤:
  1. `preview(&SetMode→Live 请求)`
  2. `confirm(&同一请求, Some(33))`
  3. 以 Viewer `CtlClient{issuer = "tg:1001"}` 对默认参数 `Kill{Account(acct-a)}` 先 `preview`,再 `confirm(&同一请求, None)`
- 期望:
  - 步骤 1 发出的请求 `preview = true`、无 `confirmed`;返回值带 `ctl_state_version = 33` 与 `preview`
  - 步骤 2 发出的请求 `cmd_id` 与步骤 1 不同,`cmd` 逐字节相同,`confirmed = true`、`expect_state_version = 33`;两次响应的 `cmd_hash` 相同
  - 步骤 3 的确认请求 `confirmed = true`,不带 `expect_state_version`
- 实现:`crates/hft-ctl/tests/client.rs::ctl_131_preview_confirm`

### CTL-132 result 以同一 cmd_id 与原请求体重发

- 规则:[客户端](README.md#客户端)、[CmdResult 回传](README.md#cmdresult-回传)
- 级别:table
- 里程碑:M2
- 前置:G0;`FakeSeq` 在 6 s 后才写 `CmdResult{Applied}`
- 实现:`crates/hft-ctl/tests/client.rs::ctl_132_result_resends`

| 行 | 动作 | 期望 |
|---|---|---|
| CTL-132.1 | `send` 得到 `accepted` 后 1 s 调 `result(cmd_id)` | 发出的帧与原请求逐字节相同;返回 `accepted` |
| CTL-132.2 | `CmdResult` 到达后调 `result(cmd_id)` | 返回 `ok`、`Applied`;环中只有一条 `ControlCmd` |
| CTL-132.3 | `result(未发过的 cmd_id)` | `Err(ClientError::UnknownCmdId)`,没有建立连接 |

### CTL-133 get_schema 返回 schema JSON

- 规则:[客户端](README.md#客户端)
- 级别:unit
- 里程碑:M2
- 前置:G0;`CtlClient{issuer = "cli:vera"}`
- 步骤:
  1. `get_schema("probe-taker")`
  2. `get_schema("nope")`
- 期望:
  - 步骤 1 返回的 `serde_json::Value` 等于 CTL-090 的 `body`
  - 步骤 2 返回 `Err`,其中带 `InvalidArgs{UnknownName}`
- 实现:`crates/hft-ctl/tests/client.rs::ctl_133_get_schema`

### CTL-134 inject_warmup 按块列表逐块发送

- 规则:[客户端](README.md#客户端)、[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)
- 级别:unit
- 里程碑:M2
- 前置:G0;`CtlClient{issuer = "pipeline"}`;块列表 = 600 块,第 k 块长度 `1 + (k × 7) mod 4096` 字节、内容为 k 的伪随机字节;`warmup_rate_per_s = 50`;gateway 让第 100、101 块各回一次 `busy`
- 步骤:
  1. `inject_warmup("s2-main", 3, &chunks)`
- 期望:
  - 返回 `Ok(600)`
  - 环中 600 条 `ControlCmd{InjectWarmup}`,`chunk_no` 为 0…599 连续、只有第 599 条 `last = 1`、`local_kind = 3`;第 k 条的载荷与输入第 k 块逐字节相同
  - 被 `busy` 或 `RateLimited` 的块以同一 `cmd_id` 重试(环中没有重复的 `chunk_no`);写命令限速计数为 0
- 实现:`crates/hft-ctl/tests/client.rs::ctl_134_inject_warmup_chunks`

### CTL-135 inject_warmup 有一块超过 4 096 字节时一块都不发

- 规则:[客户端](README.md#客户端)
- 级别:table
- 里程碑:M2
- 前置:G0;`CtlClient{issuer = "pipeline"}`
- 实现:`crates/hft-ctl/tests/client.rs::ctl_135_chunk_too_long`

| 行 | 块长度列表 | 期望 |
|---|---|---|
| CTL-135.1 | [100, 4 097] | `Err(ClientError::ChunkTooLong)`;没有建立任何连接,环中没有记录 |
| CTL-135.2 | [4 096] | `Ok(1)` |
| CTL-135.3 | [4 095, 4 096] | `Ok(2)` |

### CTL-136 inject_warmup 某块被拒即停止并返回已确认的块数

- 规则:[客户端](README.md#客户端)
- 级别:unit
- 里程碑:M2
- 前置:G0;`FakeSeq` 对第 3 块(`chunk_no = 3`)回 `CmdResult{InvalidArgs{UnknownName}}`,其余 `Applied`
- 步骤:
  1. `inject_warmup("s2-main", 3, &10 块)`
- 期望:
  - 返回 `Ok(3)`;环中只有 `chunk_no` 0…3 四条,第 4–9 块没有发出
- 实现:`crates/hft-ctl/tests/client.rs::ctl_136_stop_on_rejected_chunk`

### CTL-140 Python 客户端只用标准库

- 规则:[客户端](README.md#客户端)
- 级别:unit
- 里程碑:M4
- 前置:只装 Python 标准库的干净 venv;`crates/hft-ctl/clients/python/hftctl.py`
- 步骤:
  1. 在该 venv 中 `python -m pytest crates/hft-ctl/clients/python`
  2. 用 `modulefinder` 列出 `hftctl.py` 导入的模块
- 期望:
  - 步骤 1 全部通过
  - 步骤 2 的顶层模块 ⊆ {`socket`、`struct`、`json`、`uuid`、`time`、`base64`} 与它们的标准库依赖;不出现 `hft_py`、`pyo3` 或任何第三方包
- 实现:`crates/hft-ctl/clients/python/test_hftctl.py::ctl_140_stdlib_only`

### CTL-141 Python 客户端对金样帧逐字节一致

- 规则:[客户端](README.md#客户端)、[hft-ctl-proto 请求与响应](../hft-ctl-proto/README.md#请求与响应)
- 级别:golden
- 里程碑:M4
- 输入:`fixtures/ctl/*.request.json` 对应的方法调用(每个金样一条,`cmd_id`、时刻固定)
- 期望文件:`fixtures/ctl/frames/<同名>.frame.bin`(与 Rust 侧 CTP-162 同一组文件)
- 实现:`crates/hft-ctl/clients/python/test_hftctl.py::ctl_141_golden_frames`

### CTL-142 Python 客户端的 preview、confirm、result、get_schema 与 inject_warmup 与 Rust 语义相同

- 规则:[客户端](README.md#客户端)
- 级别:table
- 里程碑:M4
- 前置:G0 的 gateway 以独立进程运行,Python 客户端连接它
- 实现:`crates/hft-ctl/clients/python/test_hftctl.py::ctl_142_python_semantics`

| 行 | 场景 | 期望 |
|---|---|---|
| CTL-142.1 | 同 CTL-131 | 与 CTL-131 的期望相同(`confirm(state_version=None)` 不带 `expect_state_version`) |
| CTL-142.2 | 同 CTL-132 | 与 CTL-132 相同;未发过的 `cmd_id` 抛出 `UnknownCmdId` |
| CTL-142.3 | 同 CTL-133 | 与 CTL-133 相同 |
| CTL-142.4 | 同 CTL-134(块数 60) | 返回 60,`chunk_no` 连续、末块 `last = 1` |
| CTL-142.5 | 同 CTL-135.1 | 抛出 `ChunkTooLong`,一块都不发 |
| CTL-142.6 | 同 CTL-136 | 返回 3 |

### CTL-150 请求到达到写入 ctl 环的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`ctl/ingest`
- 前置:G0;`HedgeFix` 请求,uid 与 issuer 已在表中
- 实现:`crates/hft-ctl/benches/ingest.rs::ctl_150_ingest`

### CTL-151 500 个 group 的 SetParams 到达到写环的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`ctl/ingest_setparams_500`
- 前置:G0;500 个 group 的合法 SPB1,含 body 解码与 `validate`
- 实现:`crates/hft-ctl/benches/ingest.rs::ctl_151_ingest_setparams_500`

### CTL-152 GetSchema 的应答耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`ctl/get_schema`
- 前置:G0;`probe-target` 的 schema
- 实现:`crates/hft-ctl/benches/ingest.rs::ctl_152_get_schema`

### CTL-153 4 KiB InjectWarmup 块到达到写环的耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`ctl/ingest_warmup_4k`
- 前置:G0;4 096 字节载荷
- 实现:`crates/hft-ctl/benches/ingest.rs::ctl_153_ingest_warmup_4k`

### CTL-154 写入 ctl 环的命令在下一轮 step 内被处理

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:integ
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`;sequencer 的 ctl 每轮上限 4 条
- 步骤:
  1. gateway 写入 1 条 `HedgeFix` 后记下写环时的 TSC `t_w`
  2. 调用一次 `Sequencer::step`
- 期望:
  - 步骤 2 之后 journal 中有该 `ControlCmd` 与它的 `CmdResult`;`ControlCmd` 记录头 `tsc − t_w` 不大于这一次 `step` 的时长;gateway 尾随时把两者配对写进自身统计
- 实现:`crates/hft-ctl/tests/e2e/latency.rs::ctl_154_processed_next_step`

### CTL-155 客户端发出到收到 ok 的端到端耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:integ
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`,sequencer 以 `poll` 驱动连续运行;1 000 条 `HedgeFix`(不同 `cmd_id`)
- 步骤:
  1. 客户端依次 `send`,记录每条从发出到收到 `ok` 的墙钟耗时
- 期望:
  - 1 000 条都 `ok`;p99 不超过 [延迟与资源契约](README.md#延迟与资源契约) 的端到端目标
- 实现:`crates/hft-ctl/tests/e2e/latency.rs::ctl_155_end_to_end_p99`

### CTL-156 gateway 退出不影响交易

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:integ
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`;`probe-taker` 每 100 ms 产生一次两腿开仓意图
- 步骤:
  1. 让 gateway 任务 panic 并退出
  2. 继续驱动 10 s 虚拟时间的行情与回报
- 期望:
  - sequencer 每轮照常 `step`,订单照常出环与成交;journal 中这 10 s 内的 `OrderDecision` 数与不退出 gateway 的同种子对照运行相同
  - 客户端连接 socket 得到 `ECONNREFUSED` 或超时
- 实现:`crates/hft-ctl/tests/e2e/latency.rs::ctl_156_gateway_down_trading_continues`

### CTL-160 Gateway 对策略集泛型,新增测试策略不改本 crate

- 规则:[概要](README.md#概要)
- 级别:unit
- 里程碑:M2
- 前置:hft-testkit 的 `TestStrategies`;测试内以 SDK 的 `strategy_set!` 只装 `probe-taker` 生成第二个策略集 `TestStrategiesMin`
- 步骤:
  1. 分别以两个策略集实例化 `Gateway<S>`,各发一次 `GetSchema` 与一条合法 `SetParams`
- 期望:
  - 两者都通过;`TestStrategiesMin` 下 `GetSchema{kind_id "65282"}` 回 `InvalidArgs{UnknownName}`
  - 解耦门槛(新增第二个测试策略时本 crate 的 diff 为零)由 `cargo xtask scaffold-gate` 判定,本用例只证明本 crate 不引用任何具体策略类型
- 实现:`crates/hft-ctl/tests/generic.rs::ctl_160_generic_over_strategy_set`

### CTL-161 依赖闭包不含 tokio、策略包与注册表

- 规则:[依赖](README.md#依赖)
- 级别:unit
- 里程碑:M2
- 前置:`tools/xtask/deps.toml`
- 步骤:
  1. `cargo xtask deps` 检查 `hft-ctl` 的依赖闭包(含 dev 依赖)
- 期望:
  - 闭包中没有 `tokio`、`hft-strategy-registry` 与任何 `strategies/*`;没有 hot crate 反向依赖本 crate
  - 源码中 `hft_sequencer::` 的引用只有 `ParsedConfig`(xtask 的源码扫描)
- 实现:`tools/xtask/tests/deps_ctl.rs::ctl_161_ctl_dep_closure`

### CTL-200 SetMode 经客户端的每种结果码

- 规则:[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)、[control-plane 类别](../../docs/subsystems/control-plane.md#类别)、[control-plane 版本与确认](../../docs/subsystems/control-plane.md#版本与确认)、[control-plane 账户语义](../../docs/subsystems/control-plane.md#账户语义)、[gateway(warm 任务,`poll_once` 形态)](README.md#gatewaywarm-任务poll_once-形态)
- 级别:table
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`;issuer `cli:tom`;`g1` 有 0.01 BTC 永续持仓;每行从 `S0` 起,另列出的状态先行设好;"两步确认"见基线
- 实现:`crates/hft-ctl/tests/e2e/set_mode.rs::ctl_200_set_mode_codes`
- 备注:`account = *` 的升级在任一账户闸门未放行时整体被拒由 MD-9 覆盖,本表不重复

| 行 | 状态 / 请求 | 期望 |
|---|---|---|
| CTL-200.1 | `SetMode→CloseOnly{s1-main, g1}`,`acct-a` | `ok`、`Applied`;该 cell 有效模式 CloseOnly;`ctl_state_version = V0 + 1` |
| CTL-200.2 | 200.1 之后再发同一请求(新 `cmd_id`) | `NoOp`;`ctl_state_version` 不变 |
| CTL-200.3 | `SetMode→Shadow{s1-main, g1}` | `Applied`,`detail = ShadowDeferred`;有效模式 CloseOnly |
| CTL-200.4 | 先 200.1;`SetMode→Live{s1-main, g1}`,不带确认字段 | `NeedsConfirm`;期望模式仍为 CloseOnly |
| CTL-200.5 | 先 200.1;同上两步确认 | 预览 `Applied`(`rflags` 含 `PREVIEW`、`NEEDS_CONFIRM`);确认 `Applied`,有效模式 Live |
| CTL-200.6 | 先 200.1;预览后另发 `Kill{Account(acct-b)}`(版本 +1),再以预览的版本确认 | `ConfirmInvalid`,`detail = StateChanged`;期望模式不变 |
| CTL-200.7 | `acct-a` 启动闸门未放行;两步确认 `SetMode→Live{s1-main, g1}` | 预览与确认都回 `PreconditionFailed`,`detail = GateNotReleased` |
| CTL-200.8 | 先 `Drain{cancel_resting false}`;两步确认 `SetMode→Live` | `PreconditionFailed`,`detail = Draining` |
| CTL-200.9 | 先 `Drain{cancel_resting false}`;`SetMode→CloseOnly{s1-main, g1}` | `Applied` |

### CTL-201 SetParams 经客户端的每种结果码

- 规则:[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)、[control-plane 类别](../../docs/subsystems/control-plane.md#类别)、[control-plane 结果码](../../docs/subsystems/control-plane.md#结果码)
- 级别:table
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`;`(s1-main, acct-a)` 生效版本 41,所用测试种类有结构字段 `win`、非结构字段 `thr` 与每个 group 的 `open_allowed`;`g1` 的 `open_allowed = 1` 且有 0.01 BTC 持仓;`max_cells_total = 32`;新 blob 版本为 42(另注明的除外);issuer `cli:tom`(另注明的除外)
- 实现:`crates/hft-ctl/tests/e2e/set_params.rs::ctl_201_set_params_codes`
- 备注:人工主体与 pipeline 主体对 `open_allowed` 0→1、新增 group 与 Live 实例结构字段的确认判定由 MD-7(第 3–6 行)覆盖,本表只补其余结果码与 Live 实例定义的边界

| 行 | 状态 / 请求 | 期望 |
|---|---|---|
| CTL-201.1 | 只改 `thr`,不确认 | `Applied`;`CmdResult.params_version = 42` |
| CTL-201.2 | 版本 41 | `StaleVersion` |
| CTL-201.3 | `base_version "40"` | `VersionConflict` |
| CTL-201.4 | 改 `win`,不确认 | `NeedsConfirm`;预览的 `preview.reset_groups` 与 `no_open_ms` 等于 SDK `reset_preview` 的估算 |
| CTL-201.5 | 先把 `s1-main` 的全部 cell `SetMode→Shadow`(落定后);`pipeline` 改 `win` | `Applied`(不是 Live 实例) |
| CTL-201.6 | `pipeline` 对 `(s9-sim, sim-a)` 改 `win` | `Applied`(Sim 账户上的实例不是 Live 实例) |
| CTL-201.7 | 移除 `g1` | `PreconditionFailed`,`detail = GroupHasPosition`;旧参数生效 |
| CTL-201.8 | 新增一个与 `g1` 同为 `(acct-a, binance-usdm:BTCUSDT)` 的非 Shadow group | `InvalidArgs{InstOwned}` |
| CTL-201.9 | 新增 group 两腿 lot 0.004 与 0.01,参考价使 `pair_lot` 名义超过 `acct-a` 的 `force_slice.max_chunk_notional` | `InvalidArgs{LotMismatch}` |
| CTL-201.10 | 新增 group 的交易腿 `min_notional` 高于 `acct-a` 的 `dust_notional` | `InvalidArgs{DustBelowMinNotional}` |
| CTL-201.11 | 新参数使全部实例的 `model_mem` 之和超过 `advisor_mem_budget_mib`(单实例不超) | `InvalidArgs{ParamInvalid}`,`arg = "ModelMemBudget"`(sequencer 产生) |
| CTL-201.12 | 新增 group 使全进程 cell 数达到 33 | `CapacityExceeded` |
| CTL-201.13 | 策略 `on_params` 返回 `Err` | `InvalidArgs{ParamInvalid}`,`arg = "Strategy"`;旧参数生效 |
| CTL-201.14 | 先 `Drain{cancel_resting false}`;新增 `open_allowed = 1` 的 group(人工主体,升级),两步确认 | `PreconditionFailed{Draining}` |
| CTL-201.15 | 先 `Drain{cancel_resting false}`;201.1 | `Applied` |

### CTL-202 SetRisk 经客户端的每种结果码

- 规则:[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)、[control-plane 类别](../../docs/subsystems/control-plane.md#类别)
- 级别:table
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`;`RiskParams` 当前版本 7,新文档版本 8;`acct-a` 的 `force_slice.max_chunk_notional = 200 USDT`;issuer `cli:alice`
- 实现:`crates/hft-ctl/tests/e2e/set_risk.rs::ctl_202_set_risk_codes`

| 行 | 状态 / 请求 | 期望 |
|---|---|---|
| CTL-202.1 | `acct-a` 的 `max_order_notional` 由 1 000 降到 500 USDT,不确认 | `Applied` |
| CTL-202.2 | 由 1 000 升到 2 000 USDT,不确认 | `NeedsConfirm` |
| CTL-202.3 | 202.2 两步确认 | `Applied` |
| CTL-202.4 | 降到 199 USDT(低于 `max_chunk_notional`) | `InvalidArgs{ExecRiskMismatch}`;版本 7 继续生效 |
| CTL-202.5 | 新文档版本 7 | `StaleVersion` |
| CTL-202.6 | 先 `Drain{cancel_resting false}`;202.2 两步确认 | `PreconditionFailed{Draining}` |
| CTL-202.7 | 先 `Drain{cancel_resting false}`;202.1 | `Applied` |

### CTL-203 Kill 经客户端的每种参数组合

- 规则:[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)、[control-plane 模式与上限的统一优先级](../../docs/subsystems/control-plane.md#模式与上限的统一优先级)
- 级别:table
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`;`acct-a` 的 `g1` 有一条第一腿已成交、第二腿待发的链接;`acct-b` 的 `g2` 同样;issuer `cli:tom`(另注明的除外)
- 实现:`crates/hft-ctl/tests/e2e/kill.rs::ctl_203_kill_codes`
- 备注:`hard` 的组合、账户级 HARD 不影响其他账户与 Sim 账户的 Kill 由 MD-10 覆盖,本表不重复

| 行 | 状态 / 请求 | 期望 |
|---|---|---|
| CTL-203.1 | `Kill{Account(acct-a)}` | `Applied`;`acct-a` 的 Kill cap;之后 `acct-a` 的 Open 意图被拒、第二腿对冲单照常出环;`acct-b` 不受影响 |
| CTL-203.2 | `Kill{Account(acct-a), flatten}`,不确认 | `NeedsConfirm`;无 cap |
| CTL-203.3 | 203.2 两步确认 | `Applied`;清仓锁;执行层减仓单出环 |
| CTL-203.4 | 先 `Drain{cancel_resting false}`;203.3 | `Applied` |
| CTL-203.5 | `acct-a` 启动闸门未放行;203.3 | `Applied` |
| CTL-203.6 | 先 `Drain{cancel_resting false}`;`Kill{Global}` | `Applied` |
| CTL-203.7 | issuer `ops_bot`,`Kill{Account(acct-a)}`,不带确认字段 | `Applied` |
| CTL-203.8 | 全部账户启动闸门未放行;`Kill{Global}` | `Applied` |

### CTL-204 Resume 经客户端的每种结果码

- 规则:[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)、[control-plane 结果码](../../docs/subsystems/control-plane.md#结果码)、[control-plane 模式与上限的统一优先级](../../docs/subsystems/control-plane.md#模式与上限的统一优先级)
- 级别:table
- 里程碑:M2
- 前置:`E2E` 稳态 `S0` 加各行所列的 cap;issuer `cli:tom`;除第 1 行外都两步确认;`daily_loss_max_resumes = 1`
- 实现:`crates/hft-ctl/tests/e2e/resume.rs::ctl_204_resume_codes`

| 行 | 状态 / 请求 | 期望 |
|---|---|---|
| CTL-204.1 | `acct-a` 有 Kill cap;`Resume{Kill}`,`acct-a`,不确认 | `NeedsConfirm` |
| CTL-204.2 | 同上,两步确认 | `Applied`;Kill cap 删除 |
| CTL-204.3 | `acct-b` 没有 Kill cap;`Resume{Kill}`,`acct-b` | `PreconditionFailed{NoMatchingCap}` |
| CTL-204.4 | 有 `Kill{Global}`;`Resume{Kill}`,`account *` | `Applied` |
| CTL-204.5 | `Resume{ClockBad}`,`account *` | `InvalidArgs{NotResumable}` |
| CTL-204.6 | `Resume{Draining}`,`account *` | `InvalidArgs{NotResumable}` |
| CTL-204.7 | `acct-a` 有 `Broken(g1)` 与 `HedgeFailure`;`Resume{HedgeFailure}` | `Rejected`,`detail = BrokenRemaining` |
| CTL-204.8 | `g1` `Broken` 且已成交裸量高于尘埃线;`Resume{Broken, group g1}` | `PreconditionFailed{NakedNotCleared}` |
| CTL-204.9 | `acct-a` 当日已 `Resume{DailyLoss}` 1 次后再越线;`Resume{DailyLoss}` | `PreconditionFailed{ResumeLimit}` |
| CTL-204.10 | `Recon(acct-a, bitget-spot)` 且最近一轮对账仍有差异;`Resume{Recon, venue bitget-spot}` | `PreconditionFailed{ReconNotClean}` |
| CTL-204.11 | `UnknownTimeout(acct-a, bitget-spot)` 且仍有 Unknown 槽;`Resume{UnknownTimeout, venue bitget-spot}` | `PreconditionFailed{UnknownPresent}` |
| CTL-204.12 | `acct-a` 有 Kill cap;先 `Drain{cancel_resting false}`;`Resume{Kill}` | `PreconditionFailed{Draining}` |
| CTL-204.13 | `acct-a` 启动闸门未放行、有 `Recovery`;`Resume{Recovery}` | `PreconditionFailed{GateNotReleased}` |
| CTL-204.14 | 有 `Kill{Account(acct-a), hard}`,其后新一轮对账含非 `Ignore` 差异;`Resume{Kill}`,`acct-a` | `PreconditionFailed{ReconNotClean}` |
| CTL-204.15 | issuer `tg:1001`(Viewer);`Resume{Kill}` | `Forbidden`(gateway),不写环 |

### CTL-205 Viewer 紧急停止的预览与确认

- 规则:[control-plane 版本与确认](../../docs/subsystems/control-plane.md#版本与确认)、[control-plane 角色](../../docs/subsystems/control-plane.md#角色)、[鉴权](README.md#鉴权)
- 级别:table
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`;`acct-a` 有 2 个 Open 挂单与一条第一腿已成交、第二腿待发的链接;请求为 `tg:1001`(Viewer)的默认参数 `Kill{Account(acct-a)}`;"预览"在虚拟时刻 T 发出,确认不带 `expect_state_version`
- 实现:`crates/hft-ctl/tests/e2e/viewer_stop.rs::ctl_205_viewer_emergency_stop`
- 备注:确认绑定的主体规则(他人确认、作用域不同、重复确认、重启失效、60 001 ms 过期、Viewer 的越权命令)由 MD-11 覆盖;本表只补客户端可见的预览渲染、TTL 两侧边界与预览表容量

| 行 | 状态 / 请求 | 期望 |
|---|---|---|
| CTL-205.1 | 预览 | `ok`、`Applied`;`rflags = PREVIEW \| NEEDS_CONFIRM \| PREVIEW_TTL`;`preview.confirm_within_s = 60`;无 Kill cap;`ctl_state_version` 不变 |
| CTL-205.2 | T + 60 000 ms 确认 | `Applied` |
| CTL-205.3 | T + 59 999 ms 确认 | `Applied` |
| CTL-205.4 | `tg:1001` 预览后 `tg:1005` 预览 16 次 `Kill{Global}`,`tg:1001` 再确认 | `ConfirmInvalid{NoValidPreview}`(最旧一条被淘汰) |

### CTL-206 HedgeFix 经客户端的结果

- 规则:[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)
- 级别:table
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`;`g1` `Broken`,永续腿多出 0.002 BTC;issuer `cli:tom`
- 实现:`crates/hft-ctl/tests/e2e/hedge_fix.rs::ctl_206_hedge_fix`

| 行 | 状态 / 请求 | 期望 |
|---|---|---|
| CTL-206.1 | `HedgeFix{g1}`,`acct-a`,不确认 | `Applied`;同一事件内一条带 `RISK_REDUCING` 的 `OrderDecision`,平多出的永续腿 0.002 BTC |
| CTL-206.2 | 先 `Drain{cancel_resting false}`;同上 | `Applied` |
| CTL-206.3 | `acct-a` 启动闸门未放行;同上 | `Applied` |

### CTL-207 WriteOffResidual 经客户端的每种结果码

- 规则:[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)、[control-plane 结果码](../../docs/subsystems/control-plane.md#结果码)
- 级别:table
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`;`g1` 静止,永续腿余数 0.0004 BTC(低于 lot 0.001、高于尘埃线);issuer `cli:alice`;除第 1 行外两步确认
- 实现:`crates/hft-ctl/tests/e2e/write_off.rs::ctl_207_write_off_codes`

| 行 | 状态 / 请求 | 期望 |
|---|---|---|
| CTL-207.1 | `WriteOffResidual{g1}`,`acct-a`,不确认 | `NeedsConfirm` |
| CTL-207.2 | 两步确认 | `Applied`;该 group 全部腿 `residual = 0`;ledger 一条 `AdjustDelta::WriteOff`(0.0004 BTC 移到 unassigned);`Note{ResidualWrittenOff}`;无订单出环 |
| CTL-207.3 | `g1` 处于 `Broken` 时两步确认 | `Applied`;同一 Input 内 `Broken` 自动解除,`HedgeFailure` 保留 |
| CTL-207.4 | `g1` 有在途切片 | `PreconditionFailed{NotQuiescent}` |
| CTL-207.5 | group `"16778215"`(`s1-main` 的计数器 999,不在当前参数中) | `InvalidArgs{UnknownGroup}` |
| CTL-207.6 | 先 `Drain{cancel_resting false}` | `PreconditionFailed{Draining}` |
| CTL-207.7 | `acct-a` 启动闸门未放行 | `PreconditionFailed{GateNotReleased}` |
| CTL-207.8 | `g1` 没有多出的一腿、只有对冲腿余数 | `Applied`;`WriteOff.qty = 0`,账不变,余数清零 |
| CTL-207.9 | `(sim-a, g9)` 同样的余数 | `Applied`(模拟账户同样适用) |
| CTL-207.10 | issuer `cli:tom`(Trader) | `Forbidden`(gateway) |

### CTL-208 SmokeOrder 经客户端的每种结果码

- 规则:[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)、[control-plane 结果码](../../docs/subsystems/control-plane.md#结果码)
- 级别:table
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`;请求 `SmokeOrder{venue bitget-spot, inst bitget-spot:BTCUSDT, side Buy}`,`acct-a`;issuer `cli:alice`;除第 1 行外两步确认
- 实现:`crates/hft-ctl/tests/e2e/smoke.rs::ctl_208_smoke_order_codes`
- 备注:冒烟单在 Shadow/CloseOnly 下的挂出、定价、到期撤单、`SmokeActive`、`SmokeNoPrice`、`NotLive` 与 Trader 的 `RoleInsufficient` 由 MD-21 覆盖;本表只补确认、Draining、闸门、Telegram Admin 与结果在客户端的渲染

| 行 | 状态 / 请求 | 期望 |
|---|---|---|
| CTL-208.1 | 不确认 | `NeedsConfirm` |
| CTL-208.2 | `Kill{Account(acct-a), hard}` 生效中 | `Rejected`,`detail = {"name":"SmokeRiskReject","arg":"HARD"}` |
| CTL-208.3 | 先 `Drain{cancel_resting false}` | `PreconditionFailed{Draining}` |
| CTL-208.4 | `acct-a` 启动闸门未放行 | `PreconditionFailed{GateNotReleased}` |
| CTL-208.5 | issuer `tg:1003`(Admin,私聊) | `Applied` |

### CTL-209 AdoptExchange 与 SetBalanceBaseline 经客户端的每种结果码

- 规则:[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)、[control-plane 结果码](../../docs/subsystems/control-plane.md#结果码)
- 级别:table
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`;`report_id 7` 已生效,含 `acct-a` 在 `binance-usdm:BTCUSDT` 的 Position 差异 +0.01 BTC(`view_seq` = V);issuer `cli:tom`;除标明"不确认"的行外两步确认
- 实现:`crates/hft-ctl/tests/e2e/adopt.rs::ctl_209_adopt_and_baseline_codes`

| 行 | 状态 / 请求 | 期望 |
|---|---|---|
| CTL-209.1 | `AdoptExchange{binance-usdm, 7, Position, binance-usdm:BTCUSDT}`,不确认 | `NeedsConfirm` |
| CTL-209.2 | 同上两步确认 | `Applied`;ledger 一条 `Adjust` 类调整 +0.01 BTC,不计入价差 PnL |
| CTL-209.3 | `view_seq` 之后该 inst 有成交 | `PreconditionFailed{StaleReport}` |
| CTL-209.4 | `account sim-a` | `InvalidArgs{NotLive}` |
| CTL-209.5 | 先 `Drain{cancel_resting false}` | `PreconditionFailed{Draining}` |
| CTL-209.6 | `SetBalanceBaseline{bitget-spot}`,`acct-a` | `Applied` |
| CTL-209.7 | `acct-a` 在 `bitget-spot` 尚无余额视图 | `PreconditionFailed{NoBalanceView}` |
| CTL-209.8 | `SetBalanceBaseline{bitget-spot}`,`sim-a` | `InvalidArgs{NotLive}` |
| CTL-209.9 | `SetBalanceBaseline`,不确认 | `NeedsConfirm` |

### CTL-210 VenueNotice、SimReset、SimTransfer、RecordTransfer 与 Annotate 经客户端的结果

- 规则:[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)、[control-plane 结果码](../../docs/subsystems/control-plane.md#结果码)
- 级别:table
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`;`sim-a` 在 `binance-usdm` 可用 1 000 USDT;issuer `cli:tom`(另注明的除外)
- 实现:`crates/hft-ctl/tests/e2e/misc.rs::ctl_210_misc_codes`

| 行 | 状态 / 请求 | 期望 |
|---|---|---|
| CTL-210.1 | `VenueNotice{binance-usdm, inst binance-usdm:BTCUSDT, Delisting}` | `Applied`;`Delisting` cap `(venue 1, inst 1)`;订阅该 inst 的实例收到 `EventRef::Notice` |
| CTL-210.2 | 先 `Drain{cancel_resting false}`;210.1 | `Applied` |
| CTL-210.3 | `VenueNotice{binance-usdm, FundingInterval, new_s 14400}`,`account *` | `Applied` |
| CTL-210.4 | `VenueNotice{binance-usdm, Maintenance, …}`,`account *` | `Applied` |
| CTL-210.5 | `sim-a` 全部 cell 有效模式 CloseOnly、无活动目标、无 Unknown、无非终态 IOC/FOK;`SimReset`,两步确认 | `Applied`;`sim-a` 余额等于 `initial_balances`,仓位与库存为 0 |
| CTL-210.6 | `sim-a` 有一个有效模式 Live 的 cell;`SimReset` 两步确认 | `PreconditionFailed{SimNotIdle}` |
| CTL-210.7 | `sim-a` 有一个非终态 IOC 槽;`SimReset` 两步确认 | `PreconditionFailed{SimNotIdle}` |
| CTL-210.8 | `SimReset`,不确认 | `NeedsConfirm` |
| CTL-210.9 | `SimTransfer{binance-usdm → bitget-spot, USDT, "100"}`,`sim-a` | `Applied` |
| CTL-210.10 | 同上 `amount "1000.000001"` | `PreconditionFailed{InsufficientBalance}` |
| CTL-210.11 | `RecordTransfer{binance-usdm → bitget-spot, USDT, "100", ext_ref "TX123"}`,`acct-a` | `Applied` |
| CTL-210.12 | issuer `tg:1001`;`Annotate{OpsNote, "x"}`,`account *` | `Applied`;状态哈希与发出前相同 |
| CTL-210.13 | `Annotate{Incident, "y"}`,`account *` | `Applied` |

### CTL-211 CancelAll、Drain 与 ReplaceRefdata 经客户端的结果

- 规则:[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)、[control-plane 结果码](../../docs/subsystems/control-plane.md#结果码)
- 级别:table
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`;`acct-a` 有 3 个挂单;hft-core `config_source = journal`(另注明的除外);refdata 当前版本 1;issuer `cli:tom`,`ReplaceRefdata` 用 `cli:alice` 且两步确认
- 实现:`crates/hft-ctl/tests/e2e/maintenance.rs::ctl_211_cancel_drain_refdata`

| 行 | 状态 / 请求 | 期望 |
|---|---|---|
| CTL-211.1 | `CancelAll`,`acct-a` | `Applied`;`acct-a` 的两个 venue 各有撤单出环(支持批量撤单的 venue 为一条 venue 级 `CancelAll`),之后 `acct-a` 没有挂单 |
| CTL-211.2 | `Kill{Global, hard}` 生效中;`CancelAll`,`account *` | `Applied` |
| CTL-211.3 | `Drain{cancel_resting true}` | `Applied`;`Draining` cap;挂单撤单出环 |
| CTL-211.4 | 211.3 之后 `SetMode→Live` 两步确认 | `PreconditionFailed{Draining}` |
| CTL-211.5 | 211.3 之后 `CancelAll` | `Applied` |
| CTL-211.6 | `ReplaceRefdata`,装配校验失败 | `Rejected{RefdataAssembly}`;journal 中没有该命令的 `ControlCmd` 与 `CmdResult` |
| CTL-211.7 | `config_source = file`;`ReplaceRefdata` 版本 2 | `PreconditionFailed{ConfigSourceNotJournal}` |
| CTL-211.8 | 版本 1 | `StaleVersion` |
| CTL-211.9 | 版本 2,`RefData::load` 返回 `AssetChanged` | `Rejected`,`detail = {"name":"RefdataRecheck","arg":"AssetChanged"}`;旧 refdata 生效,不 Drain |
| CTL-211.10 | 版本 2,不确认 | `NeedsConfirm` |
| CTL-211.11 | 版本 2,通过 | `Applied`;随即 `Draining` cap(`cancel_resting = false`) |

### CTL-212 StrategyCmd 与 InjectWarmup 经客户端的结果

- 规则:[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)、[control-plane 类别](../../docs/subsystems/control-plane.md#类别)
- 级别:table
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`;`s1-main` 的命令描述符同 CTL-067(`op 1` 非只减仓、`op 2` 只减仓);`s2-main` 的种类声明接收预热;StrategyCmd 由 `cli:tom` 发出,InjectWarmup 由 `pipeline` 发出
- 实现:`crates/hft-ctl/tests/e2e/strategy_channel.rs::ctl_212_strategy_channel`

| 行 | 状态 / 请求 | 期望 |
|---|---|---|
| CTL-212.1 | `op 1`,`reduce_only false`,不确认 | `NeedsConfirm` |
| CTL-212.2 | 同上两步确认 | `Applied`;同一 Input 内 `s1-main` 的快速部分收到 `EventRef::Manual`,载荷与发送相同、`gix` 为 `g1` 的下标 |
| CTL-212.3 | `op 2`,`reduce_only true`,不确认 | `Applied` |
| CTL-212.4 | 先 `Drain{cancel_resting false}`;212.3 | `Applied` |
| CTL-212.5 | 先 `Drain{cancel_resting false}`;212.2 | `PreconditionFailed{Draining}` |
| CTL-212.6 | `op 1`,group `"16778215"` 两步确认 | `InvalidArgs{UnknownGroup}` |
| CTL-212.7 | `acct-a` 启动闸门未放行;212.3 | `Applied` |
| CTL-212.8 | `acct-a` 启动闸门未放行;212.2 | `PreconditionFailed{GateNotReleased}` |
| CTL-212.9 | 212.3 中策略据此产生一个 Open 意图 | 该意图被拒(`CLOSE_ONLY`),不出环 |
| CTL-212.10 | `InjectWarmup` 3 块(`chunk_no` 0、1、2,末块 `last true`) | 每块 `Applied`;`s2-main` 的 Model 依序收到 `EventRef::Warmup` 的 0、1、2 块 |
| CTL-212.11 | 先 `Drain{cancel_resting false}`;212.10 | 每块 `Applied` |
| CTL-212.12 | 全部账户启动闸门未放行;212.10 | 每块 `Applied` |

### CTL-213 ReconcileReport 经客户端整组生效

- 规则:[control-plane 非命令输入](../../docs/subsystems/control-plane.md#非命令输入reconcilereport)
- 级别:integ
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`,但 `acct-a` 的 `bitget-spot` 尚未 `RECONCILED`;issuer `reconciler`
- 步骤:
  1. 发 2 条的报告组(`Position` 差异 0、`Summary`),`report_id 9`
- 期望:
  - 响应 `ok`、`Applied`;journal 中依序两条 `ReconcileReport`;第二条处理后该 (account, venue) 的 `RECONCILED` 位置位;没有 `CmdResult`
- 实现:`crates/hft-ctl/tests/e2e/recon.rs::ctl_213_reconcile_report_group`

### CTL-214 在环中过期的命令由 sequencer 回 Expired 且不执行

- 规则:[线协议](README.md#线协议)、[control-plane 过期](../../docs/subsystems/control-plane.md#过期)
- 级别:integ
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`;请求 `Kill{Account(acct-a)}`,`expires_at_ns` = 到达 + 30 s
- 步骤:
  1. gateway 写环后暂停 sequencer 30 001 ms(虚拟时间),再恢复
  2. 对这段 journal 运行验证回放
- 期望:
  - `CmdResult{Expired}`;没有 Kill cap,`ctl_state_version` 不变;客户端收到 `ok`、`Expired`(或先 `accepted`,补查得到 `Expired`)
  - 验证回放得到同一 `CmdResult`
- 实现:`crates/hft-ctl/tests/e2e/expiry.rs::ctl_214_expired_in_ring`

### CTL-215 gateway 重启后用同一 cmd_id 重试只执行一次

- 规则:[CmdResult 回传](README.md#cmdresult-回传)、[control-plane 幂等](../../docs/subsystems/control-plane.md#幂等)
- 级别:integ
- 里程碑:M2
- 前置:`E2E` 稳态 `S0`;请求 `SetMode→CloseOnly{s1-main, g1}`,`acct-a`
- 步骤:
  1. 发出并得到 `Applied`
  2. 重建 gateway(去重缓存清空),以同一 `cmd_id` 调 `result()`
- 期望:
  - 步骤 2 写环一条 `ControlCmd`,sequencer 回 `CmdResult{Duplicate}`,客户端得到 `ok`、`Duplicate`;`ctl_state_version` 只因步骤 1 增加 1
- 实现:`crates/hft-ctl/tests/e2e/idempotency.rs::ctl_215_retry_after_gateway_restart`

### CTL-300 ops_bot 经本通道完成运维演练

- 规则:[概要](README.md#概要)、[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)
- 级别:drill
- 里程碑:M6
- 门槛:⑪
- 环境:dev2 测试网联调窗口:`hft-core@prod`(`instance.order_env = "testnet"`)、ops_bot、kill-switch、reconciler、pipeline 以 deploy 生成的单元运行
- 授权:`[需授权]`(启停测试网实例与联调窗口)
- 步骤:按 [incident-response kill 演练](../../docs/runbooks/incident-response.md#kill-演练) 第 1–5 步与 [launch A 阶段](../../docs/runbooks/launch.md#a-联调dev2测试网模拟盘),经 ops_bot 依次完成:全局 `Kill` 与按账户 HARD、`Drain` 后重启、`AdoptExchange`(含余额条目)、`SetBalanceBaseline`、`VenueNotice{Delisting}`、`SimReset`(`sim` 实例)、`Resume{Recovery}`、日亏熔断触发与 `Resume{DailyLoss}`、pipeline 注入预热数据(`InjectWarmup` 全部块 `Applied`)、`WriteOffResidual`
- 通过标准:每项命令的 `CmdResult` 码与 [control-plane 命令集](../../docs/subsystems/control-plane.md#命令集) 规定的一致;journal 中每项都有审计 `Note{CtlAudit}`、`ControlCmd` 与 `CmdResult`(`GetSchema` 与被 gateway 拒绝的只有审计 Note)
- 证据:incident-response 的 kill 演练记录表一行,另存 `CTL-300-<日期>.md`(命令、`cmd_id`、结果码、journal seq)
- 实现:`tests/host/CTL-300.md::ctl_300_ops_drill`

### CTL-301 老板账号的只读与紧急停止演练

- 规则:[概要](README.md#概要)、[control-plane 版本与确认](../../docs/subsystems/control-plane.md#版本与确认)、[control-plane 角色](../../docs/subsystems/control-plane.md#角色)
- 级别:drill
- 里程碑:M6
- 门槛:⑪
- 环境:同 CTL-300;老板的 Telegram 账号在签发者表中只有 Viewer
- 授权:`[需授权]`
- 步骤:按 [incident-response kill 演练](../../docs/runbooks/incident-response.md#kill-演练) 第 6 步(预览 → 本人确认 → Kill 生效 → 交易员 `Resume{Kill}`),再按第 7 步做一次 hft-core 停止时经 kill-switch 本地入口撤单(该部分的 kill-switch 侧通过标准见 [kill-switch 测试规格](../../apps/kill-switch/TESTS.md))
- 通过标准:第 6 步:预览显示范围与"不平仓",确认后 Kill cap 生效、挂单撤销、已成交第一腿的对冲照常,预览后他人改变状态不影响确认,61 s 后确认被拒,老板发 `Resume`、带 `hard`/`flatten` 的 Kill 均被拒且有审计;第 7 步:值班交易员收到 P0
- 证据:kill 演练记录表一行,另存 `CTL-301-<日期>.md`
- 实现:`tests/host/CTL-301.md::ctl_301_viewer_drill`

### CTL-302 测试网上的冒烟单与残余核销

- 规则:[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)
- 级别:testnet
- 里程碑:M6
- 环境:dev2 联调窗口,测试网账户
- 授权:`[需授权]`
- 步骤:
  1. Admin 在私聊 `/smoke <测试网账户> <venue> <inst> buy 10000`,预览后确认
  2. 等 10 s
  3. 构造一个余数低于最小下单量的静止 group,Admin `/writeoff`,预览后确认
- 通过标准:步骤 1 交易所出现一张 PostOnly、远离对手价、约最小名义的挂单,ops_bot 回显的订单号与 `trigger_seq` 指向的 `OrderDecision` 一致;步骤 2 后该单在交易所为已撤销;步骤 3 `Applied`,该 group 全部腿余数为 0,无订单发出
- 证据:交易所订单历史截图与 `CTL-302-<日期>.md`
- 实现:`tests/host/CTL-302.md::ctl_302_testnet_smoke_and_writeoff`

### CTL-303 演练期间的审计无缺口

- 规则:[鉴权](README.md#鉴权)、[开发备忘](README.md#开发备忘)
- 级别:drill
- 里程碑:M6
- 门槛:⑪
- 环境:CTL-300–CTL-302 完成后的 journal 与 ops_bot 审计表
- 授权:无
- 步骤:用 hft-py 读出演练时段的全部 `Note{CtlAudit}`、`ControlCmd` 与 `CmdResult`,与 ops_bot 本地审计表按 `cmd_id` 对齐
- 通过标准:每个 `ControlCmd` 恰有一条同 `cmd_id` 的审计 Note;ops_bot 审计表中每条发往 gateway 的请求在 journal 中都有审计 Note;没有 issuer 为空的 `ControlCmd`
- 证据:对齐脚本的输出 `CTL-303-<日期>.txt`
- 实现:`tests/host/CTL-303.md::ctl_303_audit_complete`

### CTL-304 签发者表按 TOML 加载为 uid 与 issuer 两张表

- 规则:[鉴权](README.md#鉴权)
- 级别:unit
- 里程碑:M2
- 前置:测试 passwd 夹具:`hft-opsbot` = 1100、`hft-pipeline` = 1101、`hft-recon` = 1102、`alice` = 1200;文件 `[uids]` 含 `"hft-opsbot" = "ops_bot"`、`"hft-pipeline" = "pipeline"`、`"hft-recon" = "reconciler"`、`"alice" = "cli"`,`[issuers]` 含 `"tg:1002" = ["Trader"]`、`"cli:alice" = ["Admin"]`
- 步骤:
  1. `PrincipalTable::load(path)`
- 期望:
  - `Ok`;uid 1100 的主体为 `hft-opsbot`(可用 issuer 形式 `tg:*` 与 `ops_bot`),uid 1200 只接受 `cli:alice`;`tg:1002` 的角色集合 = {Trader},`cli:alice` = {Admin}
- 实现:`crates/hft-ctl/tests/principals.rs::ctl_304_load_principals`

### CTL-305 签发者表的每种加载错误

- 规则:[鉴权](README.md#鉴权)
- 级别:table
- 里程碑:M2
- 前置:同 CTL-304 的夹具与文件;各行在其上做一处改动
- 实现:`crates/hft-ctl/tests/principals.rs::ctl_305_load_errors`

| 行 | 改动 | 期望 |
|---|---|---|
| CTL-305.1 | 文件不存在 | `Err(PrincipalsErr::Io)` |
| CTL-305.2 | 第一行改为 `[uids` | `Err(PrincipalsErr::Parse)` |
| CTL-305.3 | 加顶层键 `extra = 1` | `Err(PrincipalsErr::Parse)` |
| CTL-305.4 | `[uids]` 加 `"nobody-xyz" = "cli"`(passwd 中没有) | `Err(PrincipalsErr::UnknownUser)` |
| CTL-305.5 | `"hft-recon" = "watchtower"` | `Err(PrincipalsErr::UnknownPrincipal)` |
| CTL-305.6 | `"tg:1002" = ["Boss"]` | `Err(PrincipalsErr::UnknownRole)` |
| CTL-305.7 | `"tg:abc" = ["Viewer"]` | `Err(PrincipalsErr::BadIssuer)` |
| CTL-305.8 | `"cli:bob" = ["Trader"]`(`bob` 不在 `[uids]`) | `Err(PrincipalsErr::BadIssuer)` |
| CTL-305.9 | `[issuers]` 中 `"tg:1002"` 出现两次 | `Err(PrincipalsErr::DupEntry)` |

### CTL-306 运行中改签发者表文件不改变鉴权结果

- 规则:[鉴权](README.md#鉴权)
- 级别:unit
- 里程碑:M2
- 前置:G0(`principals` 由 CTL-304 的文件加载,另含 `"cli:tom" = ["Trader"]` 与 `"tom" = "cli"`)
- 步骤:
  1. `cli:tom` 发 `Annotate{OpsNote}`
  2. 删除文件中 `cli:tom` 一行,不重建 gateway;`cli:tom` 再发一次
- 期望:
  - 两次都写环(`Annotate` 照常进入);gateway 在步骤 2 期间没有打开该文件(测试构建的文件访问计数为 0)
- 实现:`crates/hft-ctl/tests/principals.rs::ctl_306_no_reload`

### CTL-307 hft-event 编解码错误到 detail 码的映射

- 规则:[gateway 侧的命令校验](README.md#gateway-侧的命令校验)
- 级别:table
- 里程碑:M2
- 前置:映射函数 `validate::detail_of_event_err`;无其他参数
- 实现:`crates/hft-ctl/tests/validate.rs::ctl_307_event_err_detail`

| 行 | 输入 | 期望 |
|---|---|---|
| CTL-307.1 | `ParamsErr::DupGroup` | `InvalidArgs{GroupIdDup}`(111) |
| CTL-307.2 | `ParamsErr::Truncated` | `InvalidArgs{BadGroup}`(112) |
| CTL-307.3 | `ParamsErr::BadLen` | `InvalidArgs{BadGroup}` |
| CTL-307.4 | `ParamsErr::BadGroup` | `InvalidArgs{BadGroup}` |
| CTL-307.5 | `ParamsErr::BadLeg` | `InvalidArgs{BadGroup}` |
| CTL-307.6 | `ParamsErr::Padding` | `InvalidArgs{BadGroup}` |
| CTL-307.7 | `ParamsErr::BadHdr` | `InvalidArgs{BadGroup}` |
| CTL-307.8 | `UnknownKind` | `InvalidArgs{UnknownDiscriminant}`(100) |
| CTL-307.9 | `InvalidEnum` | `InvalidArgs{UnknownDiscriminant}` |

## 验收

- 本 crate 在 M2 完成的标准:里程碑为 M2 的用例全部有同编号的实现且通过;Python 客户端(CTL-140–CTL-142)在 M4 完成([docs/tests §1](../../docs/tests/README.md#1-地位))。
- 属性测试(CTL-010、CTL-069、CTL-121):PR 按各用例的例数;CI 每夜以 100 倍例数运行一次。
- 基准 `ctl/ingest`、`ctl/ingest_setparams_500`、`ctl/get_schema`、`ctl/ingest_warmup_4k` 与端到端目标的数值只在 [延迟与资源契约](README.md#延迟与资源契约) 登记;基准在参考机判定,端到端由 CTL-155 判定。
- 上线门槛 ⑪ 的运维项([testing §8](../../docs/testing.md#8-上线门槛)):CTL-300、CTL-301、CTL-303 的演练记录齐全;CTL-302 在测试网通过。
- 解耦门槛(新增第二个测试策略时本 crate diff 为零)由 `cargo xtask scaffold-gate` 判定([testing §8](../../docs/testing.md#8-上线门槛));CTL-160 证明本 crate 对策略集泛型。
