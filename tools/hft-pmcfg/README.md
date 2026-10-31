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
  - [库](#库)
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
          [--account LABEL:UID:ROLE KEYS_DIR ...]] [--param NAME=RAW ...]
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
| `--param NAME=RAW` | 只用于 `--maker` 或 `--live`,可重复:在默认值(实盘做市另在实盘参数)之后把 binary-maker(`--taker-probe` 时为 pm-taker-probe)字段 `NAME` 设为原始值 `RAW` |
| `OUTPUT_DIR` | 输出目录(不存在则创建);写 `core.toml`、`principals.toml`、`refdata.bin`、`engine.bin`、`risk_params.bin`、`strategy_params.bin` |
| `TMPFS_DIR` | `paths.journal_ram` |
| `SRC_IP`、`BIND_DEVICE` | venue `pm`(及 `--maker`/`--live` 的 venue `bn`)的出口地址与网卡 |
| `USER`、`PIPELINE_USER` | 可选:Admin 的 cli 主体;pipeline 主体(发现进程的系统用户,发 `BindSlot`) |

成功时标准输出打印 `core.toml` 的路径,退出 0;`-h`/`--help` 打印用法,退出 0。拒绝(退出 2,标准错误打印原因与用法,不写任何文件):位置参数不是 4–6 个;未知选项;非重复选项出现两次;选项缺值;`--maker` 与 `--live` 同用;只用于 `--live` 的选项没有 `--live`;`--param` 没有 `--maker`/`--live`、不是 `NAME=整数`、字段不在 binary-maker(`--taker-probe` 时 pm-taker-probe)schema 中或超出其 `[min, max]`;`--daily-loss` 不是 > 0 且至多 6 位小数的十进制;`--daily-loss-resumes` 不在 0–20;`--account-cap`/`--group-cap` 不是 > 0 且至多 6 位小数的十进制(`pm-live` 的账户与场所名义上限、每个 slot 组的名义上限,缺省 55 / 25 USDC);`--day-loss-carry` 的日期不是 `YYYY-MM-DD` 或金额不 < 0;[多账户](#多账户)的任一条约束不满足。写文件失败同样退出 2。

### 多账户

`--live-account` 与 `--account` 按命令行出现的顺序成为账户 2、3、…(`pm-live` 是账户 1),合计至多 11 个(每 venue 至多 `KillBits::MAX_ACCOUNTS_PER_GW` = 12 个账户,[hft-core](../../apps/hft-core/README.md))。每个账户:

- refdata:Live、`is_prod`、绑定同一个 binary-maker 实例(StrategyId 1),在 Polymarket 上的 venue UID;
- `core.toml`:venue `pm` 的 `accounts` 一项(`account` = 账户号,`api_key_path`/`session_seed_path`/`rest_api_key_path`/`rest_secret_path`/`passphrase_path` 指向该账户 `KEYS_DIR` 下的文件);
- ECF1:`keys_required` 一项;RiskParams:与 `pm-live` 相同的账户与 venue 限额;
- 策略参数仍是 `pm-live` 名下的一个 blob(每实例一份)。

`--account` 的 `LABEL:UID:ROLE`:标签取第一个 `:` 之前,角色取最后一个 `:` 之后,中间为 UID;三者都不得为空。`ROLE` 目前只接受 `any`(账户角色与按角色路由尚未实现;角色不进任何生成的文件,`--account L:U:any D` 与 `--live-account U D` 的输出只差标签)。约束:标签由 `[A-Za-z0-9._-]` 组成、不以 `sim-` 开头(Live 账户,[hft-core](../../apps/hft-core/README.md))、在全部账户(含 `pm-live` 与生成的 `pm-live-<n>`)中唯一;UID 在全部账户中唯一。

### 库

```rust
pub struct PmLive { pub uid: String, pub keys_dir: PathBuf, pub day_loss_carry: Option<(u32, i64)>,
    pub daily_loss_abs: Option<i64>, pub daily_loss_max_resumes: Option<u8>, pub presign: bool,
    pub extra_accounts: Vec<LiveAccount> }
pub struct LiveAccount { pub label: Option<String>, pub uid: String, pub keys_dir: PathBuf, pub role: AccountRole }
pub enum AccountRole { Any }
pub fn write_polymarket_sim(root, hot, user, pipeline, src_ip, bind_device) -> Result<PathBuf, String>;
pub fn write_polymarket_maker_sim_with(.., params: &[(String, i64)]) -> Result<PathBuf, String>;
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

## 实现说明

<details><summary>实现要点</summary>

- `src/lib.rs`:`write_pm` 用 hft-refdata `RdbBuilder` 建 refdata(USDC、每个槽位一个代币资产与 inst,挂单模式另有 BTC/USDT/BNB 与 Binance 现货参考 inst)、SDK `encode_body` 建策略参数 blob(schema 来自 registry 的 `kinds()`)、`hft_risk::ParamsBuilder` 建风控 blob、`engine_base` + 逐项修改后 `EngineSetup::encode_blob` 建 ECF1;`core.toml` 先按模拟写出,实盘再经 `toml` 改写(去掉 `[sim]` 与 `live_feed`、加账户与 `account_io`/`net`/`presign`、gw 线程代替 sim-gw/paper-feed)。
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
- 当日亏损延续(`--day-loss-carry`)只记在 `pm-live` 上。
