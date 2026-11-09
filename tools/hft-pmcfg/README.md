---
description: "Polymarket 的 hft-core 配置生成器 hft-pmcfg:book-observer 行情模拟、binary-maker 挂单模拟、实盘做市(一个或多个 Live 账户)与实盘吃单探针(pm-taker-probe)四种模式,写出 refdata/engine/risk/strategy 四个 blob、principals.toml 与 core.toml;运维在工作站上生成实盘箱与模拟箱的配置"
kind: crate
tier: cold
scope: optional
status: impl
---

# hft-pmcfg

## 概要

`hft-pmcfg` 生成 Polymarket 的 hft-core 实例配置:四个二进制配置(RDB1 refdata、ECF1 engine、RiskParams、binary-maker 或 book-observer 的 StrategyParams)、`principals.toml` 与引用它们(含 SHA-256)的 `core.toml`。它只生成文件:不连接交易所、不读凭据内容(只把凭据目录下的文件路径写进 `core.toml`)、不启动进程;配置能否被 hft-core 加载由 hft-core 的测试用同一个库验证([CORE-426](../../apps/hft-core/TESTS.md#core-426-polymarket-挂单模拟配置可加载)、[CORE-446](../../apps/hft-core/TESTS.md) 等)。库同时是 hft-core 测试的 Polymarket 配置夹具(`apps/hft-core/tests/support`),hft-core 的 example `pm_sim_config` 是同一命令行的薄壳。触发条件:Polymarket 接入([提案](../../.agents/notes/proposed/architecture/2026-10-19-polymarket-venue.md));多账户按 [account-set-per-instance](../../.agents/notes/proposed/architecture/2026-10-29-account-set-per-instance.md)。

## 目录

- [使用本组件](#使用本组件)
  - [命令行](#命令行)
  - [多账户](#多账户)
  - [参考行情](#参考行情)
  - [库](#库)
  - [绑核](#绑核)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 命令行

```text
hft-pmcfg [--maker | --live UID KEYS_DIR [--taker-probe] [--day-loss-carry YYYY-MM-DD USDC] [--daily-loss USDC]
          [--daily-loss-resumes N] [--account-cap USDC] [--group-cap USDC] [--no-presign] [--live-account UID KEYS_DIR ...]
          [--account LABEL:UID:ROLE KEYS_DIR ...] [--no-chainlink]] [--no-coinbase] [--no-perp] [--cl-keys DIR]
          [--cpu-plan none|pin|spin] [--param NAME=RAW ...]
          OUTPUT_DIR TMPFS_DIR SRC_IP BIND_DEVICE [USER] [PIPELINE_USER]
```

`pm_sim_config`(hft-core example,`--features polymarket`)接受同一命令行、写出逐字节相同的文件。选项可以出现在任何位置;位置参数按顺序取。

| 参数 | 含义 |
|---|---|
| (无模式选项) | book-observer 行情模拟(`--dry-run`,16 个槽位 inst,Sim 账户 `sim-pm`);内容归 [hft-core 可执行配置](../../apps/hft-core/config/README.md) |
| `--maker` | binary-maker 挂单模拟(三对 BTC 槽位 + Binance BTCUSDT 参考,`maker_fill = "queue"`);内容见 [pm-md-probe 挂单模拟](../pm-md-probe/README.md#挂单模拟) |
| `--live UID KEYS_DIR` | 实盘做市(`order_env = "prod"`):Live 账户 `pm-live`(账户 1,venue UID `UID`),凭据文件 `KEYS_DIR/{api_key,secret,passphrase}`;参数、限额与 `--day-loss-carry`/`--daily-loss`/`--daily-loss-resumes` 的语义归  [hft-core 可执行配置](../../apps/hft-core/config/README.md) |
| `--taker-probe` | 只用于 `--live`:实盘吃单探针——与 `--live` 相同的 venue、槽位、账户、凭据、限额、预签名、OMS 与 REST 设置,只是策略实例换成 [pm-taker-probe](../../strategies/pm-taker-probe/README.md)(实例名 `pm-taker-probe`,一个 group = 槽位 `btc-up-0`(InstId 1),可开仓,参数为其 schema 默认值,`enabled = 0`)、价格带的 `rr_max_cross` 为 0.5(`PM_PROBE_RR_MAX_CROSS`:卖回在最优买价,半个价差可达价格的数个百分点;做市为 0.01);`--param` 改按 pm-taker-probe 的 schema 检查 |
| `--no-presign` | 只用于 `--live`:不写 `venues.pm.presign`(缺省写 (2, 2, `0.01`, 60 000),[hft-core 配置键](../../apps/hft-core/README.md)) |
| `--live-account UID KEYS_DIR` | 只用于 `--live`,可重复:再一个 Live 账户,标签 `pm-live-<账户号>`,角色 `any`(见[多账户](#多账户)) |
| `--account LABEL:UID:ROLE KEYS_DIR` | 只用于 `--live`,可重复:再一个 Live 账户,标签 `LABEL`、角色 `ROLE`(见[多账户](#多账户)) |
| `--no-coinbase` | 只用于 `--maker` 或 `--live`:不写第二个参考行情 `[venues.cb]`(缺省写:`coinbase-spot`、`id = 16`、`max_sub_insts = 1`、模拟时 `live_feed = true`,线程 `feed-cb`/`build-cb`,`[venues.cb.feed] race_rotate_s = 600`(赛跑 BBO 连接每 10 分钟评估一次,长期落后的轮换,hft-venue §3a);refdata 加 `USD` 资产(Stable,ledger 按面值计价)、Coinbase venue(端点 `wss://ws-feed.exchange.coinbase.com`、`https://api.exchange.coinbase.com`,caps 与限速取 [venue-coinbase](../../crates/venue-coinbase/README.md) 默认)与 inst `BTC-USD` = InstId 8(与 Binance 参考同一 BTC 资产,tick 0.01、lot 1e-8);ECF1 的 feeds 加该 venue;binary-maker 多一个第 8 个 group(InstId 8、不开仓、不绑账户)作为第二参考——v37 `cb_*` 保护读它的 BBO,`cb_win_ms` = 0(缺省)时不影响决定)。带此选项的输出与加入 Coinbase 之前相同,只多 `[venues.bn.feed] race_rotate_s = 600`(2026-11-07 起 Binance 赛跑连接同样轮换慢连接) |
| `--no-perp` | 只用于 `--maker` 或 `--live`:不写永续参考 `[venues.bp]`(见[参考行情](#参考行情)) |
| `--no-chainlink` | 只用于 `--live`:不写 Chainlink `[venues.cl]` |
| `--cl-keys DIR` | 只用于 `--maker` 或 `--live`,与 `--no-chainlink` 互斥:Chainlink 登录用 `DIR/{api_key,secret,passphrase}`;`--maker` 只有给了它才写 `[venues.cl]`,`--live` 缺省用 `KEYS_DIR`(与 `pm-live` 同一组文件,hft-core 只读一次)。hft-core 启动时读入并删除这些文件(模拟实例同样,`--dry-run` 也读),给一份副本 |
| `--cpu-plan none\|pin\|spin` | 线程绑核方案(见[绑核](#绑核)),缺省 `none`;任何模式都可用 |
| `--param NAME=RAW` | 只用于 `--maker` 或 `--live`,可重复:在默认值(实盘做市另在实盘参数)之后把 binary-maker(`--taker-probe` 时为 pm-taker-probe)字段 `NAME` 设为原始值 `RAW` |
| `OUTPUT_DIR` | 输出目录(不存在则创建);写 `core.toml`、`principals.toml`、`refdata.bin`、`engine.bin`、`risk_params.bin`、`strategy_params.bin` |
| `TMPFS_DIR` | `paths.journal_ram` |
| `SRC_IP`、`BIND_DEVICE` | venue `pm`(及 `--maker`/`--live` 的 venue `bn`、`cb`)的出口地址与网卡 |
| `USER`、`PIPELINE_USER` | 可选:Admin 的 cli 主体;pipeline 主体(发现进程的系统用户,发 `BindSlot`) |

成功时标准输出打印 `core.toml` 的路径,退出 0;`-h`/`--help` 打印用法,退出 0。拒绝(退出 2,标准错误打印原因与用法,不写任何文件):位置参数不是 4–6 个;未知选项;非重复选项出现两次;选项缺值;`--maker` 与 `--live` 同用;只用于 `--live` 的选项没有 `--live`;`--param` 或 `--no-coinbase` 没有 `--maker`/`--live`、不是 `NAME=整数`、字段不在 binary-maker(`--taker-probe` 时 pm-taker-probe)schema 中或超出其 `[min, max]`;`--daily-loss` 不是 > 0 且至多 6 位小数的十进制;`--daily-loss-resumes` 不在 0–20;`--account-cap`/`--group-cap` 不是 > 0 且至多 6 位小数的十进制(`pm-live` 的账户与场所名义上限、每个 slot 组的名义上限,缺省 55 / 25 USDC);`--day-loss-carry` 的日期不是 `YYYY-MM-DD` 或金额不 < 0;[多账户](#多账户)的任一条约束不满足。写文件失败同样退出 2。

### 多账户

`--live-account` 与 `--account` 按命令行出现的顺序成为账户 2、3、…(`pm-live` 是账户 1),合计至多 11 个(每 venue 至多 `KillBits::MAX_ACCOUNTS_PER_GW` = 12 个账户,[hft-core](../../apps/hft-core/README.md))。每个账户:

- refdata:Live、`is_prod`、绑定同一个 binary-maker 实例(StrategyId 1),在 Polymarket 上的 venue UID;
- `core.toml`:venue `pm` 的 `accounts` 一项(`account` = 账户号,`api_key_path`/`session_seed_path`/`rest_api_key_path`/`rest_secret_path`/`passphrase_path` 指向该账户 `KEYS_DIR` 下的文件);
- ECF1:`keys_required` 一项;RiskParams:与 `pm-live` 相同的账户与 venue 限额;
- 策略参数仍是 `pm-live` 名下的一个 blob(每实例一份)。

`--account` 的 `LABEL:UID:ROLE`:标签取第一个 `:` 之前,角色取最后一个 `:` 之后,中间为 UID;三者都不得为空。`ROLE` 目前只接受 `any`(账户角色与按角色路由尚未实现;角色不进任何生成的文件,`--account L:U:any D` 与 `--live-account U D` 的输出只差标签)。约束:标签由 `[A-Za-z0-9._-]` 组成、不以 `sim-` 开头(Live 账户,[hft-core](../../apps/hft-core/README.md))、在全部账户(含 `pm-live` 与生成的 `pm-live-<n>`)中唯一;UID 在全部账户中唯一。

### 参考行情

`--maker`/`--live` 的 binary-maker 在 Polymarket 槽位之外读这些参考市场,group 依次排在槽位之后(InstId 同序):

| venue 键 | 协议 / VenueId | inst | 缺省 | 用途 |
|---|---|---|---|---|
| `bn` | `binance-spot` / 2 | `BTCUSDT` 现货,InstId 7 | 总有 | 公允价参考(bookTicker,9443 端口) |
| `cb` | `coinbase-spot` / 16 | `BTC-USD`,InstId 8 | 有(`--no-coinbase` 去掉) | 第二参考(v37 保护、v38 融合) |
| `bp` | `binance-usdm` / 1 | `BTCUSDT` 永续(`InstKind::Perp`,tick 0.10、lot 0.001,`min_notional` 5 USDT:从不交易),紧接其前的 InstId 之后 | 有(`--no-perp` 去掉) | 只记录:bookTicker(赛跑两路)、depth5@100ms、aggTrade 进 journal;binary-maker `ref_aux` 位 0 |
| `cl` | `polymarket-price` / 17 | `btcusd`(资产 BTC、计价 USD),最后一个 InstId | `--live` 有(`--no-chainlink` 去掉),`--maker` 需 `--cl-keys` | Chainlink 价格(IndexPrice)与 60 s TWAP(MarkPrice),`[venues.cl.md_login]` 指向登录文件;binary-maker `ref_aux` 位 1,规则 12 改读原生样本、忽略旁路 |

每个参考 venue 都只读(无账户)、`live_feed`(模拟)、自己的 `feed:<v>`/`conn-builder:<v>` 线程;refdata 加 `USD` 资产(有 `cb` 或 `cl` 时,ledger 按面值计价)。binary-maker 的 `ref_aux` 由生成器按 `bp`/`cl` 是否存在写入(位 0 / 位 1),`--param ref_aux=…` 在其后生效(与 group 表不符时 hft-core 加载期 `check` 拒绝)。全部去掉(`--no-coinbase --no-perp`,`--live` 另加 `--no-chainlink`)时 venue、inst 与 group 与加入它们之前的 `--no-coinbase` 相同(参数 blob 是 schema v41,`ref_aux` = 0)。

### 库

```rust
pub struct PmLive { pub uid: String, pub keys_dir: PathBuf, pub day_loss_carry: Option<(u32, i64)>,
    pub daily_loss_abs: Option<i64>, pub daily_loss_max_resumes: Option<u8>, pub presign: bool,
    pub extra_accounts: Vec<LiveAccount>, pub coinbase: bool,   // coinbase = false:--no-coinbase
    pub perp: bool, pub chainlink: bool, pub cl_keys: Option<PathBuf> }   // --no-perp、--no-chainlink、--cl-keys
impl PmLive { pub fn refs(&self) -> Refs; }
pub struct Refs { pub coinbase: bool, pub perp: bool, pub chainlink: Option<PathBuf> /* 登录文件目录 */ }
pub struct LiveAccount { pub label: Option<String>, pub uid: String, pub keys_dir: PathBuf, pub role: AccountRole }
pub enum AccountRole { Any }
pub fn write_polymarket_sim(root, hot, user, pipeline, src_ip, bind_device) -> Result<PathBuf, String>;
pub fn write_polymarket_maker_sim_with(.., params: &[(String, i64)]) -> Result<PathBuf, String>;   // 含 Coinbase
pub fn write_polymarket_maker_sim_opts(.., params: &[(String, i64)], coinbase: bool) -> Result<PathBuf, String>;   // 含永续,无 Chainlink
pub fn write_polymarket_maker_sim_refs(.., params: &[(String, i64)], refs: &Refs) -> Result<PathBuf, String>;
pub fn write_polymarket_maker_live_with(.., live: &PmLive, params: &[(String, i64)]) -> Result<PathBuf, String>;
pub fn write_polymarket_taker_probe_live(.., live: &PmLive, params: &[(String, i64)]) -> Result<PathBuf, String>;
pub enum PmStrategy { Observer, Maker, TakerProbe }   // kind_name(): book-observer | binary-maker | pm-taker-probe
pub fn parse_param(s: &str) -> Result<(String, i64), String>;
pub fn check_maker_params(params: &[(String, i64)]) -> Result<(), String>;   // = check_params(Maker, ..)
pub fn check_params(s: PmStrategy, params: &[(String, i64)]) -> Result<(), String>;
pub fn engine_base(rd: &RefData, caps: &Capacities, ledger: LedgerConfig, venues: Vec<VenueId>) -> EngineSetup;
pub mod cli { pub fn parse(args: &[String]) -> Result<Cmd, String>; pub fn run(cmd: &Cmd) -> Result<PathBuf, String>; pub fn main(prog: &str) -> ExitCode; }
```

另有不带 `_with` 的两个写函数(无参数值)与常量 `PM_CROSSED_TOLERANCE`、`PM_COINS`、`PM_LIVE_PRESIGN`、`PM_LIVE_DAILY_LOSS`、`PM_LIVE_PARAMS`、`PM_REST_TIMEOUT_MS`、`PM_REST_MAX_IDLE_MS`、`PM_PROBE_RR_MAX_CROSS`。`LiveAccount.label = None` 即 `pm-live-<账户号>`。

### 绑核

`--cpu-plan`(库函数 `apply_cpu_plan(core.toml, plan)`,写完配置后改写 `[[threads]]`)面向 4 核的 pm-strategy 实盘机(c8g.xlarge,逻辑 CPU 0–3):

| 方案 | `sequencer` | `feed:*`(pm、bn、cb) | `gw:pm` | 其余线程 | 主机要求 |
|---|---|---|---|---|---|
| `none`(缺省) | 不绑 | 不绑 | 不绑 | 不绑 | 无;全部线程空闲时休眠 `park_us` 100 µs |
| `pin` | 核 1 | 核 2(`feed:cl` 除外) | 核 3 | 核 0(`feed:cl` 在此,`park_us = 1000`) | 无;仍休眠,只减少迁核与缓存失效 |
| `spin` | 核 1,`idle = "spin"` | 核 2,`park_us = 20`(`feed:cl` 除外) | 核 3,`idle = "spin"` | 核 0(`feed:cl`:`park_us = 1000`) | 内核 `isolcpus=1-3`(GRUB,需重启);未隔离时 hft-core 在线程启动时拒绝(`ThreadPolicy`,退出 3) |

`feed:cl` 每秒只有两条消息、样本本身晚 1.1–1.4 s,不占核 2 的低延迟预算(常量 `SLOW_FEED_PARK_US`)。

负载(2026-11-09 实盘机 60 s 采样,live-20261108T142342,`spin`,单位 = 一个核的百分比):核 1 sequencer 100(空转)、核 3 gw-pm 100(空转);核 2 feed-pm 21.4 + feed-bn 16.7 + feed-cb 16.4 = 54.5;核 0 persist 32.9 + ctl-gateway 3.6 + build-* 6.6 + housekeeping 2.6,另有 Python 进程(旁路、心跳、绑定、赎回、机器人)。feed-cb 的消息量约为 feed-bn 的 1/20,CPU 却相同:核 2 的开销主要是 `park_us = 20` 的唤醒,不是解码。永续 BTC bookTicker 约 612 条/s(赛跑两路约 1 200 帧/s;现货约 190 条/s)、depth5 10 条/s、aggTrade 10 条/s:预计 feed-bp 约 20 %(唤醒 16 % + 解码几个百分点),核 2 约 75 %;journal 写入从约 4.4 GB/h 增加约 0.6 GB/h(+14 %),persist 约 +5 %。旁路 `cl_feed.py` 停用后核 0 少一个 Python 进程。

依据(2026-11-07,11-06 实盘 journal):决策 → 写出网关 p50 94–150 µs、p99 180–339 µs;行情到达 → 决策 p50 约 145 µs。`spin` 去掉每跳至多一个 `park_us` 的唤醒等待,并免受同机其他进程抢核;两个空转线程各占满一个核,机器上不能再并行跑第二个引擎。

## 实现说明

<details><summary>实现要点</summary>

- `src/lib.rs`:`write_pm` 用 hft-refdata `RdbBuilder` 建 refdata(USDC、每个槽位一个代币资产与 inst,挂单模式另有 BTC/USDT/BNB 与 Binance 现货参考 inst,缺省再加 USD 与 Coinbase 参考 inst)、SDK `encode_body` 建策略参数 blob(schema 来自 registry 的 `kinds()`)、`hft_risk::ParamsBuilder` 建风控 blob、`engine_base` + 逐项修改后 `EngineSetup::encode_blob` 建 ECF1;`core.toml` 先按模拟写出,实盘再经 `toml` 改写(去掉 `[sim]` 与 `pm`/`bn`/`cb` 的 `live_feed`、加账户与 `account_io`/`net`/`presign`、gw 线程代替 sim-gw/paper-feed)。
- `engine_base` 与 hft-testkit `seq_rig::engine_setup` 对同一输入编码相同(PMC-006);生成器的 `[dependencies]` 不含 hft-testkit,生产配置不随测试夹具变化。
- `src/cli.rs`:顺序扫描参数,选项消耗其值,其余为位置参数;`src/main.rs` 只调 `cli::main("hft-pmcfg")`。
- 输出只由参数决定(无时间戳、无随机数):同一命令行两次运行逐字节相同。

</details>

## 延迟与资源契约

无热路径约束(离线工具,运行一次不到一秒)。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

允许:`hft-types`、`hft-refdata`、`hft-event`、`hft-strategy-sdk`、`hft-strategy-registry`(只取 `kinds()` 的描述符与 schema;registry 的使用方之一,[crates/AGENTS.md](../../crates/AGENTS.md#依赖方向))、`hft-risk`、`hft-ledger`、`hft-oms`、`hft-exec`、`hft-sequencer`(缺省 feature:`EngineSetup` 与 ECF1 编码,不启用 journal writer 与测试钩子)、`hft-venue-proto`、`venue-binance`、`venue-polymarket`;外部 `toml`、`zerocopy`、`sha2`(不启用 `asm`)。不依赖 `apps/hft-core`;被 `apps/hft-core` 以 `[dev-dependencies]` 依赖(测试夹具与 example)。

`[dev-dependencies]`:`hft-testkit`(`seq`,PMC-006 的对照)、`tempfile`。

## 延伸阅读

- [hft-core 可执行配置](../../apps/hft-core/config/README.md)
- [venue-polymarket](../../crates/venue-polymarket/README.md)
- [pm-md-probe](../pm-md-probe/README.md)

## 已知限制与后续工作

- 账户角色只有 `any`:按角色(挂单/吃单、Up/Down、备用)路由的模型实现后,`ROLE` 才接受其他值,并在那时决定它写进哪里。
- 每个附加账户沿用 `pm-live` 的限额与 venue 级出口 IP(不生成账户级 `src_ip`);需要不同限额或出口时手工改生成的 `core.toml` 或 RiskParams。
- 核 2 上每个行情线程各自休眠/唤醒;hft-core 的多任务线程(`tasks = [...]`,README 已规定、实现仍是一线程一任务)落地后,`bn`/`bp`/`cb` 应合进一个线程,省掉两份唤醒开销。
- 当日亏损延续(`--day-loss-carry`)只记在 `pm-live` 上。
