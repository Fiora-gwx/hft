---
component: hft-pmcfg
prefix: PMC
spec: README.md
---

# hft-pmcfg 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。生成的配置能否被 hft-core 加载由 hft-core 的用例覆盖(CORE-424、CORE-426、CORE-436、CORE-439、CORE-445、CORE-446 等,经 `apps/hft-core/tests/support` 调用本库)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | PMC-005 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 见子节 |
| 命令行 | PMC-001、PMC-002、PMC-005 |
| 多账户 | PMC-001、PMC-003、PMC-004 |
| 库 | PMC-004、PMC-005、PMC-006 |
| 实现说明 | PMC-005、PMC-006 |
| 延迟与资源契约 | 不适用:无门槛 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:非规范 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |

## 用例

### PMC-001 合法命令行解析为对应的模式与参数

- 规则:[命令行](README.md#命令行)、[多账户](README.md#多账户)
- 级别:table
- 里程碑:M6
- 前置:位置参数 `out hot 10.0.0.5 ens5`
- 实现:`tools/hft-pmcfg/tests/cli.rs::pmc_001_valid_command_lines`

| 行 | 输入 | 期望 |
|---|---|---|
| PMC-001.1 | 只有位置参数 | 模式 Sim;`root = out`、`hot = hot`、`src_ip = 10.0.0.5`、`bind_device = ens5`;无 USER、PIPELINE_USER |
| PMC-001.2 | `--param clip_shares=7 --maker … alice pmpipe --param inv_max_shares=20` | 模式 Maker;USER `alice`、PIPELINE_USER `pmpipe`;参数 `[(clip_shares, 7), (inv_max_shares, 20)]` |
| PMC-001.3 | `--live 12345 k1 --no-presign --daily-loss 20.5 --daily-loss-resumes 3 --day-loss-carry 2026-10-29 -3.5` | 模式 Live:UID `12345`、目录 `k1`、`presign = false`、`daily_loss_abs = 20.5 USDC (20 500 000)`、resumes 3、carry `(20 755, −3.5 USDC (−3 500 000))`、无附加账户 |
| PMC-001.4 | `--live 12345 k1` | `presign = true`;carry、`daily_loss_abs`、resumes 均为 `None` |
| PMC-001.5 | `--account acct-b:888:any k3 --live 12345 k1 --live-account 777 k2 --account acct-c:999:any k4` | 附加账户依次为 `acct-b`/888/k3/any、无标签/777/k2/any、`acct-c`/999/k4/any;标签 `[pm-live, acct-b, pm-live-3, acct-c]` |

### PMC-002 不合法的命令行被拒绝并给出原因

- 规则:[命令行](README.md#命令行)
- 级别:table
- 里程碑:M6
- 前置:位置参数 `out hot 10.0.0.5 ens5`;`L` = `--live 12345 k1` 加位置参数
- 实现:`tools/hft-pmcfg/tests/cli.rs::pmc_002_invalid_command_lines`

| 行 | 输入 | 期望(错误信息含) |
|---|---|---|
| PMC-002.1 | 3 个位置参数 | `expected OUTPUT_DIR` |
| PMC-002.2 | 7 个位置参数 | `got 7 arguments` |
| PMC-002.3 | `--maker --live 1 k1` | `exclusive` |
| PMC-002.4 | `--param clip_shares=7`,无模式选项 | `--param needs --maker or --live` |
| PMC-002.5 | `--no-presign`,无 `--live` | `--no-presign needs --live` |
| PMC-002.6 | `--live-account 7 k2`,无 `--live` | `needs --live` |
| PMC-002.7 | `--daily-loss 20`,无 `--live` | `--daily-loss needs --live` |
| PMC-002.8 | `--daily-loss 0` + L | `> 0` |
| PMC-002.9 | `--daily-loss 1.0000001` + L(7 位小数) | `at most 6 places` |
| PMC-002.10 | `--daily-loss-resumes 21` + L | `0..=20` |
| PMC-002.11 | `--day-loss-carry 2026-10-29 0` + L | `< 0` |
| PMC-002.12 | `--day-loss-carry 2026-13-01 -1` + L | `bad date` |
| PMC-002.13 | `--maker --param clip_shares` | `expected NAME=RAW` |
| PMC-002.14 | `--maker --param clip_shares=0.5` | `integer` |
| PMC-002.15 | `--maker --param no_such_field=1` | `not a binary-maker parameter` |
| PMC-002.16 | `--maker --param clip_shares=0`(低于 schema 下限) | `outside` |
| PMC-002.17 | `--frobnicate` | `unknown option --frobnicate` |
| PMC-002.18 | `--daily-loss 5 --daily-loss 6` + L | `given twice` |
| PMC-002.19 | `--live 12345`(缺 KEYS_DIR) | `--live: missing KEYS_DIR` |
| PMC-002.20 | `--day-loss-carry 2026-10-29 -1`,无 `--live` | `--day-loss-carry needs --live` |

### PMC-003 `--account` 条目的解析与账户约束

- 规则:[多账户](README.md#多账户)
- 级别:table
- 里程碑:M6
- 前置:`--live 12345 k1` 与位置参数 `out hot 10.0.0.5 ens5`,再加下表的条目
- 实现:`tools/hft-pmcfg/tests/cli.rs::pmc_003_account_entries`

| 行 | 输入 | 期望 |
|---|---|---|
| PMC-003.1 | `--account a.b_c-1:0f5e:8d2a:any k2` | 接受:标签 `a.b_c-1`、UID `0f5e:8d2a`、角色 Any |
| PMC-003.2 | `acct`、`acct:888`、`:888:any`、`acct::any`(各带目录 k2) | 拒绝,含 `expected LABEL:UID:ROLE` |
| PMC-003.3 | `--account acct:888:any` 无 KEYS_DIR | 拒绝 |
| PMC-003.4 | 角色 `maker`、`taker`、`up`、`down`、`standby`、`ANY`、空 | 拒绝,含 ``only `any` is supported`` |
| PMC-003.5 | 标签 `sim-x`;标签 `a/b` | 拒绝,分别含 `sim-`、`[A-Za-z0-9._-]` |
| PMC-003.6 | 标签 `pm-live`;标签 `pm-live-3` 且其后一条 `--live-account`(生成 `pm-live-3`);两条标签 `a`;UID `12345`(与 `pm-live` 同);`--live-account 777` 与 UID 777 的 `--account` | 前三者拒绝含 `used twice`,后两者含 `used by two accounts` |
| PMC-003.7 | 11 条附加账户(`--live-account` 与 `--account` 交替);12 条 | 前者接受,附加账户 11 个;后者拒绝,含 `at most 12 accounts` |

### PMC-004 多账户实盘配置:每个账户进 refdata、凭据表、密钥要求与风控

- 规则:[多账户](README.md#多账户)、[库](README.md#库)
- 级别:unit
- 里程碑:M6
- 前置:`PmLive`:UID `12345`、目录 k1;附加账户 `LiveAccount::shorthand(777, k2)`、`acct-b`/888/k3/any、`acct-c`/999/k4/any;`write_polymarket_maker_live`;对照组附加账户全部为 shorthand(777/k2、888/k3、999/k4)
- 步骤:
  1. 生成配置,读 `refdata.bin`(`RefData::load`)、`core.toml`、`engine.bin`(`EngineSetup::decode_blob`)、`risk_params.bin`(`RiskParams::decode_blob`)
  2. 生成对照组配置
- 期望:
  - .1 `accounts_of(StrategyId 1)` = [1, 2, 3, 4];标签依次 `pm-live`、`pm-live-2`、`acct-b`、`acct-c`;全部 Live、`is_prod`、`bound_instance = 1`;Polymarket UID 依次 12345、777、888、999
  - .2 venue `pm` 的 `accounts` 4 项,`account` 依次 1–4,`api_key_path`/`rest_secret_path`/`passphrase_path` 在 k1–k4 下
  - .3 `keys_required` = [(1, pm, 3), (2, pm, 3), (3, pm, 3), (4, pm, 3)];风控账户行与 venue 行各 4 条
  - .4 对照组账户 3、4 的标签为 `pm-live-3`、`pm-live-4`;两组的 `engine.bin`、`risk_params.bin`、`strategy_params.bin` 逐字节相同
- 实现:`tools/hft-pmcfg/tests/output.rs::pmc_004_multi_account_live_output`

### PMC-005 工具的输出与库函数逐字节相同

- 规则:[命令行](README.md#命令行)、[概要](README.md#概要)
- 级别:unit
- 里程碑:M6
- 前置:已构建的 `hft-pmcfg` 二进制;位置参数 `<目录> <hot> 10.0.0.5 ens5`
- 步骤:
  1. 对每行分别运行工具与对应的库函数,写到两个目录
  2. 运行 `hft-pmcfg --no-presign <bad> <hot> 10.0.0.5 ens5`
- 期望:
  - 步骤 1 每行退出 0、标准输出为工具目录下 `core.toml` 的路径;`core.toml`、`principals.toml`、`refdata.bin`、`engine.bin`、`risk_params.bin`、`strategy_params.bin` 两边逐字节相同。行:.1 无模式选项 ↔ `write_polymarket_sim`;.2 `--maker --param clip_shares=7` ↔ `write_polymarket_maker_sim_with`;.3 `--live 12345 k1 --no-presign --daily-loss 20 --daily-loss-resumes 3 --day-loss-carry 2026-10-29 -3.5` ↔ `write_polymarket_maker_live`(`presign = false`、20 USDC、3、(20 755, −3.5 USDC));.4 `--live 12345 k1 --live-account 777 k2 --account acct-b:888:any k3 --param clip_shares=7` ↔ `write_polymarket_maker_live_with`
  - .5 步骤 2 退出 2,标准输出为空,标准错误含 `--no-presign needs --live` 与 `usage: hft-pmcfg`;`<bad>` 不存在
- 实现:`tools/hft-pmcfg/tests/output.rs::pmc_005_tool_output_equals_library`

### PMC-006 `engine_base` 与 SeqRig 的 `engine_setup` 编码相同

- 规则:[实现说明](README.md#实现说明)、[库](README.md#库)
- 级别:unit
- 里程碑:M6
- 前置:容量 (16, 32, 1024, 256, 4, 16, 4);ledger 缺省加 `par_assets = [USDC]`;epoch 1、1 GHz、校准对 (0, 0)
- 步骤:对 .1 book-observer 模拟配置的 refdata(venues [pm])、.2 三账户实盘配置的 refdata(venues [pm, bn]),分别以 `engine_base` 与 hft-testkit `seq_rig::engine_setup` 构建并 `encode_blob(1, Dev2, 0)`
- 期望:两种构建的 ECF1 字节逐字节相同
- 实现:`tools/hft-pmcfg/tests/output.rs::pmc_006_engine_base_equals_seq_rig_setup`

## 验收

PMC-001–PMC-006 全部通过。迁移验收(2026-10-30,一次性):迁移前的 `pm_sim_config` 与迁移后的 `hft-pmcfg`、`pm_sim_config` 对 7 组命令行(无模式选项;`--maker` 带 USER/PIPELINE_USER;`--live`;`--live` + `--live-account` + `--param`;`--live --no-presign --daily-loss 20 --daily-loss-resumes 3 --day-loss-carry 2026-10-29 -3.5`;`--maker` 两个 `--param`;`--live` + 两个 `--live-account`)的 42 个输出文件 `cmp` 全部相同。
