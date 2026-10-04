---
component: pipeline
prefix: PIP
spec: README.md
---

# pipeline 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | PIP-026、PIP-070、PIP-100、PIP-118、PIP-168、PIP-175 |
| 目录 | 不适用:非规范 |
| 使用本组件 | PIP-011、PIP-035、PIP-151、PIP-162–PIP-166、PIP-183 |
| 一轮的步骤 | PIP-017、PIP-022–PIP-024、PIP-029–PIP-040、PIP-045–PIP-051、PIP-113、PIP-116、PIP-139、PIP-140、PIP-150、PIP-172、PIP-173、PIP-182 |
| 插件接口 | PIP-013、PIP-015–PIP-025、PIP-028、PIP-029、PIP-033、PIP-040、PIP-069、PIP-143、PIP-145、PIP-148、PIP-162 |
| 插件的打包与发现 | PIP-009、PIP-011–PIP-016、PIP-027、PIP-028、PIP-070 |
| schema 校验与编码 | PIP-060–PIP-069、PIP-164 |
| GroupId 分配 | PIP-006、PIP-080–PIP-090、PIP-184 |
| 实例配置 | PIP-001–PIP-010、PIP-119、PIP-121、PIP-166 |
| 参数通道 | PIP-025、PIP-034、PIP-036、PIP-037、PIP-085、PIP-100–PIP-120、PIP-183、PIP-185、PIP-186、PIP-181、MD-7、MD-8、CTP-101、CTP-111、CTP-113、CTP-114、CTP-118、CTP-124 |
| Model 预热 | PIP-018、PIP-130–PIP-151、PIP-178、PIP-181、MD-24 |
| 输出与版本库 | PIP-007、PIP-030、PIP-089、PIP-090、PIP-112–PIP-115、PIP-120、PIP-142、PIP-160、PIP-161、PIP-180、PIP-182 |
| 交易宇宙候选 | PIP-049、PIP-167–PIP-169 |
| 实现说明 | PIP-008、PIP-019、PIP-026、PIP-101、PIP-106、PIP-110–PIP-112、PIP-121、PIP-138、PIP-170–PIP-173、PIP-178–PIP-180 |
| 延迟与资源契约 | PIP-008、PIP-176–PIP-178 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | PIP-026、PIP-068、PIP-118、PIP-174、PIP-175 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

**夹具**(各用例的"前置"按名字引用;实现在 `python/pipeline/tests/conftest.py`,参数一律显式赋值,不取配置默认值;用例改动其中某项时在"前置"里写出新值):

- `rd_pf`:测试 refdata。交易所 `EA`、`EB`、`EC`;venue `vA-spot`(`VenueId 1`,`EA`,现货)、`vA-perp`(2,`EA`,USDT 永续)、`vB-perp`(3,`EB`,USDT 永续)、`vC-spot`(4,`EC`,现货);资产 BTC(`px_exp = −2`,`qty_exp = −8`)、ETH(`px_exp = −2`,`qty_exp = −6`)、XRP(`px_exp = −5`,`qty_exp = −1`);交易宇宙内的 inst:`BTC/vA-spot`(`InstId 11`,`min_qty` 0.00001)、`BTC/vB-perp`(21,`min_qty` 0.001)、`BTC/vA-perp`(31,`min_qty` 0.001)、`BTC/vC-spot`(41,`min_qty` 0.00001)、`ETH/vA-spot`(12,`min_qty` 0.0001)、`ETH/vB-perp`(22,`min_qty` 0.01);`XRP/vA-spot`、`XRP/vB-perp` 不在交易宇宙内。账户:`pf-live-1`(`kind = Live`,venues {vA-spot, vA-perp, vB-perp, vC-spot},绑定实例 `live-a`)、`sim-pf-1`(Sim,同 venues,绑定 `sim-a`)、`sim-pf-2`(Sim,绑定 `sim-b`)、`sim-np-1`(Sim,绑定 `sim-np`)。refdata `Instances`:`sim-a` = 7、`live-a` = 8、`sim-b` = 9、`sim-np` = 10。
- `S_pf`:测试 schema `pip-fix`(`kind_id = schema_id = 65296`,`schema_version = 2`;规范 JSON 由 SDK `params::schema_json` 生成后提交,无 `notes`/`report`/`cmds`)。字段(原始定点值):

  | ix | 名称 | `Unit` | min | max | default | `per_group` | `structural` | `manual_only` | `relax` | `since` |
  |---|---|---|---|---|---|---|---|---|---|---|
  | 0 | `open_th` | Rate | 0 | 100 000 000 | 5 000 000 | 是 | 否 | 否 | Down | 1 |
  | 1 | `max_notional` | Notional | 0 | 1 000 000 000 000 | 10 000 000 000 | 是 | 否 | 否 | Up | 1 |
  | 2 | `cooldown_ms` | DurationMs | 0 | 600 000 | 20 000 | 否 | 否 | 否 | Down | 1 |
  | 3 | `pct_bin` | Rate | 10 000 | 1 000 000 | 100 000 | 否 | 是 | 否 | None | 1 |
  | 4 | `force_exit` | Bool | 0 | 1 | 0 | 是 | 否 | 是 | None | 1 |
  | 5 | `mode` | Enum(`a`, `b`) | 0 | 1 | 0 | 否 | 否 | 否 | None | 2 |
  | 6 | `entry_qty` | Qty | 0 | 10 000 000 000 | 0 | 是 | 否 | 否 | Up | 1 |
  | 7 | `max_groups` | Count | 0 | 64 | 8 | 否 | 否 | 否 | Up | 1 |
  | 8 | `score` | Rate | 0 | 1 000 000 000 | 0 | 是 | 否 | 否 | None | 1 |
- `S_tk`、`S_tp`:hft-testkit 两个测试策略 `probe-taker`、`probe-target` 导出的规范 schema JSON(kind_id 取其 `StrategyDesc`);只用于"schema 通用"用例,字段按属性(`per_group`、`min/max`)从载入的 schema 中选取。
- `D_schemas`:schema 目录 `python/pipeline/tests/fixtures/schemas/`:`index.json`(按 kind_id 升序列出 `S_tk`、`S_tp`、`S_pf`)+ 三个 `<kind_id>-<name>.schema.json`,经 `hft_py.params.load_schemas` 载入。
- `P_pf`:测试插件包 `python/pipeline/tests/fixtures/plugins/pf_plugin/`(分发名 `hft-plugin-pf`,版本 `0.0.1`,entry point `[project.entry-points."hft_pipeline.plugins"] pf = "hft_plugin_pf:PLUGIN"`,测试前 `pip install -e`):`strategy_kind = 65296`,`api_version = A`(`A` = 框架的 `hft_pipeline.api.API_VERSION`,夹具要求 `A ≥ 2`),`owned_fields = {open_th, max_notional, cooldown_ms, score}`,`required_inputs` = (`I_px`:REST `https://ex.test/px`,必需,令牌桶域 `ex.test`;`I_vol`:REST `https://ex.test/vol`,非必需,同一域),`warmup_inputs` = (`W_col`:collector Parquet;`W_rest`:REST `https://ex.test/kline`)。`compute`/`warmup` 是脚本化的:返回用例"前置"给出的 `PluginOutput`/`WarmupOut`,并记录每次调用收到的 `Snapshot`/`WarmupSnapshot`。
- `cfg_pf`:`/etc/hft/pipeline.toml` 的测试副本(路径都在 pytest 的 `tmp_path` 下):`schema = 4`;`host = "h1"`;`hft.instances` = [`{name = "prod", journal_cold, journal_ram, ctl_socket = <tmp>/prod.sock, s3_prefix = "hft"}`, `{name = "sim", …, ctl_socket = <tmp>/sim.sock, s3_prefix = "hft"}`];`instances` = `sim-a`(`core = "sim"`,`strategy_kind = 65296`,`strategy_id = 7`,`account = "sim-pf-1"`,`plugin = "pf"`,`cadence_min = 10`,`policy = "policy/sim-a.toml"`,`venues = {spot = "vA-spot", perp = "vB-perp"}`)、`live-a`(`core = "prod"`,65296,`strategy_id = 8`,`pf-live-1`,`pf`,10,`policy/live-a.toml`,同 venues)、`sim-b`(`sim`,65296,9,`sim-pf-2`,`pf`,10,`policy/sim-b.toml`,同 venues)、`sim-np`(`sim`,`S_tk` 的 kind_id,10,`sim-np-1`,无 `plugin`,10,`policy/sim-np.toml`,`venues = {a = "vA-spot", b = "vB-perp"}`);`pairs = [{name = "p1", live = "live-a", sim = "sim-a"}]`;`paths.state/out/versions/overrides` = `<tmp>/state`、`<tmp>/out`、`<tmp>/versions`、`<tmp>/overrides`;`s3.params_prefix = "s3://test-bucket/{prefix}/{host}/{core}/private/params/"`(假 S3);`http.buckets = {"ex.test" = {rate_per_s = 10, burst = 20}}`;`schemas.dir = D_schemas`;`warmup.auto = true`;`warmup.sources = ["collector", "rest"]`;`warmup.collector_prefix = "s3://test-bucket/collector/"`。
- `pol_pf`:各实例的 policy 文件,内容为 `[fields] cooldown_ms = "30000"`、顶层插件阈值 `min_vol = "1000000"`,无 `[[groups]]`;`sim-np` 的 policy 另有 `[[groups]]` 一项:BTC,腿 (`vA-spot`, 现货, `BTC/vA-spot`)、(`vB-perp`, 永续, `BTC/vB-perp`),`open_allowed = true`。
- `js0`:框架读到的 journal 快照(`hft_py` 读取由测试替身返回):`snap.ts = T0 = 1 790 812 800 000 000 000`(2026-10-01T00:00:00Z);`sim-a` 当前生效 `StrategyParams` `version = 41`,group `G1` = `gid(7, 1)` = `0x0700_0001`(BTC;腿 0 `BTC/vA-spot`、腿 1 `BTC/vB-perp`;`open_allowed = 1`)、`G2` = `gid(7, 2)` = `0x0700_0002`(ETH;腿 0 `ETH/vA-spot`、腿 1 `ETH/vB-perp`;`open_allowed = 1`),body 全为 schema 默认值;`live-a` 生效 `version = 12`,group `gid(8, 1)`(BTC,同样两腿),全部 cell 期望模式 `Live`;各 group 每腿净量 0、无在途;账户级(`GROUP_NONE`)持仓为空;无 `Note{ModelCheckpointDiscarded}`、`Note{ModelGroupReset}`。
- `gj0`:`versions/sim-a/groups.json` = `{next_ctr = 3, ids = {1: fp(G1), 2: fp(G2)}}`;`versions/sim-a/index.jsonl` 最后一项 `version = 41`(本包下发、`Applied`);`live-a` 的对应文件 `next_ctr = 2`、最后一项 `version = 12`。
- `out_pf(…)`:插件输出的简写。默认 `out_pf()` = `PluginOutput{groups = (G_BTC, G_ETH), fields = {cooldown_ms: 20000}, universe_candidates = (), ok = True, notes = ()}`,`G_BTC` = `GroupOut{asset = "BTC", legs = ((vA-spot, Spot, BTC/vA-spot), (vB-perp, Perp, BTC/vB-perp)), open_allowed = True, fields = {}, reason = "keep"}`,`G_ETH` 同形(ETH 两腿)。`out_pf()` 与 `js0` 中 `sim-a` 的生效参数内容相同(同一组指纹、`open_allowed`、全部字段等于默认值,无覆盖表项)。插件对 `live-a` 的默认输出是 `out_pf()` 去掉 `G_ETH`,与 `live-a` 的生效参数内容相同。用例只写与它们的差异。
- `FakeCtl`:替代 `hftctl.py` 的假客户端,记录每个请求(`type`、`issuer`、`preview`、`confirmed`、`base_version`、`issued_at_ns`、`expires_at_ns`、`blob`、`chunk_no`、`last`、目标 socket),默认对每个请求回 `{status = "ok", code = "Applied"}`;用例可按序脚本化返回值。
- 记法:插件输出与覆盖项里写出的数值一律是 `Decimal`/十进制字符串(PIP-061 的 `float` 行除外);`gid(s, n)` = `(s << 24) | n`;Rate 写成百分比或 bp 时括号内给出 raw(1e-9);Notional 的 raw 指数 −6;"一轮" = 对一个实例执行"一轮的步骤"1–8;"告警 X" = 该实例 `status.json` 的 `alerts` 新增一项 `code = X`(级别按 README 的码表)。

### PIP-001 实例配置各键的默认值

- 规则:[实例配置](README.md#实例配置)
- 级别:table
- 里程碑:M4
- 前置:最小配置文件,只含必填键:`schema = 4`、`host = "h1"`、`hft.instances = [{name = "sim", journal_cold = "/x/c", journal_ram = "/x/r", ctl_socket = "/x/s.sock"}]`、`instances = [{name = "sim-a", core = "sim", strategy_kind = 65296, strategy_id = 7, account = "sim-pf-1", cadence_min = 10, venues = {spot = "vA-spot", perp = "vB-perp"}}]`、`http.buckets = {"ex.test" = {rate_per_s = 10, burst = 20}}`;`rd_pf`;`P_pf` 已安装;本用例是默认值的唯一用例
- 实现:`python/pipeline/tests/test_config.py::test_pip_001_config_defaults`

| 行 | 键 | 期望(加载后的值) |
|---|---|---|
| PIP-001.1 | `hft.instances[0].s3_prefix` | `"hft"` |
| PIP-001.2 | `pairs` | `[]` |
| PIP-001.3 | `paths.state` | `/var/lib/hft/pipeline` |
| PIP-001.4 | `paths.out` | `/var/lib/hft/pipeline/out` |
| PIP-001.5 | `paths.versions` | `/var/lib/hft/pipeline/versions` |
| PIP-001.6 | `paths.overrides` | `/var/lib/hft/pipeline/overrides` |
| PIP-001.7 | `s3.params_prefix` | `"s3://gwx-trading-data/{prefix}/{host}/{core}/private/params/"` |
| PIP-001.8 | `schemas.dir` | `/opt/hft/current-cold/schemas/strategies` |
| PIP-001.9 | `warmup.auto` | `true` |
| PIP-001.10 | `warmup.sources` | `["collector", "rest"]` |
| PIP-001.11 | `instances[0].plugin` | 缺省;插件按 `strategy_kind = 65296` 查得 entry point `pf` |
| PIP-001.12 | `instances[0].policy` | `policy/sim-a.toml` |

### PIP-002 缺必填键时启动退出 2

- 规则:[实例配置](README.md#实例配置)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;每行删去一个键后执行 `hft-pipeline run --config <文件>`
- 实现:`python/pipeline/tests/test_config.py::test_pip_002_missing_required_key_exits_2`

| 行 | 删去的键 | 期望 |
|---|---|---|
| PIP-002.1 | `host` | 退出码 2;stderr 含 `host`;未连接任何 ctl socket |
| PIP-002.2 | `hft.instances` | 退出码 2;stderr 含 `hft.instances` |
| PIP-002.3 | `instances` | 退出码 2;stderr 含 `instances` |
| PIP-002.4 | `http.buckets` | 退出码 2;stderr 含 `http.buckets` |
| PIP-002.5 | `instances[sim-a].strategy_id` | 退出码 2;stderr 含 `sim-a` 与 `strategy_id` |
| PIP-002.6 | 不删(对照) | 进程进入调度循环,10 s 内未退出 |

### PIP-003 角色 venue 属于同一交易所时加载失败

- 规则:[实例配置](README.md#实例配置)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`rd_pf`;每行替换 `sim-a.venues`
- 实现:`python/pipeline/tests/test_config.py::test_pip_003_role_venues_distinct_exchanges`

| 行 | `sim-a.venues` | 期望 |
|---|---|---|
| PIP-003.1 | `{spot = "vA-spot", perp = "vA-perp"}`(都在 `EA`) | 退出码 2;状态写明 `sim-a` 与交易所 `EA` |
| PIP-003.2 | `{spot = "vA-spot", perp = "vB-perp"}` | 加载成功 |
| PIP-003.3 | `{a = "vA-spot", b = "vB-perp", c = "vC-spot"}`(三个交易所) | 加载成功 |
| PIP-003.4 | `{a = "vA-spot", b = "vB-perp", c = "vA-perp"}`(`a`、`c` 同在 `EA`) | 退出码 2 |
| PIP-003.5 | `{spot = "vZ-spot"}`(refdata 中没有该 venue) | 退出码 2;状态写明 `vZ-spot` |

### PIP-004 校准对两个实例的策略种类不同时加载失败

- 规则:[实例配置](README.md#实例配置)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`,另加 `pairs` 一项 `{name = "p2", live = "live-a", sim = "sim-np"}`(`live-a` 为 65296,`sim-np` 为 `S_tk` 的 kind_id)
- 步骤:
  1. `hft-pipeline run --config <文件>`
- 期望:
  - 退出码 2;状态写明 `p2` 与两个 `strategy_kind`
  - 删去 `p2` 后同一文件加载成功
- 实现:`python/pipeline/tests/test_config.py::test_pip_004_pair_kind_mismatch`

### PIP-005 两个实例共享同一 policy 文件只在同一校准对内允许

- 规则:[实例配置](README.md#实例配置)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;每行设定各实例的 `policy`
- 实现:`python/pipeline/tests/test_config.py::test_pip_005_shared_policy_only_within_pair`

| 行 | 设定 | 期望 |
|---|---|---|
| PIP-005.1 | `live-a.policy = sim-a.policy = "policy/p1.toml"`(同在 `p1`) | 加载成功;两个实例的 `policy_version` 相同 |
| PIP-005.2 | `sim-a.policy = sim-b.policy = "policy/s.toml"`(不在同一校准对) | 退出码 2;状态写明 `sim-a`、`sim-b` 与该文件 |
| PIP-005.3 | `live-a.policy = sim-b.policy = "policy/x.toml"`(`sim-b` 不在 `p1`) | 退出码 2 |

### PIP-006 strategy_id 只接受 GroupId::compose 允许的范围

- 规则:[实例配置](README.md#实例配置)、[GroupId 分配](README.md#groupid-分配)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;每行替换 `sim-a.strategy_id`
- 实现:`python/pipeline/tests/test_config.py::test_pip_006_strategy_id_range`

| 行 | `strategy_id` | 期望 |
|---|---|---|
| PIP-006.1 | 0 | 退出码 2 |
| PIP-006.2 | 1 | 加载成功;该实例分配的第一个新 id = `0x0100_0003`(`gj0` 的 `next_ctr = 3`) |
| PIP-006.3 | 254 | 加载成功;第一个新 id = `0xFE00_0003` |
| PIP-006.4 | 255 | 退出码 2 |

### PIP-007 s3.params_prefix 按实例展开模板

- 规则:[实例配置](README.md#实例配置)、[输出与版本库](README.md#输出与版本库)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`,`s3.params_prefix = "s3://test-bucket/{prefix}/{host}/{core}/private/params/"`;每行设定 `hft.instances[*].s3_prefix` 后跑一轮
- 实现:`python/pipeline/tests/test_config.py::test_pip_007_params_prefix_template`

| 行 | 设定与实例 | 期望(版本库上传的对象前缀) |
|---|---|---|
| PIP-007.1 | `sim` 的 `s3_prefix = "hft"`,实例 `sim-a` | `s3://test-bucket/hft/h1/sim/private/params/` |
| PIP-007.2 | `prod` 的 `s3_prefix = "hft-testnet"`,实例 `live-a` | `s3://test-bucket/hft-testnet/h1/prod/private/params/` |
| PIP-007.3 | `prod` 的 `s3_prefix` 缺省,实例 `live-a` | `s3://test-bucket/hft/h1/prod/private/params/` |

### PIP-008 外部请求按域名令牌桶限速

- 规则:[实例配置](README.md#实例配置)、[实现说明](README.md#实现说明)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:integ
- 里程碑:M4
- 前置:`cfg_pf`,`http.buckets = {"ex.test" = {rate_per_s = 10, burst = 20}, "other.test" = {rate_per_s = 1, burst = 1}}`;假 HTTP 服务器记录每个请求的到达时刻;`sim-a`、`live-a`、`sim-b` 三个实例同时到期,插件的 `I_px` 每轮各需 30 个 `ex.test` 请求,另 1 个 `other.test` 请求
- 步骤:
  1. 三个实例各跑一轮
- 期望:
  - `ex.test` 共 90 个请求;任意时刻起 1 s 窗口内到达数 ≤ 20,且从第 21 个请求起相邻请求间隔 ≥ 100 ms(三个实例共用同一个桶)
  - `other.test` 的 3 个请求两两间隔 ≥ 1 s
- 实现:`python/pipeline/tests/test_http.py::test_pip_008_token_bucket_per_domain`

### PIP-009 instances 显式给出 plugin 时按名字取插件,不按种类

- 规则:[实例配置](README.md#实例配置)、[插件的打包与发现](README.md#插件的打包与发现)
- 级别:unit
- 前置:`cfg_pf`;另装一个测试插件 `pf2`(entry point `pf2`,`strategy_kind = 65297`,`owned_fields = {}`);`D_schemas` 另含 kind 65297 的 schema(字段同 `S_pf`);`sim-b.strategy_kind = 65297`、`sim-b.plugin = "pf2"`;`sim-a.plugin` 缺省
- 里程碑:M4
- 步骤:
  1. 加载配置并对 `sim-a`、`sim-b` 各跑一轮
- 期望:
  - `sim-a` 一轮内调用的是 `pf` 的 `compute`(按 `strategy_kind = 65296` 查得),`sim-b` 调用的是 `pf2` 的 `compute`
  - `status.json` 中两个实例各自记录所用插件的 entry point 名
- 实现:`python/pipeline/tests/test_plugins.py::test_pip_009_explicit_plugin_name`

### PIP-010 缺省 policy 路径为 policy/<实例名>.toml

- 规则:[实例配置](README.md#实例配置)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf` 去掉 `sim-a.policy`;`policy/sim-a.toml` 内容 `[fields] cooldown_ms = "45000"`
- 步骤:
  1. `sim-a` 跑一轮,插件输出 `out_pf()` 去掉 `cooldown_ms`
- 期望:
  - 下发的 blob 中 `cooldown_ms` = 45 000
  - `status.json` 的 `policy.version` = `policy/sim-a.toml` 内容的 SHA-256 十六进制
- 实现:`python/pipeline/tests/test_config.py::test_pip_010_default_policy_path`

### PIP-011 插件经 entry point 被发现并按 strategy_kind 建表

- 规则:[插件的打包与发现](README.md#插件的打包与发现)、[使用本组件](README.md#使用本组件)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`D_schemas`;venv 中只装了 `P_pf`(`hft-plugin-pf 0.0.1`)
- 步骤:
  1. `hft-pipeline plugins --config <cfg_pf>`
- 期望:
  - 退出码 0;stdout 恰好一行插件:entry point `pf`、`strategy_kind = 65296`、版本 `0.0.1`、来源 wheel `hft_plugin_pf-0.0.1`
  - 框架插件表 `{65296: pf}`;`sim-a`、`live-a`、`sim-b` 解析到 `pf`,`sim-np` 解析为"无插件"
- 实现:`python/pipeline/tests/test_plugins.py::test_pip_011_discover_entry_point`

### PIP-012 两个插件声明同一 strategy_kind 时启动退出 2

- 规则:[插件的打包与发现](README.md#插件的打包与发现)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`;另装测试插件 `pf_dup`(entry point `pf_dup`,`strategy_kind = 65296`)
- 步骤:
  1. `hft-pipeline run --config <cfg_pf>`
- 期望:
  - 退出码 2;状态与 stderr 写明 `65296`、`pf`、`pf_dup`
  - 未向任何 ctl socket 发请求
- 实现:`python/pipeline/tests/test_plugins.py::test_pip_012_duplicate_kind_exits_2`

### PIP-013 插件 api_version 高于框架时启动退出 2

- 规则:[插件接口](README.md#插件接口)、[插件的打包与发现](README.md#插件的打包与发现)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;每行把 `P_pf.api_version` 设为给定值后启动
- 实现:`python/pipeline/tests/test_plugins.py::test_pip_013_api_version_bounds`

| 行 | `api_version` | 期望 |
|---|---|---|
| PIP-013.1 | `A − 1` | 载入成功 |
| PIP-013.2 | `A` | 载入成功 |
| PIP-013.3 | `A + 1` | 退出码 2;状态写明 `pf`、`A + 1` 与 `A` |

### PIP-014 插件的 strategy_kind 不在 schema index.json 中时启动退出 2

- 规则:[插件的打包与发现](README.md#插件的打包与发现)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`;另装测试插件 `pf_orphan`(`strategy_kind = 65400`);`D_schemas` 的 `index.json` 无 65400
- 步骤:
  1. `hft-pipeline run --config <cfg_pf>`
- 期望:
  - 退出码 2;状态写明 `pf_orphan` 与 `65400`
- 实现:`python/pipeline/tests/test_plugins.py::test_pip_014_kind_not_in_index`

### PIP-015 owned_fields 含结构字段或只由人工设置的字段时插件载入失败

- 规则:[插件接口](README.md#插件接口)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`S_pf`;每行替换 `P_pf.owned_fields`
- 实现:`python/pipeline/tests/test_plugins.py::test_pip_015_owned_fields_restrictions`

| 行 | `owned_fields` | 期望 |
|---|---|---|
| PIP-015.1 | `{open_th, pct_bin}`(`pct_bin` 结构字段) | 退出码 2;状态写明 `pf` 与 `pct_bin` |
| PIP-015.2 | `{open_th, force_exit}`(`force_exit` `manual_only`) | 退出码 2;状态写明 `force_exit` |
| PIP-015.3 | `{open_th, max_notional, cooldown_ms, score}` | 载入成功 |

### PIP-016 插件 import 框架内部模块时 CI 的 import 检查失败

- 规则:[插件接口](README.md#插件接口)、[插件的打包与发现](README.md#插件的打包与发现)
- 级别:table
- 里程碑:M4
- 前置:CI `ops-py` 作业的 import 检查(对 `strategies/*/pipeline` 与 `tests/fixtures/plugins/*` 运行);每行是一个只含一条 import 的测试插件模块
- 实现:`python/pipeline/tests/test_plugins.py::test_pip_016_plugin_import_contract`

| 行 | 插件中的 import | 期望 |
|---|---|---|
| PIP-016.1 | `from hft_pipeline.assemble import merge` | 检查退出码 ≠ 0,报出该文件与模块名 `hft_pipeline.assemble` |
| PIP-016.2 | `import hft_pipeline.publish` | 检查退出码 ≠ 0 |
| PIP-016.3 | `from hft_pipeline.api import GroupOut, PluginOutput` | 检查通过 |
| PIP-016.4 | `import decimal, statistics` | 检查通过 |

### PIP-017 没有插件的实例按 policy 静态 group 表、默认值与覆盖项生成参数

- 规则:[插件接口](README.md#插件接口)、[一轮的步骤](README.md#一轮的步骤)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`pol_pf`(`sim-np` 的静态 group 表:BTC 两腿,`open_allowed = true`);`sim-np` 当前无生效参数、`groups.json` 不存在、版本库为空;`overrides/sim-np.toml` 一项:`S_tk` 的一个 `per_group = false` 字段 F 设为该字段 `min`
- 步骤:
  1. `sim-np` 跑一轮
- 期望:
  - `FakeCtl` 收到一条 `SetParams`,`strategy = "sim-np"`、`account = "sim-np-1"`、`issuer = "pipeline"`;blob 经 `hft_py.params.decode` 得 1 个 group:`gid(10, 1)`,BTC,两腿 `BTC/vA-spot`、`BTC/vB-perp`,`open_allowed = 1`;F = 其 `min`;其余字段 = `S_tk` 默认值
  - 无插件调用;`status.json` 记录插件为"无"
- 实现:`python/pipeline/tests/test_round.py::test_pip_017_no_plugin_instance`

### PIP-018 插件没有 warmup 时触发预热只记状态

- 规则:[插件接口](README.md#插件接口)、[Model 预热](README.md#model-预热)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;`P_pf` 去掉 `warmup` 与 `warmup_inputs`;journal 出现 `sim-a` 的 `Note{ModelCheckpointDiscarded, v[1] = Missing(1)}`
- 步骤:
  1. `sim-a` 跑一轮
- 期望:
  - `FakeCtl` 没有收到 `InjectWarmup`
  - `status.json` 的 `warmup.result = "NoWarmup"`;`alerts` 中没有 `WarmupNotInjected`
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_018_plugin_without_warmup`

### PIP-019 同一快照两次 compute 的输出与组装的 blob 逐字节相同

- 规则:[插件接口](README.md#插件接口)、[实现说明](README.md#实现说明)
- 级别:prop
- 里程碑:M4
- 性质:对任一快照,`compute(snap, policy)` 调两次得到相等的 `PluginOutput`,框架由两次输出组装的 blob 逐字节相同;对同一快照副本(深拷贝)结果也相同
- 生成:快照:1–64 个资产的 `I_px` 价格(Decimal,8 位小数,0.01–100 000)、`I_vol`(存在或缺失)、0–64 个当前 group(随机 `open_allowed`、每腿净量 0 或 ±1 lot)、`snap.ts` ∈ [T0, T0 + 30 天];插件 = `tests/fixtures/plugins/` 中的参考插件 `pf_ref`(按价格排序选前 k 个资产,纯函数)
- 参考:断言两次输出 `==` 且 blob 字节相等
- 例数:PR 200;每夜 5 000
- 实现:`python/pipeline/tests/test_plugins.py::test_pip_019_compute_deterministic`

### PIP-020 同一快照两次 warmup 的块列表逐字节相同

- 规则:[插件接口](README.md#插件接口)
- 级别:prop
- 里程碑:M4
- 性质:对任一 `WarmupSnapshot`,`warmup(snap, policy)` 调两次得到相等的 `WarmupOut`(每批 `local_kind` 相同、`blocks` 逐字节相同)
- 生成:`snap.groups` ∈ {`None`, 1–8 个 GroupId 的列表};K 线 0–2 000 根(Decimal 价格);`snap.params` 取 `S_pf` 字段的随机合法值;插件 = 参考插件 `pf_ref` 的 `warmup`(每块 ≤ 4096 B)
- 参考:断言两次输出 `==`
- 例数:PR 200;每夜 5 000
- 实现:`python/pipeline/tests/test_plugins.py::test_pip_020_warmup_deterministic`

### PIP-021 插件的时间来自 snap.ts,系统时钟变化不影响输出

- 规则:[插件接口](README.md#插件接口)
- 级别:unit
- 里程碑:M4
- 前置:参考插件 `pf_ref`(其 `reason` 文本含 `snap.ts` 的 UTC 日期);`js0` 的 `snap.ts = T0`
- 步骤:
  1. 系统时钟(`time.time`、`datetime.now` 打桩)设为 2030-01-01,调用 `compute`
  2. 系统时钟设为 2020-01-01,再调用
- 期望:
  - 两次输出相等,每个 `GroupOut.reason` 含 `2026-10-01`
- 实现:`python/pipeline/tests/test_plugins.py::test_pip_021_time_from_snapshot`

### PIP-022 插件输出 owned_fields 之外的字段时拒绝该轮

- 规则:[插件接口](README.md#插件接口)、[一轮的步骤](README.md#一轮的步骤)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;每行替换插件输出
- 实现:`python/pipeline/tests/test_round.py::test_pip_022_unowned_field_rejects_round`

| 行 | 插件输出 | 期望 |
|---|---|---|
| PIP-022.1 | `out_pf()` 的全局 `fields` 加 `max_groups: 4`(不在 `owned_fields`) | 无 `SetParams`;告警 `RoundRejected`(P1),`detail` 含 `max_groups`;生效版本仍为 41 |
| PIP-022.2 | `G_BTC.fields` 加 `entry_qty: 0.01` | 无 `SetParams`;告警 `RoundRejected`,`detail` 含 `entry_qty` |
| PIP-022.3 | `out_pf()`(全部在 `owned_fields` 内) | 无告警;`out_pf()` 与生效参数相同 → 无 `SetParams`(见 PIP-116) |

### PIP-023 插件运行中输出结构字段或只由人工设置的字段时拒绝该轮

- 规则:[一轮的步骤](README.md#一轮的步骤)、[插件接口](README.md#插件接口)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;`P_pf.owned_fields` 照夹具(载入时通过);每行插件在 `compute` 输出中额外写入一个字段
- 实现:`python/pipeline/tests/test_round.py::test_pip_023_runtime_forbidden_field`

| 行 | 额外输出 | 期望 |
|---|---|---|
| PIP-023.1 | 全局 `pct_bin: 0.0002`(结构字段) | 无 `SetParams`;告警 `RoundRejected`,`detail` 含 `pct_bin` |
| PIP-023.2 | `G_BTC.fields` 加 `force_exit: 1`(`manual_only`) | 无 `SetParams`;告警 `RoundRejected`,`detail` 含 `force_exit` |
| PIP-023.3 | 全局 `mode: 1`(既不是结构字段也不是 `manual_only`,但不在 `owned_fields`) | 无 `SetParams`;告警 `RoundRejected`,`detail` 含 `mode` |

### PIP-024 一个 group 最多 4 条腿

- 规则:[插件接口](README.md#插件接口)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;refdata 另加交易所 `ED` 的现货 venue `vD-spot` 与 `BTC/vD-spot`(在宇宙内)、交易所 `EE` 的 `vE-spot` 与 `BTC/vE-spot`;插件输出 `out_pf()` 再加 group `G_BTC4`(BTC,腿数按行)
- 实现:`python/pipeline/tests/test_round.py::test_pip_024_max_four_legs`

| 行 | `G_BTC4.legs` | 期望 |
|---|---|---|
| PIP-024.1 | `BTC/vA-spot`、`BTC/vB-perp`、`BTC/vC-spot`、`BTC/vD-spot`(4 腿,交易所互不相同) | `SetParams` 中有该 group,4 条腿顺序与 `legs` 相同(`leg_ix` 0–3) |
| PIP-024.2 | 上一行再加 `BTC/vE-spot`(5 腿) | 无 `SetParams`;告警 `RoundRejected`,`detail` 含该 group 的资产与腿数 5 |

### PIP-025 policy 出现与 schema 字段同名的顶层键时该 policy 加载失败

- 规则:[插件接口](README.md#插件接口)、[参数通道](README.md#参数通道)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;每行替换 `policy/sim-a.toml`
- 实现:`python/pipeline/tests/test_policy.py::test_pip_025_policy_key_collision`

| 行 | `policy/sim-a.toml` | 期望 |
|---|---|---|
| PIP-025.1 | 顶层 `open_th = "0.01"` | `sim-a` 这一轮不调插件、无 `SetParams`;告警 `ConfigInvalid`(P1),`detail` 含 `open_th`;`live-a`、`sim-b` 照常跑 |
| PIP-025.2 | `[fields] open_th = "0.01"` | 加载成功;插件未输出 `open_th` 时下发值为 10 000 000 |
| PIP-025.3 | `[fields] no_such = "1"` | 告警 `ConfigInvalid`,`detail` 含 `no_such`;无 `SetParams` |
| PIP-025.4 | 顶层 `min_vol = "1000000"`(插件阈值) | 加载成功;`Policy` 对象中 `min_vol = Decimal("1000000")` |

### PIP-026 框架代码不按策略名或交易所名分支

- 规则:[插件接口](README.md#插件接口)、[实现说明](README.md#实现说明)、[依赖](README.md#依赖)、[概要](README.md#概要)
- 级别:unit
- 里程碑:M4
- 前置:`python/pipeline/src/hft_pipeline/` 全部源码;registry 的策略名表与 refdata 交易所名表
- 步骤:
  1. AST 扫描:找出与字符串字面量比较的表达式、`match` 的字面量分支、以整数字面量与 `strategy_kind` 比较的表达式
  2. 运行 `cargo xtask doc-check` 对 `python/pipeline/`
- 期望:
  - 步骤 1:没有一个字面量等于任何策略名、`kind_id` 或交易所名
  - 步骤 2:`python/pipeline/` 下零条策略族名报错
- 实现:`python/pipeline/tests/test_static.py::test_pip_026_no_strategy_or_exchange_branches`

### PIP-027 插件包的目录与 entry point 不符合约定时不被发现

- 规则:[插件的打包与发现](README.md#插件的打包与发现)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;每行另装一个测试插件包
- 实现:`python/pipeline/tests/test_plugins.py::test_pip_027_entry_point_group`

| 行 | 测试插件包 | 期望 |
|---|---|---|
| PIP-027.1 | entry point 写在组 `hft_pipeline.plugin`(少一个 s) | `hft-pipeline plugins` 不列出它;插件表不变 |
| PIP-027.2 | entry point 指向不存在的对象 `hft_plugin_x:NOPE` | 退出码 2;状态写明该 entry point 名与加载错误 |
| PIP-027.3 | 组 `hft_pipeline.plugins`,对象缺 `compute` | 退出码 2;状态写明缺少的成员 `compute` |

### PIP-028 hft_pipeline.api 只追加:已有名字与字段不被删改

- 规则:[插件接口](README.md#插件接口)、[插件的打包与发现](README.md#插件的打包与发现)
- 级别:table
- 里程碑:M4
- 前置:金样 `python/pipeline/tests/fixtures/api_surface.txt`(`hft_pipeline.api` 的公开名字、每个数据类的字段名与类型、`API_VERSION`);被测为当前 `hft_pipeline.api` 与该金样的比较函数;每行是一个对 api 的改动
- 实现:`python/pipeline/tests/test_api_surface.py::test_pip_028_api_append_only`

| 行 | 改动 | 期望 |
|---|---|---|
| PIP-028.1 | 无 | 比较通过 |
| PIP-028.2 | `GroupOut` 删去字段 `reason` | 比较失败,报出 `GroupOut.reason` |
| PIP-028.3 | `WarmupBatch.local_kind` 类型由 `int` 改为 `str` | 比较失败,报出该字段 |
| PIP-028.4 | 新增数据类 `FooSpec`,`API_VERSION` 加 1,金样同提交追加 | 比较通过 |
| PIP-028.5 | 新增数据类 `FooSpec`,`API_VERSION` 不变 | 比较失败,报出"新增名字须提升 API_VERSION" |

### PIP-029 快照给插件的 journal 输入齐全

- 规则:[一轮的步骤](README.md#一轮的步骤)、[插件接口](README.md#插件接口)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`rd_pf`、`gj0`;journal:`sim-a` 生效 `version = 41`(`js0` 的两个 group),`G1` 腿 0 净量 +0.002 BTC、腿 1 净量 −0.002 BTC,`G2` 腿 0 即时在途 0.01 ETH,`G2.open_allowed = 0`,账户 `sim-pf-1` 在 `ETH/vB-perp` 有账户级持仓 −0.004
- 步骤:
  1. `sim-a` 跑一轮,取插件 `compute` 收到的 `Snapshot`
- 期望:
  - `snap.ts = T0`;每个 group 的仓位、在途与 `open_allowed` 与上述一致(`G1` +0.002 / −0.002;`G2` 在途 0.01、`open_allowed = false`);账户级持仓含 `ETH/vB-perp` −0.004;生效参数版本 41 及按 `S_pf` 解码的字段值;交易宇宙为 `rd_pf` 中在宇宙内的 6 个 inst;`vA-spot`、`vA-perp` 的交易所为 `EA`,`vB-perp` 为 `EB`,`vC-spot` 为 `EC`
  - 插件声明的外部输入 `I_px`、`I_vol` 各读一次
- 实现:`python/pipeline/tests/test_snapshot.py::test_pip_029_snapshot_contents`

### PIP-030 一轮从快照到状态文件依次完成八步并下发新版本

- 规则:[一轮的步骤](README.md#一轮的步骤)、[输出与版本库](README.md#输出与版本库)
- 级别:integ
- 里程碑:M4
- 前置:`cfg_pf`、`rd_pf`、`js0`、`gj0`、`pol_pf`、`FakeCtl`;`overrides/` 为空;插件输出 `out_pf()`,其中 `G_BTC.fields = {open_th: 0.006}`
- 步骤:
  1. `sim-a` 跑一轮
- 期望:
  - `FakeCtl` 恰好一条请求:`SetParams{strategy = "sim-a", account = "sim-pf-1"}`,`issuer = "pipeline"`,`preview = false`,`confirmed = false`,`base_version = "41"`;blob 头 `version = 42`
  - blob 解码:group `G1`(`0x0700_0001`)的覆盖表恰有一项 `open_th` = 6 000 000;`G2` 无覆盖;默认值:`cooldown_ms` = 20 000(插件值盖过 `pol_pf` 的 30 000),其余为 `S_pf` 默认
  - `status.json`:`ok = true`;`step_ms` 含 8 项(步骤 1–8 各一项);`field_changes` = `[{group: 0x0700_0001, field: "open_th", from: "0.005", to: "0.006"}]`;`groups_added = []`、`groups_removed = []`;`publish = {version: 42, result: "Applied"}`;`alerts = []`
  - `versions/sim-a/42.bin` 与该 blob 逐字节相同;`index.jsonl` 新增 `version = 42` 一行
- 实现:`python/pipeline/tests/test_round.py::test_pip_030_full_round`

### PIP-031 外部输入的超时、限速、服务端错误与解析失败都算读取失败

- 规则:[一轮的步骤](README.md#一轮的步骤)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;假 HTTP 服务器对 `https://ex.test/px`(必需输入 `I_px`)按行返回;`I_vol` 返回 200 与合法 JSON;插件输出 `out_pf()`
- 实现:`python/pipeline/tests/test_snapshot.py::test_pip_031_input_failure_kinds`

| 行 | `I_px` 的响应 | 期望 |
|---|---|---|
| PIP-031.1 | 超时(> 请求超时) | 快照中 `I_px` 标记失败;`status.ok = false`;插件仍被调用一次 |
| PIP-031.2 | HTTP 418 | 同上 |
| PIP-031.3 | HTTP 429,响应体 `[]` | 同上;快照中 `I_px` 不是空列表而是失败标记 |
| PIP-031.4 | HTTP 503 | 同上 |
| PIP-031.5 | HTTP 200,响应体不是合法 JSON | 同上 |
| PIP-031.6 | HTTP 200,`[]`(合法空列表) | `I_px` 读取成功(空列表);`status.ok = true` |
| PIP-031.7 | HTTP 200 合法数据,而 `I_vol`(非必需)返回 503 | `status.ok = true`;快照中 `I_vol` 标记失败 |

### PIP-032 必需输入失败的一轮只下发收紧类变更

- 规则:[一轮的步骤](README.md#一轮的步骤)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`(`G1`、`G2` 净量 0、无在途)、`gj0`;`I_px` 返回 HTTP 503;每行插件输出相对 `out_pf()` 的一处改动
- 实现:`python/pipeline/tests/test_round.py::test_pip_032_failed_input_tighten_only`

| 行 | 插件输出的改动 | 期望 |
|---|---|---|
| PIP-032.1 | `G_ETH.open_allowed = False` | `SetParams` 下发,`G2` 的 `open_allowed = 0` |
| PIP-032.2 | 不再输出 `G_ETH`(移除无仓位的 `G2`) | `SetParams` 下发,blob 中只剩 `G1` |
| PIP-032.3 | 新增 group `G_BTCC`(BTC,腿 `BTC/vC-spot`、`BTC/vB-perp`;新指纹) | 无 `SetParams`;新 group 不出现;`status.ok = false` |
| PIP-032.4 | `js0` 中 `G2.open_allowed = 0`,插件输出 `G_ETH.open_allowed = True` | 无 `SetParams`;`G2` 仍为 0 |
| PIP-032.5 | `G_BTC.fields = {open_th: 0.006}`(`relax = Down`,值变大 = 收紧) | `SetParams` 下发,`G1` 的 `open_th` = 6 000 000 |
| PIP-032.6 | `G_BTC.fields = {open_th: 0.004}`(变小 = 放宽) | 无 `SetParams`;`open_th` 保持 5 000 000 |
| PIP-032.7 | 全局 `max_notional` 不变,`G_BTC.fields = {max_notional: 9000}`(`relax = Up`,变小 = 收紧) | `SetParams` 下发,`G1` 的 `max_notional` = 9 000 000 000 |
| PIP-032.8 | `G_BTC.fields = {max_notional: 11000}`(变大 = 放宽) | 无 `SetParams` |
| PIP-032.9 | `G_BTC.fields = {score: 0.5}`(`relax = None`) | 无 `SetParams`;`score` 保持 0 |
| PIP-032.10 | 同时有 PIP-032.1 与 PIP-032.6 的改动 | 一条 `SetParams`:`G2.open_allowed = 0`,`G1.open_th` 仍为 5 000 000 |

### PIP-033 插件返回 ok = False 与必需输入失败同样只下发收紧

- 规则:[一轮的步骤](README.md#一轮的步骤)、[插件接口](README.md#插件接口)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;全部输入成功;插件输出 `out_pf()` 置 `ok = False`,`G_ETH.open_allowed = False`,`G_BTC.fields = {open_th: 0.004}`,`notes = ("vol feed stale",)`
- 步骤:
  1. `sim-a` 跑一轮
- 期望:
  - 一条 `SetParams`:`G2.open_allowed = 0`;`G1.open_th` = 5 000 000(放宽未下发)
  - `status.json`:`ok = false`,插件 `notes` 原样记录;告警 `Plugin`(P1),`detail` = `vol feed stale`
- 实现:`python/pipeline/tests/test_round.py::test_pip_033_plugin_not_ok`

### PIP-034 字段值按实例覆盖项、校准对覆盖项、插件、policy、schema 默认的顺序取第一个

- 规则:[一轮的步骤](README.md#一轮的步骤)、[参数通道](README.md#参数通道)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;`snap.ts = T0`;覆盖项都无到期,`pair-p1.toml` 的项都写明 `scope = "pair"`;`policy/sim-a.toml` 与 `policy/live-a.toml` 内容相同;每行给出字段 `open_th`(全局默认值)在各层的值,未列出的层不给该字段
- 实现:`python/pipeline/tests/test_merge.py::test_pip_034_merge_precedence`

| 行 | `overrides/sim-a.toml` | `overrides/pair-p1.toml` | 插件 `fields` | policy `[fields]` | 期望(`sim-a` 下发的全局 `open_th`) |
|---|---|---|---|---|---|
| PIP-034.1 | 0.007 | 0.008 | 0.009 | 0.010 | 7 000 000 |
| PIP-034.2 | — | 0.008 | 0.009 | 0.010 | 8 000 000 |
| PIP-034.3 | — | — | 0.009 | 0.010 | 9 000 000 |
| PIP-034.4 | — | — | — | 0.010 | 10 000 000 |
| PIP-034.5 | — | — | — | — | 5 000 000(schema 默认) |
| PIP-034.6 | — | 0.008(`p1` 的项) | 0.009 | — | `live-a` 同一轮下发 8 000 000;`sim-b`(不在 `p1`)下发 9 000 000 |

### PIP-035 人工覆盖项盖过插件输出时状态与 explain 标出被覆盖的插件值

- 规则:[一轮的步骤](README.md#一轮的步骤)、[使用本组件](README.md#使用本组件)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;`overrides/sim-a.toml` 一项 `{field = "open_th", group = 0x0700_0001, value = "0.007", scope = "instance", set_by = "tg:1002", set_at = T0 − 1 h}`;插件输出 `G_BTC.fields = {open_th: 0.006}`
- 步骤:
  1. `sim-a` 跑一轮
  2. `hft-pipeline explain --config <cfg_pf> --instance sim-a --asset BTC`
- 期望:
  - 步骤 1:下发的 `G1.open_th` = 7 000 000;`status.json` 的 `overridden_plugin_values` = `[{group: 0x0700_0001, field: "open_th", plugin: "0.006", applied: "0.007", source: "instance"}]`
  - 步骤 2:退出码 0;输出中 `open_th` 一步写明插件值 `0.006`、覆盖项值 `0.007` 与来源 `overrides/sim-a.toml`
- 实现:`python/pipeline/tests/test_merge.py::test_pip_035_override_reports_plugin_value`

### PIP-036 覆盖项在快照时刻到达 expires_at 时到期

- 规则:[参数通道](README.md#参数通道)、[一轮的步骤](README.md#一轮的步骤)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;`overrides/sim-a.toml` 一项 `open_th = "0.007"`(全局)带 `expires_at = E`;插件输出全局 `open_th: 0.006`
- 实现:`python/pipeline/tests/test_merge.py::test_pip_036_override_expiry_boundary`

| 行 | `snap.ts` | 期望(下发的 `open_th`) |
|---|---|---|
| PIP-036.1 | `E − 1 ms` | 7 000 000(覆盖项生效) |
| PIP-036.2 | `E` | 6 000 000(已到期,回到插件值) |
| PIP-036.3 | `E + 1 ms` | 6 000 000 |

### PIP-037 覆盖项改动结构字段时 Live 实例只下发其余字段

- 规则:[一轮的步骤](README.md#一轮的步骤)、[参数通道](README.md#参数通道)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`(`live-a` 全部 cell 期望 `Live`,`pf-live-1` 是 Live 账户,所以 `live-a` 是 Live 实例)、`gj0`;`overrides/live-a.toml` 两项:`pct_bin = "0.0002"`(结构字段,当前 100 000)、`cooldown_ms = "25000"`(人工直接编辑文件写入);插件对 `live-a` 输出 `out_pf()` 去掉 `G_ETH`(与 `live-a` 生效的 `gid(8, 1)` 同一指纹)
- 步骤:
  1. `live-a` 跑一轮
- 期望:
  - 一条 `SetParams`(`issuer = "pipeline"`,不带 `confirmed`):`cooldown_ms` = 25 000;`pct_bin` = 100 000(保持当前生效值)
  - 告警 `StructuralLive`(P1),`detail` 含 `pct_bin` 与"须经 hft-cfg + 运维 CLI"
- 实现:`python/pipeline/tests/test_merge.py::test_pip_037_structural_override_live_kept`

### PIP-038 policy 改动结构字段时 Live 实例同样保持当前值

- 规则:[一轮的步骤](README.md#一轮的步骤)
- 级别:unit
- 里程碑:M4
- 前置:同 PIP-037(插件对 `live-a` 只输出 `G_BTC`),但覆盖项为空;`policy/live-a.toml` 的 `[fields] pct_bin = "0.0003"`
- 步骤:
  1. `live-a` 跑一轮
- 期望:
  - 无 `SetParams`(其余内容与生效参数相同);告警 `StructuralLive`,`detail` 含 `pct_bin`
- 实现:`python/pipeline/tests/test_merge.py::test_pip_038_structural_policy_live_kept`

### PIP-039 结构字段的改动对非 Live 实例照常下发

- 规则:[一轮的步骤](README.md#一轮的步骤)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;每行目标实例的覆盖项文件一项 `pct_bin = "0.0002"`;插件对 `sim-a` 输出 `out_pf()`,对 `live-a` 只输出 `G_BTC`
- 实现:`python/pipeline/tests/test_merge.py::test_pip_039_structural_non_live_published`

| 行 | 目标实例与状态 | 期望 |
|---|---|---|
| PIP-039.1 | `sim-a`(Sim 账户) | `SetParams` 下发 `pct_bin` = 200 000;无 `StructuralLive` |
| PIP-039.2 | `live-a`,全部 cell 期望 `Shadow` | 同上 |
| PIP-039.3 | `live-a`,全部 cell 期望 `CloseOnly` | 同上 |
| PIP-039.4 | `live-a`,一个 cell 期望 `Live`、其余 `Shadow` | 无该字段的下发(`pct_bin` 保持 100 000);告警 `StructuralLive` |

### PIP-040 只由人工设置的字段只取覆盖项或 policy

- 规则:[一轮的步骤](README.md#一轮的步骤)、[插件接口](README.md#插件接口)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;`overrides/sim-a.toml` 一项 `{field = "force_exit", group = 0x0700_0002, value = "true", scope = "instance"}`
- 步骤:
  1. `sim-a` 跑一轮
- 期望:
  - `SetParams` 下发:`G2` 的覆盖表含 `force_exit` = 1;`G1` 无 `force_exit` 覆盖(取默认 0)
- 实现:`python/pipeline/tests/test_merge.py::test_pip_040_manual_only_from_override`

### PIP-045 有仓位或在途的 group 不被移除而改为 open_allowed = false

- 规则:[一轮的步骤](README.md#一轮的步骤)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;插件输出 `out_pf()` 去掉 `G_ETH`(意图移除 `G2`);每行设定 `G2` 的状态
- 实现:`python/pipeline/tests/test_precheck.py::test_pip_045_remove_blocked_by_position`

| 行 | `G2` 状态 | 期望 |
|---|---|---|
| PIP-045.1 | 腿 0 净量 +0.01 ETH、腿 1 净量 −0.01 ETH | `SetParams` 中 `G2` 仍在,`open_allowed = 0`;`groups_removed = []`;无 `WriteOffNeeded` |
| PIP-045.2 | 两腿净量 0,腿 0 即时在途 0.01 | 同上 |
| PIP-045.3 | 腿 0 净量 +0.01、腿 1 净量 0 | 同 PIP-045.1 |
| PIP-045.4 | 两腿净量 0、无在途 | `SetParams` 中没有 `G2`;`groups_removed = [0x0700_0002]` |

### PIP-046 残余低于最小下单量且无在途时提示由 Admin 核销

- 规则:[一轮的步骤](README.md#一轮的步骤)
- 级别:table
- 里程碑:M4
- 前置:同 PIP-045(意图移除 `G2`;`ETH/vA-spot` 的 `min_qty` = 0.0001、`ETH/vB-perp` 的 `min_qty` = 0.01);每行设定 `G2` 两腿净量与在途
- 实现:`python/pipeline/tests/test_precheck.py::test_pip_046_writeoff_hint_boundary`

| 行 | 腿 0 净量 | 腿 1 净量 | 在途 | 期望 |
|---|---|---|---|---|
| PIP-046.1 | +0.000099 | 0 | 无 | `G2` 保留且 `open_allowed = 0`;告警 `WriteOffNeeded`(P2),`detail` 含 `0x0700_0002`、`ETH/vA-spot`、`0.000099` |
| PIP-046.2 | +0.0001 | 0 | 无 | `G2` 保留且 `open_allowed = 0`;无 `WriteOffNeeded`(这一腿可下单平掉) |
| PIP-046.3 | +0.0001 | −0.009 | 无 | 无 `WriteOffNeeded`(腿 0 不低于 `min_qty`) |
| PIP-046.4 | +0.00005 | −0.009 | 无 | 告警 `WriteOffNeeded`,`detail` 列出两腿 |
| PIP-046.5 | +0.00005 | −0.01 | 无 | 无 `WriteOffNeeded`(腿 1 = `min_qty`) |
| PIP-046.6 | +0.00005 | 0 | 腿 1 即时在途 0.01 | 无 `WriteOffNeeded`(有在途) |

### PIP-047 平仓后的下一轮移除该 group

- 规则:[一轮的步骤](README.md#一轮的步骤)
- 级别:unit
- 里程碑:M4
- 前置:PIP-045.1 之后(`G2` 以 `open_allowed = 0` 生效,`version = 42`);随后 journal 显示 `G2` 两腿净量 0、无在途;插件仍不输出 `G_ETH`
- 步骤:
  1. `sim-a` 再跑一轮
- 期望:
  - 一条 `SetParams`(`version = 43`,`base_version = 42`),blob 中没有 `0x0700_0002`;`groups_removed = [0x0700_0002]`
  - `groups.json` 中 id 2 仍保留其指纹,`next_ctr` 仍为 3
- 实现:`python/pipeline/tests/test_precheck.py::test_pip_047_remove_after_flat`

### PIP-048 移除 group 时其腿的 inst 仍有账户级持仓则照常移除并提示

- 规则:[一轮的步骤](README.md#一轮的步骤)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;插件输出不含 `G_ETH`;`G2` 两腿净量 0、无在途;每行设定账户 `sim-pf-1` 的账户级(不属于任何 group)持仓
- 实现:`python/pipeline/tests/test_precheck.py::test_pip_048_remove_with_account_level_position`

| 行 | 账户级持仓 | 期望 |
|---|---|---|
| PIP-048.1 | `ETH/vB-perp` −0.004(由核销移入) | `SetParams` 中没有 `G2`;告警 `RemovedWithPosition`(P2),`detail` 含 `ETH/vB-perp` 与 `-0.004`;`/pipeline` 的状态同样列出 |
| PIP-048.2 | `ETH/vA-spot` 库存 +0.3(unassigned) | 同上,`detail` 含 `ETH/vA-spot` 与 `0.3` |
| PIP-048.3 | `BTC/vB-perp` +0.002(不属于被移除 group 的腿) | `G2` 移除;无 `RemovedWithPosition` |
| PIP-048.4 | 无 | `G2` 移除;无 `RemovedWithPosition` |

### PIP-049 腿不在当前交易宇宙内的 group 进入候选且不在这一轮下发

- 规则:[一轮的步骤](README.md#一轮的步骤)、[交易宇宙候选](README.md#交易宇宙候选)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;插件输出 `out_pf()` 再加 `G_XRP`(XRP,腿 `XRP/vA-spot`、`XRP/vB-perp`;两者不在 `rd_pf` 的交易宇宙)与 `universe_candidates = ((vA-spot, Spot, XRP/vA-spot), (vB-perp, Perp, XRP/vB-perp))`
- 步骤:
  1. `sim-a` 跑一轮
- 期望:
  - 无 `SetParams`(其余内容与生效参数相同);`groups.json` 不为 XRP 分配 id(`next_ctr` 仍为 3)
  - `status.json` 记录该 group 未下发的原因"腿不在交易宇宙内"与两个 inst;当日宇宙候选汇总含这两个 inst
- 实现:`python/pipeline/tests/test_precheck.py::test_pip_049_legs_outside_universe`

### PIP-050 两腿在同一交易所的 group 被拦下,其余 group 照常下发

- 规则:[一轮的步骤](README.md#一轮的步骤)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;每行插件在 `out_pf()` 之外再输出一个新 group
- 实现:`python/pipeline/tests/test_precheck.py::test_pip_050_same_exchange_legs`

| 行 | 新 group 的腿 | 期望 |
|---|---|---|
| PIP-050.1 | `BTC/vA-spot`、`BTC/vA-perp`(同在 `EA`) | 无 `SetParams`;告警 `SameExchangeLegs`(P1),`detail` 含两条腿与 `EA`;`next_ctr` 仍为 3 |
| PIP-050.2 | `BTC/vC-spot`、`BTC/vB-perp` | 一条 `SetParams`,新增 `gid(7, 3)`;无告警 |
| PIP-050.3 | `BTC/vC-spot`、`BTC/vB-perp`、`BTC/vA-spot`(三个交易所) | 一条 `SetParams`,新增 `gid(7, 3)`,3 条腿 |
| PIP-050.4 | PIP-050.1 与 PIP-050.2 的两个 group 同时输出 | 一条 `SetParams`,只新增 PIP-050.2 的 group(`gid(7, 3)`);告警 `SameExchangeLegs` 一条 |

### PIP-051 sequencer 以 LotMismatch 拒绝时保留旧版本并告警

- 规则:[一轮的步骤](README.md#一轮的步骤)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;插件输出 `out_pf()` 再加 `G_BTCC`(`BTC/vC-spot`、`BTC/vB-perp`);`FakeCtl` 对这条 `SetParams` 回 `{status = "ok", code = "InvalidArgs", detail = {name = "LotMismatch"}}`
- 步骤:
  1. `sim-a` 跑一轮
- 期望:
  - `status.json`:`publish.result = "InvalidArgs"`,`publish.detail = "LotMismatch"`;告警 `PublishRejected`(P1),`detail` 含 `LotMismatch`
  - 下一轮快照中生效版本仍为 41;这一轮没有 `InjectWarmup`(新 group 未生效)
- 实现:`python/pipeline/tests/test_publish.py::test_pip_051_lot_mismatch_keeps_old`

### PIP-060 插件与覆盖项的十进制值按字段单位换算为原始值

- 规则:[schema 校验与编码](README.md#schema-校验与编码)
- 级别:table
- 里程碑:M4
- 前置:`S_pf`、`rd_pf`;被测函数为框架 `schema.py` 的换算入口(字段、十进制值、该 group 资产的指数 → 原始 i64 或 `InvalidValue{reason}`);`G1` 的资产 BTC(`qty_exp = −8`),`G2` 的资产 ETH(`qty_exp = −6`)
- 实现:`python/pipeline/tests/test_schema.py::test_pip_060_unit_conversion`

| 行 | 字段(group) | 输入 | 期望 |
|---|---|---|---|
| PIP-060.1 | `open_th` | `Decimal("0.005")` | 5 000 000 |
| PIP-060.2 | `open_th` | `Decimal("0.000000001")` | 1 |
| PIP-060.3 | `open_th` | `Decimal("0.0000000005")` | `InvalidValue{reason = BadText}`(精度超过 1e-9) |
| PIP-060.4 | `max_notional` | `Decimal("10000")` | 10 000 000 000 |
| PIP-060.5 | `max_notional` | `Decimal("0.000001")` | 1 |
| PIP-060.6 | `max_notional` | `Decimal("0.0000001")` | `InvalidValue{reason = BadText}` |
| PIP-060.7 | `entry_qty`(`G1`) | `Decimal("0.00000001")` | 1 |
| PIP-060.8 | `entry_qty`(`G1`) | `Decimal("0.000000001")` | `InvalidValue{reason = BadText}` |
| PIP-060.9 | `entry_qty`(`G2`) | `Decimal("0.000001")` | 1 |
| PIP-060.10 | `entry_qty`(无 group) | `Decimal("1")` | `InvalidValue{reason = NotPerGroup}`(`Qty` 只能按 group) |
| PIP-060.11 | `cooldown_ms` | `Decimal("1500")` | 1 500 |
| PIP-060.12 | `cooldown_ms` | `Decimal("1500.5")` | `InvalidValue{reason = BadText}` |
| PIP-060.13 | `max_groups` | `Decimal("8")` | 8 |
| PIP-060.14 | `force_exit`(`G1`) | `True` / `False` | 1 / 0 |
| PIP-060.15 | `mode` | `"b"` | 1 |
| PIP-060.16 | `mode` | `"c"` | `InvalidValue{reason = BadEnum}` |
| PIP-060.17 | `no_such` | `Decimal("1")` | `InvalidValue{reason = UnknownField}` |

### PIP-061 float 值不进入换算

- 规则:[schema 校验与编码](README.md#schema-校验与编码)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;每行插件输出的一个值类型为 Python `float`
- 实现:`python/pipeline/tests/test_schema.py::test_pip_061_float_rejected`

| 行 | 插件输出 | 期望 |
|---|---|---|
| PIP-061.1 | `G_BTC.fields = {open_th: 0.006}`(`float`) | 无 `SetParams`;告警 `RoundRejected`,`detail` 含 `open_th` 与 `float` |
| PIP-061.2 | 全局 `cooldown_ms: 20000.0`(`float`) | 同上,`detail` 含 `cooldown_ms` |
| PIP-061.3 | `G_BTC.fields = {open_th: Decimal("0.006")}` | `SetParams` 下发,`open_th` = 6 000 000 |

### PIP-062 字段值在 min 与 max 的边界内才下发

- 规则:[schema 校验与编码](README.md#schema-校验与编码)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;每行给出插件输出(`open_th`、`cooldown_ms`)或 policy `[fields]`(`max_groups`)中的一个值
- 实现:`python/pipeline/tests/test_schema.py::test_pip_062_min_max_bounds`

| 行 | 字段与值 | 期望 |
|---|---|---|
| PIP-062.1 | `G_BTC.open_th = 0.1`(= max,100 000 000) | `SetParams` 下发该值 |
| PIP-062.2 | `G_BTC.open_th = 0.100000001`(max + 1 raw) | 无 `SetParams`;告警 `RoundRejected`,`detail` 含 `open_th` 与 `OutOfRange` |
| PIP-062.3 | `G_BTC.open_th = 0`(= min) | `SetParams` 下发 0 |
| PIP-062.4 | `G_BTC.open_th = -0.000000001`(min − 1 raw) | 无 `SetParams`;告警 `RoundRejected` |
| PIP-062.5 | `cooldown_ms = 600000`(= max) | `SetParams` 下发 |
| PIP-062.6 | `cooldown_ms = 600001` | 无 `SetParams`;告警 `RoundRejected` |
| PIP-062.7 | `max_groups = 64`(policy) | `SetParams` 下发 64 |
| PIP-062.8 | `max_groups = 65`(policy) | 无 `SetParams`;告警 `RoundRejected` |

### PIP-063 非 per_group 字段不得按 group 覆盖

- 规则:[schema 校验与编码](README.md#schema-校验与编码)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`
- 实现:`python/pipeline/tests/test_schema.py::test_pip_063_not_per_group`

| 行 | 来源 | 期望 |
|---|---|---|
| PIP-063.1 | 插件 `G_BTC.fields = {cooldown_ms: Decimal("1000")}` | 无 `SetParams`;告警 `RoundRejected`,`detail` 含 `cooldown_ms` 与 `NotPerGroup` |
| PIP-063.2 | `overrides/sim-a.toml` 一项 `{field = "cooldown_ms", group = 0x0700_0001, value = "1000", scope = "instance"}`(手工写入) | 同上 |
| PIP-063.3 | 同一项不带 `group` | `SetParams` 下发全局 `cooldown_ms` = 1 000 |

### PIP-064 编码后的 blob 经 decode 回读与组装值逐字段一致才下发

- 规则:[schema 校验与编码](README.md#schema-校验与编码)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;插件输出 `G_BTC.fields = {open_th: Decimal("0.006")}`;每行对 `hft_py.params` 打桩
- 实现:`python/pipeline/tests/test_schema.py::test_pip_064_decode_readback`

| 行 | 打桩 | 期望 |
|---|---|---|
| PIP-064.1 | 不打桩 | `encode` 被调用一次、`decode` 被调用一次;回读 `G1.open_th` = 6 000 000;`SetParams` 下发 |
| PIP-064.2 | `decode` 返回的 `G1.open_th` 改为 6 000 001 | 无 `SetParams`;告警 `RoundRejected`,`detail` 含 `open_th` 与两个值 |
| PIP-064.3 | `encode` 返回 `(None, [<错误 OutOfRange field_ix = 0>])` | 无 `SetParams`;告警 `RoundRejected`,`detail` 含该错误 |
| PIP-064.4 | `decode` 抛 `hft_py.ParamsError` | 无 `SetParams`;告警 `RoundRejected`;进程不退出,下一实例照常跑 |

### PIP-065 参数编码金样:框架组装后的 blob 与金样逐字节相同

- 规则:[schema 校验与编码](README.md#schema-校验与编码)
- 级别:golden
- 里程碑:M4
- 输入:`fixtures/params/*/<case>.input.toml` 中期望结果为 `Ok` 的每个 case(经框架的组装入口:把 TOML 中的 group、默认值与覆盖项作为"插件 + 覆盖项"的合并结果,`generated_at_ns` 与版本取 `.meta.toml`)
- 期望文件:`fixtures/params/*/<case>.blob`;另断言 `hft_py.params.decode` 回读的每个字段与 `.input.toml` 相同
- 实现:`python/pipeline/tests/test_golden.py::test_pip_065_params_golden`

### PIP-066 运行时 schema 与归档不同时以 GetSchema 为准并告警

- 规则:[schema 校验与编码](README.md#schema-校验与编码)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;`D_schemas` 中 `S_pf` 为 `schema_version = 2`;每行设定 journal 中 `sim-a` 生效参数头的 `(schema.id, schema.version)` 与 `FakeCtl` 对 `GetSchema{kind_id = 65296}` 的应答
- 实现:`python/pipeline/tests/test_schema.py::test_pip_066_runtime_schema`

| 行 | 生效头 / `GetSchema` 应答 | 期望 |
|---|---|---|
| PIP-066.1 | `(65296, 2)` / 不应被调用 | 无 `GetSchema` 请求;无 `SchemaRuntime` |
| PIP-066.2 | `(65296, 3)` / `S_pf` 加字段 `since = 3` 的 v3 | 一次 `GetSchema`;告警 `SchemaRuntime`(P2);下发的 blob 头 `schema_version = 3`,`n_fields` = 10 |

### PIP-067 StrategyParams 头的来源、生成时刻与账户

- 规则:[schema 校验与编码](README.md#schema-校验与编码)、[输出与版本库](README.md#输出与版本库)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`(`snap.ts = T0`)、`gj0`;插件输出 `G_BTC.fields = {open_th: Decimal("0.006")}`
- 步骤:
  1. `sim-a` 跑一轮,取 `SetParams` 的 blob 头
- 期望:
  - `source = Pipeline`;`generated_at_ns = 1 790 812 800 000 000 000`;`account` = `sim-pf-1` 的 `AccountId`;`strategy = 7`;`kind_id = 65296`;`schema_version = 2`;`version = 42`
- 实现:`python/pipeline/tests/test_schema.py::test_pip_067_header_fields`

### PIP-068 本包没有参数编解码实现,下发的 blob 只来自 hft_py.params.encode

- 规则:[schema 校验与编码](README.md#schema-校验与编码)、[依赖](README.md#依赖)
- 级别:unit
- 里程碑:M4
- 前置:`python/pipeline/src/hft_pipeline/` 全部源码;`cfg_pf`、`js0`;`hft_py.params.encode` 打桩为返回固定 32 字节 `b"\xAB" * 32` 与空错误表,`decode` 打桩为返回组装时的值
- 步骤:
  1. AST 扫描源码中的 `import struct`、`struct.pack`、`int.to_bytes`、`int.from_bytes`、`memoryview` 与 `bytearray` 构造
  2. `sim-a` 跑一轮(插件输出 `G_BTC.fields = {open_th: Decimal("0.006")}`)
- 期望:
  - 步骤 1:零处命中
  - 步骤 2:`SetParams` 的 `blob_b64` 解码后恰为 32 个 `0xAB`
- 实现:`python/pipeline/tests/test_static.py::test_pip_068_no_python_codec`

### PIP-069 两个测试策略的 schema 不写插件也能被通用处理

- 规则:[schema 校验与编码](README.md#schema-校验与编码)、[插件接口](README.md#插件接口)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf` 另加实例 `sim-tp`(`core = "sim"`,`S_tp` 的 kind_id,`strategy_id = 11`,账户 `sim-tp-1`(Sim,绑定 `sim-tp`),无插件,policy 静态 group 表一项 BTC 两腿);`sim-np` 用 `S_tk`;对每个 schema 从载入的字段中选:`Fg` = 一个 `per_group = true` 的数值字段,`Fn` = 一个 `per_group = false` 的数值字段
- 实现:`python/pipeline/tests/test_generic.py::test_pip_069_generic_schemas`

| 行 | 实例与覆盖项 | 期望 |
|---|---|---|
| PIP-069.1 | `sim-np`:`Fn` = 其 `max` | `SetParams` 下发,`Fn` = `max` |
| PIP-069.2 | `sim-np`:`Fn` = `max + 1 raw` | 无 `SetParams`;告警 `RoundRejected`(`OutOfRange`) |
| PIP-069.3 | `sim-np`:`Fn` 带 `group` | 无 `SetParams`;告警 `RoundRejected`(`NotPerGroup`) |
| PIP-069.4 | `sim-np`:`Fg` 带 `group = gid(10, 1)`,值 = 其 `min` | `SetParams` 覆盖表含该项 |
| PIP-069.5 | `sim-tp`:同 PIP-069.1 | 同 PIP-069.1 |
| PIP-069.6 | `sim-tp`:同 PIP-069.2 | 同 PIP-069.2 |
| PIP-069.7 | `sim-tp`:同 PIP-069.3 | 同 PIP-069.3 |

### PIP-070 新增第二个测试策略时框架代码零改动

- 规则:[插件的打包与发现](README.md#插件的打包与发现)、[概要](README.md#概要)
- 级别:proc
- 里程碑:M4
- 前置:CI 工作副本;`python/pipeline/src/` 的文件哈希清单 H0;`D_schemas` 只含 `S_tk`、`S_pf`
- 步骤:
  1. 把 `S_tp` 的 schema 文件与 `index.json` 项加入 `D_schemas`,在配置中加实例 `sim-tp`(同 PIP-069),不改任何 `.py`
  2. 启动 `hft-pipeline run`,等 `sim-tp` 跑完一轮
  3. 计算 `python/pipeline/src/` 的文件哈希清单 H1
- 期望:
  - 步骤 2:`FakeCtl` 收到 `sim-tp` 的一条 `SetParams`,`CmdResult` 为 `Applied`
  - H1 = H0;`git diff --stat python/pipeline` 为空
- 实现:`python/pipeline/tests/test_generic.py::test_pip_070_second_strategy_zero_diff`

### PIP-080 新 GroupId 的高 8 位等于实例的 strategy_id,同机两个实例各自从计数器 1 起分配

- 规则:[GroupId 分配](README.md#groupid-分配)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`;`sim-a`(`strategy_id = 7`)与 `sim-b`(`strategy_id = 9`)同在 `sim`;两者都无生效参数、无 `groups.json`、版本库为空;插件对两者都输出 `out_pf()`(BTC、ETH 两个 group)
- 步骤:
  1. `sim-a` 跑一轮,`sim-b` 跑一轮
- 期望:
  - `sim-a` 的 blob:`0x0700_0001`(BTC)、`0x0700_0002`(ETH);`sim-b` 的 blob:`0x0900_0001`、`0x0900_0002`
  - 两个实例的 id 集合交集为空;`versions/sim-a/groups.json` 与 `versions/sim-b/groups.json` 的 `next_ctr` 都为 3
- 实现:`python/pipeline/tests/test_groupid.py::test_pip_080_high_bits_and_per_instance_counter`

### PIP-081 同一指纹连续存在时沿用原 id

- 规则:[GroupId 分配](README.md#groupid-分配)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;插件输出 `out_pf()`,但 `G_ETH` 排在 `G_BTC` 之前、两者 `reason` 文本改变、`G_BTC.fields = {open_th: Decimal("0.006")}`
- 步骤:
  1. `sim-a` 跑一轮
- 期望:
  - 下发的 blob 中 BTC 组的 id 仍为 `0x0700_0001`、ETH 组仍为 `0x0700_0002`;`next_ctr` 仍为 3;`groups_added = []`
- 实现:`python/pipeline/tests/test_groupid.py::test_pip_081_same_fingerprint_keeps_id`

### PIP-082 group 被移除后再次加入同一指纹得到新 id

- 规则:[GroupId 分配](README.md#groupid-分配)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;`G2` 两腿净量 0、无在途
- 步骤:
  1. 插件不输出 `G_ETH`,跑一轮(`G2` 被移除,`version = 42` 生效)
  2. 插件再输出 `G_ETH`(同一指纹),跑一轮
- 期望:
  - 步骤 1:blob 中只有 `0x0700_0001`
  - 步骤 2:blob 中 ETH 组的 id = `0x0700_0003`;`0x0700_0002` 不出现;`groups.json` 中 id 2 与 id 3 记同一指纹,`next_ctr` = 4
- 实现:`python/pipeline/tests/test_groupid.py::test_pip_082_readd_gets_new_id`

### PIP-083 指纹变化的 group 视同移除旧 group、新增新 group

- 规则:[GroupId 分配](README.md#groupid-分配)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;`G1` 两腿净量 0、无在途;插件把 `G_BTC` 的腿 0 从 `BTC/vA-spot` 换成 `BTC/vC-spot`(同资产,不同腿)
- 步骤:
  1. `sim-a` 跑一轮
- 期望:
  - blob 中没有 `0x0700_0001`,有 `0x0700_0003`(腿 `BTC/vC-spot`、`BTC/vB-perp`);`groups_removed = [0x0700_0001]`、`groups_added = [0x0700_0003]`
- 实现:`python/pipeline/tests/test_groupid.py::test_pip_083_fingerprint_change`

### PIP-084 计数器到上限后拒绝新增 group

- 规则:[GroupId 分配](README.md#groupid-分配)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;每行设定 `groups.json` 的 `next_ctr`(其余 id 照 `gj0`);插件输出 `out_pf()` 再加一个新指纹 `G_BTCC`(`BTC/vC-spot`、`BTC/vB-perp`)
- 实现:`python/pipeline/tests/test_groupid.py::test_pip_084_counter_exhaustion`

| 行 | `next_ctr` | 期望 |
|---|---|---|
| PIP-084.1 | `0xFF_FFFD` | 新 group id = `0x07FF_FFFD`;`next_ctr` = `0xFF_FFFE` |
| PIP-084.2 | `0xFF_FFFE` | 新 group id = `0x07FF_FFFE`;`next_ctr` = `0xFF_FFFF` |
| PIP-084.3 | `0xFF_FFFF`(已用尽) | 无新 group;告警 `GroupIdExhausted`(P1);同一轮其余改动照常下发(`G_BTC.open_th = 0.006` 时 `SetParams` 只带它) |

### PIP-085 生效 blob 中由 hft-cfg 新增的 id 被登记,计数器越过它

- 规则:[GroupId 分配](README.md#groupid-分配)、[参数通道](README.md#参数通道)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`gj0`(`next_ctr = 3`);journal 中 `sim-a` 当前生效的 `StrategyParams`(由 `cli:alice` 发出,`version = 50`)比 `js0` 多一个 group,id 按行;随后插件输出一个新指纹 `G_BTCC`
- 实现:`python/pipeline/tests/test_groupid.py::test_pip_085_register_external_id`

| 行 | 外来 group 的 id | 期望 |
|---|---|---|
| PIP-085.1 | `gid(7, 101)`(指纹 `fp_x` = BTC,腿 `BTC/vA-perp`、`BTC/vC-spot`) | `groups.json` 登记 id 101 → `fp_x`,来源 `external`;`next_ctr` = 102;`G_BTCC` 得到 `gid(7, 102)` |
| PIP-085.2 | `gid(7, 3)`(= `next_ctr`) | 登记 id 3;`next_ctr` = 4;`G_BTCC` 得到 `gid(7, 4)` |
| PIP-085.3 | `gid(7, 2)`(已在 `groups.json`,同一指纹) | 不重复登记;`next_ctr` = 3;`G_BTCC` 得到 `gid(7, 3)` |
| PIP-085.4 | PIP-085.1 之后,插件输出的某个新指纹与 `fp_x` 不同 | 该新指纹得到的 id ≠ 101 |

### PIP-086 next_ctr 至少比生效 blob 的水位与最大计数器大 1

- 规则:[GroupId 分配](README.md#groupid-分配)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;每行设定 `groups.json` 的 `next_ctr`、生效 blob 中本实例的最大计数器(其 group 已登记在 `groups.json`)与生效 blob 的 `group_ctr_hwm`(未注明时等于最大计数器);插件输出一个新指纹
- 实现:`python/pipeline/tests/test_groupid.py::test_pip_086_continue_after_effective_max`

| 行 | `next_ctr` / 生效 blob 最大 `n` | 期望(新 group 的 `n`) |
|---|---|---|
| PIP-086.1 | 3 / 2 | 3 |
| PIP-086.2 | 3 / 7 | 8 |
| PIP-086.3 | 9 / 7(id 8 曾分配后被移除) | 9 |
| PIP-086.4 | 3 / 2,`group_ctr_hwm = 9` | 10 |
| PIP-086.5 | 10 / 2,`group_ctr_hwm = 9` | 10 |

### PIP-087 groups.json 丢失时从版本库重建,计数器不回退

- 规则:[GroupId 分配](README.md#groupid-分配)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;`sim-a` 的版本库历史:版本 40 含 id 1、2、5,版本 41 含 id 1、2(id 5 已移除);两版的 `group_ctr_hwm` 都为 5,另注明者除外;journal 当前生效 = 版本 41;删除 `versions/sim-a/groups.json`;插件输出一个新指纹
- 实现:`python/pipeline/tests/test_groupid.py::test_pip_087_rebuild_from_versions`

| 行 | 可用的版本库 | 期望 |
|---|---|---|
| PIP-087.1 | 本地 `versions/sim-a/` 完整 | 重建后 `next_ctr` = 6;新指纹得到 `gid(7, 6)`;新写出的 `groups.json` 含 id 1、2、5、6 |
| PIP-087.2 | 本地目录不存在,S3 `…/sim/private/params/` 有版本 40、41 | 同上 |
| PIP-087.3 | 本地与 S3 都只有版本 41;journal 生效版本 41 | `next_ctr` = 6(取生效 blob 的 `group_ctr_hwm`);新指纹得到 `gid(7, 6)` |
| PIP-087.4 | 本地与 S3 版本库都不存在;journal 生效版本 41 | `next_ctr` = 6;新指纹得到 `gid(7, 6)`;无 `GroupIdStateLost` |

### PIP-088 读不到当前生效参数且 groups.json 丢失时只保留现有 group 并拒绝新增

- 规则:[GroupId 分配](README.md#groupid-分配)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;`groups.json`、本地版本库与 S3 版本库都不存在;journal 读者读取当前生效 `StrategyParams` 所在段时报损坏(`ReadErr`),上一轮已读到的生效 group 为 `0x0700_0001`、`0x0700_0002`;插件输出 `out_pf()` 再加新指纹 `G_BTCC`,`G_BTC.fields = {open_th: Decimal("0.006")}`
- 步骤:
  1. `sim-a` 跑一轮
  2. 同一状态再跑一轮
- 期望:
  - 步骤 1:`SetParams` 中只有 `0x0700_0001`、`0x0700_0002`(`G1.open_th` = 6 000 000);无新 group;告警 `GroupIdStateLost`(P1)
  - 步骤 2:仍不新增 group,`GroupIdStateLost` 再记一次
- 实现:`python/pipeline/tests/test_groupid.py::test_pip_088_state_lost_rejects_new`

### PIP-089 写 groups.json 中途崩溃时旧文件完整

- 规则:[GroupId 分配](README.md#groupid-分配)、[输出与版本库](README.md#输出与版本库)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;插件输出新指纹 `G_BTCC`;在 `groups.json` 写入流程的给定点注入进程终止(`os._exit`),再重启跑一轮
- 实现:`python/pipeline/tests/test_groupid.py::test_pip_089_crash_during_groups_write`

| 行 | 终止点 | 期望(重启后) |
|---|---|---|
| PIP-089.1 | `.tmp` 写了一半、`os.replace` 之前 | `groups.json` 内容 = `gj0`;残留的 `.tmp` 被忽略并覆盖;新指纹得到 `gid(7, 3)` |
| PIP-089.2 | `os.replace` 之后、`SetParams` 之前 | `groups.json` 已含 id 3;重启后的一轮仍为该指纹分配 `gid(7, 3)`(同一指纹沿用),不分配 4 |
| PIP-089.3 | `SetParams` `Applied` 之后、`index.jsonl` 写入之前(`42.bin` 已写) | 重启后生效版本 42 被读到且与 `42.bin` 同 SHA-256;下一次下发 `version = 43`、`base_version = "42"`(见 PIP-114、PIP-115) |

### PIP-090 groups.json 随版本库原子写并上传

- 规则:[GroupId 分配](README.md#groupid-分配)、[输出与版本库](README.md#输出与版本库)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`(假 S3)、`js0`、`gj0`;插件输出新指纹 `G_BTCC`
- 步骤:
  1. `sim-a` 跑一轮
- 期望:
  - 本地 `versions/sim-a/groups.json`:`next_ctr = 4`,id 3 的指纹 = `G_BTCC` 的 `GroupSpec::fingerprint()`,首个与最后一个参数版本均为 42
  - 假 S3 前缀 `s3://test-bucket/hft/h1/sim/private/params/` 下出现与本地 `groups.json` 逐字节相同的对象
- 实现:`python/pipeline/tests/test_groupid.py::test_pip_090_groups_json_uploaded`

### PIP-100 pipeline 主体的 SetParams 在 Live 账户上也不带确认

- 规则:[参数通道](README.md#参数通道)、[概要](README.md#概要)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`(`live-a` 是 Live 实例,生效 `version = 12`,group `gid(8, 1)` `open_allowed = 1`);`FakeCtl`;每行插件对 `live-a` 的输出
- 实现:`python/pipeline/tests/test_publish.py::test_pip_100_pipeline_setparams_neutral`

| 行 | 插件输出 | 期望 |
|---|---|---|
| PIP-100.1 | `G_BTC` 与新 group `G_ETH`(`open_allowed = True`) | 一条 `SetParams`:`issuer = "pipeline"`、`preview = false`、`confirmed = false`、无 `expect_state_version`;新增 `gid(8, 2)` |
| PIP-100.2 | js0 中 `gid(8, 1)` 的 `open_allowed = 0`;插件输出 `G_BTC.open_allowed = True` | 同上形状的一条 `SetParams`,`gid(8, 1)` 的 `open_allowed = 1` |
| PIP-100.3 | `G_BTC.fields = {max_notional: Decimal("20000")}`(放宽) | 同上形状的一条 `SetParams` |
| PIP-100.4 | PIP-100.1 且 `FakeCtl` 回 `NeedsConfirm` | `publish.result = "NeedsConfirm"`;告警 `PublishRejected`;pipeline 不发确认请求,这一轮不再发任何 `SetParams` |

### PIP-101 SetParams 的过期时刻为签发后 30 s

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;插件输出 `G_BTC.fields = {open_th: Decimal("0.006")}`;墙钟打桩为 `1 790 812 860 000 000 000`
- 步骤:
  1. `sim-a` 跑一轮
- 期望:
  - `SetParams` 的 `issued_at_ns = 1 790 812 860 000 000 000`、`expires_at_ns = 1 790 812 890 000 000 000`(差恰为 30 000 000 000)
- 实现:`python/pipeline/tests/test_publish.py::test_pip_101_expires_30s`

### PIP-102 对模拟实例 apply_override 只改它自己的覆盖项文件

- 规则:[参数通道](README.md#参数通道)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;`overrides/` 为空;`p1 = {live-a, sim-a}`
- 步骤:
  1. `preview_override(target = "instance:sim-a", field = "open_th", group = 0x0700_0001, value = "0.007")`,取返回的 `override_version`(0)
  2. `apply_override(target = "instance:sim-a", field = "open_th", group = 0x0700_0001, value = "0.007", expect_override_version = 0)`
  3. 等 `sim-a` 与 `live-a` 各跑完一轮
- 期望:
  - 步骤 2:返回 `ok = true`;只有 `overrides/sim-a.toml` 被创建,含一项 `{field = "open_th", group = 0x0700_0001, value = "0.007", scope = "instance"}`,文件 `override_version` = 1;`overrides/live-a.toml` 与 `overrides/pair-p1.toml` 不存在
  - 步骤 3:`sim-a` 的 `SetParams` 中 `G1.open_th` = 7 000 000;`live-a` 无 `SetParams`(内容未变)
- 实现:`python/pipeline/tests/test_overrides.py::test_pip_102_instance_override_scoped`

### PIP-103 校准对覆盖项文件的每项必须显式 scope = "pair"

- 规则:[参数通道](README.md#参数通道)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;`overrides/pair-p1.toml` 两项:`cooldown_ms = "25000"`(`scope` 按行)与 `max_groups = "10"`(`scope = "pair"`);插件不输出 `cooldown_ms`
- 实现:`python/pipeline/tests/test_overrides.py::test_pip_103_pair_scope_explicit`

| 行 | 第一项的 `scope` | 期望 |
|---|---|---|
| PIP-103.1 | `"pair"` | `sim-a` 与 `live-a` 下发 `cooldown_ms` = 25 000、`max_groups` = 10 |
| PIP-103.2 | 缺省 | 整个文件加载失败:两项都不生效(`sim-a` 的 `cooldown_ms` = 30 000 来自 policy,`max_groups` = 8);告警 `ConfigInvalid`(P1),`detail` 含 `pair-p1.toml` |
| PIP-103.3 | `"instance"` | 同 PIP-103.2 |
| PIP-103.4 | `"Pair"` | 同 PIP-103.2 |

### PIP-104 实例覆盖项与校准对覆盖项同时存在时实例项生效,不同字段各自生效

- 规则:[参数通道](README.md#参数通道)、[一轮的步骤](README.md#一轮的步骤)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;`overrides/sim-a.toml`:`cooldown_ms = "21000"`;`overrides/pair-p1.toml`:`cooldown_ms = "22000"`、`max_groups = "12"`(都 `scope = "pair"`);插件不输出 `cooldown_ms`
- 步骤:
  1. `sim-a` 与 `live-a` 各跑一轮
- 期望:
  - `sim-a`:`cooldown_ms` = 21 000、`max_groups` = 12
  - `live-a`:`cooldown_ms` = 22 000、`max_groups` = 12
- 实现:`python/pipeline/tests/test_overrides.py::test_pip_104_instance_beats_pair`

### PIP-105 apply_override 带过期的 expect_override_version 被拒

- 规则:[参数通道](README.md#参数通道)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;`overrides/sim-a.toml` 当前 `override_version = 5`;每行调用 `apply_override(target = "instance:sim-a", field = "cooldown_ms", value = "21000", expect_override_version = v)`
- 实现:`python/pipeline/tests/test_overrides.py::test_pip_105_expect_version`

| 行 | `v` | 期望 |
|---|---|---|
| PIP-105.1 | 5 | `ok = true`;文件 `override_version` = 6 |
| PIP-105.2 | 4 | `ok = false`,`error = VersionChanged`;文件内容与版本不变 |
| PIP-105.3 | 6 | `ok = false`,`error = VersionChanged` |
| PIP-105.4 | 对 `pair:p1`(`pair-p1.toml` 版本 2)传 2 | `ok = true`;`pair-p1.toml` 版本 3,`sim-a.toml` 版本仍 5 |

### PIP-106 两个并发 apply_override 带同一期望版本时只有一个成功

- 规则:[参数通道](README.md#参数通道)、[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M4
- 前置:`cfg_pf`;本地接口 socket 已启动;`overrides/sim-a.toml` 版本 5
- 步骤:
  1. 两个客户端同时(同一事件循环 tick 内)发 `apply_override(target = "instance:sim-a", field = "cooldown_ms", value = "21000" / "23000", expect_override_version = 5)`,重复 200 次(每次重置文件到版本 5)
- 期望:
  - 每次恰好一个返回 `ok = true`、另一个返回 `error = VersionChanged`;文件版本 6,`cooldown_ms` 等于成功那个请求的值;文件可被 TOML 解析(无交错写入)
- 实现:`python/pipeline/tests/test_api.py::test_pip_106_concurrent_apply`

### PIP-107 preview_override 返回差异、放宽提示、结构标志、受影响实例与当前版本

- 规则:[参数通道](README.md#参数通道)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;`overrides/pair-p1.toml` 版本 2;`live-a` 是 Live 实例
- 步骤:
  1. `preview_override(target = "pair:p1", field = "max_notional", group = None, value = "12000")`
- 期望:
  - `ok = true`;`diff` = `[{instance: "live-a", field: "max_notional", from: "10000", to: "12000"}, {instance: "sim-a", field: "max_notional", from: "10000", to: "12000"}]`;`relax_hint = "loosen"`(`relax = Up`,变大);`structural = false`;`instances = [{name: "live-a", live: true}, {name: "sim-a", live: false}]`;`override_version = 2`
  - 任何文件不变
- 实现:`python/pipeline/tests/test_api.py::test_pip_107_preview_fields`

### PIP-108 preview_override 的放宽提示按字段 relax 方向

- 规则:[参数通道](README.md#参数通道)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;目标 `instance:sim-a`;当前值取 `js0`
- 实现:`python/pipeline/tests/test_api.py::test_pip_108_relax_hint`

| 行 | 字段与新值 | 期望 `relax_hint` |
|---|---|---|
| PIP-108.1 | `open_th`(Down)0.004(当前 0.005) | `loosen` |
| PIP-108.2 | `open_th` 0.006 | `tighten` |
| PIP-108.3 | `max_notional`(Up)9000 | `tighten` |
| PIP-108.4 | `max_notional` 11000 | `loosen` |
| PIP-108.5 | `score`(None)0.1 | `unknown` |

### PIP-109 结构字段的修改作用于 Live 实例时预览即拒绝

- 规则:[参数通道](README.md#参数通道)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;每行设定 `live-a` 的 cell 期望模式并调用 `preview_override(field = "pct_bin", value = "0.0002")`
- 实现:`python/pipeline/tests/test_api.py::test_pip_109_structural_preview_live`

| 行 | 目标与 `live-a` 状态 | 期望 |
|---|---|---|
| PIP-109.1 | `instance:live-a`,cell 期望 `Live` | `ok = false`,`error = StructuralLive`;无文件改动 |
| PIP-109.2 | `pair:p1`,`live-a` cell 期望 `Live` | `ok = false`,`error = StructuralLive` |
| PIP-109.3 | `instance:live-a`,全部 cell 期望 `Shadow` | `ok = true`,`structural = true`,`instances = [{name: "live-a", live: false}]` |
| PIP-109.4 | `instance:sim-a` | `ok = true`,`structural = true` |

### PIP-110 preview_override 与 apply_override 对无效目标与无效值返回错误

- 规则:[参数通道](README.md#参数通道)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;每行同时以 `preview_override` 与 `apply_override(expect_override_version = 当前版本)` 调用,两者结果相同
- 实现:`python/pipeline/tests/test_api.py::test_pip_110_invalid_requests`

| 行 | 参数 | 期望 |
|---|---|---|
| PIP-110.1 | `target = "instance:nope"` | `error = UnknownTarget` |
| PIP-110.2 | `target = "pair:nope"` | `error = UnknownTarget` |
| PIP-110.3 | `target = "sim-a"`(缺前缀) | `error = UnknownTarget` |
| PIP-110.4 | `field = "no_such"` | `error = InvalidValue{field = "no_such", reason = UnknownField}` |
| PIP-110.5 | `field = "open_th"`,`value = "0.2"`(> max 0.1) | `InvalidValue{reason = OutOfRange}` |
| PIP-110.6 | `field = "cooldown_ms"`,`group = 0x0700_0001` | `InvalidValue{reason = NotPerGroup}` |
| PIP-110.7 | `field = "cooldown_ms"`,`value = "abc"` | `InvalidValue{reason = BadText}` |
| PIP-110.8 | `field = "mode"`,`value = "z"` | `InvalidValue{reason = BadEnum}` |
| PIP-110.9 | `field = "open_th"`,`value = "0.0071"` | 预览 `ok = true`;应用 `ok = true` |

### PIP-111 apply_override 写入后立即为受影响的实例各跑一轮

- 规则:[参数通道](README.md#参数通道)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;调度器的下一次定时轮在 10 min 后;`FakeCtl` 记录请求时刻
- 实现:`python/pipeline/tests/test_api.py::test_pip_111_apply_triggers_round`

| 行 | 操作 | 期望 |
|---|---|---|
| PIP-111.1 | `apply_override(instance:sim-a, cooldown_ms = 21000)`,无轮在跑 | 5 s 内 `sim-a` 一条 `SetParams`(`cooldown_ms` = 21 000);`live-a` 无请求 |
| PIP-111.2 | `apply_override(pair:p1, cooldown_ms = 21000)` | 5 s 内 `sim-a` 与 `live-a` 各一条 `SetParams` |
| PIP-111.3 | `sim-a` 正有一轮在跑(插件 `compute` 阻塞 2 s)时 `apply_override(instance:sim-a, …)` | 正在跑的那一轮结束后接着跑一轮;接着跑的那一轮的 `SetParams` 含新值;两轮不并发(同一实例同一时刻只有一轮在跑) |

### PIP-112 别的主体发出的 SetParams 以 VersionConflict 发现,暂停该实例的下发并报 P1,不自动载入覆盖项

- 规则:[参数通道](README.md#参数通道)、[输出与版本库](README.md#输出与版本库)、[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`(版本库最后 `Applied` = 41);在两轮之间 journal 出现 `cli:alice` 的 `SetParams`(`version = 50`,`G1.open_th` = 9 000 000,其余同 41);插件每轮输出 `G_BTC.fields = {open_th: Decimal("0.006")}`;`FakeCtl` 对 `base_version ≠ 50` 的 `SetParams` 回 `VersionConflict`,否则 `Applied`
- 步骤:
  1. `sim-a` 跑一轮(轮 R1)
  2. 再跑一轮(轮 R2)
  3. 再跑一轮(轮 R3)
- 期望:
  - 步骤 1:`SetParams` 带 `base_version = "41"`、`version = 51`;返回 `VersionConflict`;告警 `VersionConflict`(P1)恰一条;`overrides/sim-a.toml` 不变;`sim-a` 进入暂停
  - 步骤 2:无 `SetParams`;`publish.result = "Paused"`;`overrides/sim-a.toml` 不变;`index.jsonl` 没有 `version = 50` 的行
  - 步骤 3:同步骤 2;`alerts` 中没有新的 `VersionConflict`
- 实现:`python/pipeline/tests/test_publish.py::test_pip_112_version_conflict_pauses`

### PIP-113 StaleVersion 与其他拒绝保留旧版本并告警

- 规则:[一轮的步骤](README.md#一轮的步骤)、[输出与版本库](README.md#输出与版本库)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;插件输出 `G_BTC.fields = {open_th: Decimal("0.006")}`;每行 `FakeCtl` 对 `SetParams` 的应答
- 实现:`python/pipeline/tests/test_publish.py::test_pip_113_publish_rejections`

| 行 | 应答 | 期望 |
|---|---|---|
| PIP-113.1 | `code = StaleVersion` | `publish.result = "StaleVersion"`;告警 `PublishRejected`(P1);生效版本仍 41 |
| PIP-113.2 | `code = InvalidArgs`,`detail = {name = "ParamInvalid", arg = "Strategy:0"}` | `publish.detail = "ParamInvalid:Strategy:0"`;告警 `PublishRejected` |
| PIP-113.3 | `code = PreconditionFailed`,`detail = GroupHasPosition` | 告警 `PublishRejected`,`detail` 含 `GroupHasPosition` |
| PIP-113.4 | `status = "accepted"`,随后按 `cmd_id` 补查得 `Applied` | `publish.result = "Applied"`;无告警 |
| PIP-113.5 | 连接 ctl socket 失败(`ConnectionRefusedError`) | `publish.result = "Unreachable"`;告警 `PublishRejected`;进程不退出,下一轮重算 |

### PIP-114 新版本号 = max(版本库最大版本, 当前生效版本) + 1

- 规则:[输出与版本库](README.md#输出与版本库)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;插件输出与生效内容不同;每行设定版本库 `index.jsonl` 中的最大版本与 journal 中的生效版本
- 实现:`python/pipeline/tests/test_publish.py::test_pip_114_version_allocation`

| 行 | 版本库最大 / 生效 | 期望 `version` |
|---|---|---|
| PIP-114.1 | 41 / 41 | 42 |
| PIP-114.2 | 41 / 50 | 51 |
| PIP-114.3 | 55 / 50(55 曾被拒或未生效) | 56 |
| PIP-114.4 | 空 / 无生效参数 | 1 |
| PIP-114.5 | 空 / 7 | 8 |

### PIP-115 base_version 取版本库中最后一个已生效的版本

- 规则:[输出与版本库](README.md#输出与版本库)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;插件输出与生效内容不同;每行设定 `index.jsonl` 与 journal
- 实现:`python/pipeline/tests/test_publish.py::test_pip_115_base_version`

| 行 | `index.jsonl` 与生效版本 | 期望 `base_version` |
|---|---|---|
| PIP-115.1 | 最后一项 41(`Applied`);生效 41 | `"41"` |
| PIP-115.2 | 41(`Applied`)、42(被拒);生效 41 | `"41"` |
| PIP-115.3 | 41(`Applied`)、50(`source = external`);生效 50 | `"50"` |
| PIP-115.4 | 空;生效 7 | `"7"` |
| PIP-115.5 | 空;无生效参数 | 请求中没有 `base_version` 字段 |
| PIP-115.6 | 41(`Applied`);生效 50(`cli:alice` 发出,版本库中没有 50) | `"41"`(下发将得到 `VersionConflict`,见 PIP-112) |
| PIP-115.7 | 41(`Applied`)、42(`Unreachable`,`42.bin` 已写);生效 42,SHA-256 与 `42.bin` 相同 | `"42"` |

### PIP-116 内容不变时不产生新版本、不发命令

- 规则:[一轮的步骤](README.md#一轮的步骤)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;插件输出 `out_pf()`
- 实现:`python/pipeline/tests/test_publish.py::test_pip_116_unchanged_no_publish`

| 行 | 相对 `js0` 的差别 | 期望 |
|---|---|---|
| PIP-116.1 | 无 | `FakeCtl` 无请求;`index.jsonl` 不增行;`status.publish.result = "Unchanged"` |
| PIP-116.2 | 只有 `snap.ts` 不同(头的 `generated_at_ns` 会不同) | 同上 |
| PIP-116.3 | 插件 `reason` 文本与 `notes` 不同 | 同上 |
| PIP-116.4 | `G_BTC.fields = {score: Decimal("0.000000001")}` | 一条 `SetParams`,版本 42 |

### PIP-117 带到期时间的实例覆盖项只作用于该模拟实例,到期后回到插件值

- 规则:[参数通道](README.md#参数通道)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;`overrides/sim-a.toml` 一项 `{field = "cooldown_ms", value = "40000", scope = "instance", set_by = "tg:1002", set_at = T0 − 1 min, expires_at = T0 + 1 h}`(研究提案进入时的形状);插件输出全局 `cooldown_ms: Decimal("20000")`
- 步骤:
  1. `snap.ts = T0`,`sim-a`、`live-a` 各跑一轮
  2. `snap.ts = T0 + 1 h`,`sim-a` 再跑一轮
- 期望:
  - 步骤 1:`sim-a` 的 `cooldown_ms` = 40 000;`live-a` 无 `SetParams`
  - 步骤 2:`sim-a` 的 `SetParams` 中 `cooldown_ms` = 20 000
- 实现:`python/pipeline/tests/test_overrides.py::test_pip_117_expiring_sim_override`

### PIP-118 pipeline 只构造权限表允许的四种请求

- 规则:[参数通道](README.md#参数通道)、[依赖](README.md#依赖)
- 级别:unit
- 里程碑:M4
- 前置:`python/pipeline/src/hft_pipeline/` 全部源码
- 步骤:
  1. AST 扫描所有对 `hftctl` 客户端的调用与请求体中的 `type` 字面量
- 期望:
  - 出现的请求类型只有 `SetParams`、`InjectWarmup`、`GetSchema`(经 `get_schema`),以及 group 级 `SetMode`(若出现,`mode` 字面量只有 `Shadow`、`CloseOnly` 且 `group` 不是 `"*"`);没有 `Kill`、`Resume`、`ReplaceRefdata`、`SetRisk`、`StrategyCmd`、`VenueNotice`、`SmokeOrder`
- 实现:`python/pipeline/tests/test_static.py::test_pip_118_only_allowed_commands`

### PIP-119 共享 policy 的改动作用于校准对内两个实例并标出影响实盘

- 规则:[参数通道](README.md#参数通道)、[实例配置](README.md#实例配置)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`,`live-a.policy = sim-a.policy = "policy/p1.toml"`(`[fields] max_groups = "8"`);`js0`
- 步骤:
  1. 把 `policy/p1.toml` 改为 `max_groups = "10"`
  2. `sim-a`、`live-a` 各跑一轮
- 期望:
  - 两个实例各一条 `SetParams`,`max_groups` = 10
  - 两个 `status.json` 的 `policy = {version: <新文件 SHA-256>, shared: true, affects_live: true}`
- 实现:`python/pipeline/tests/test_policy.py::test_pip_119_shared_policy_affects_live`

### PIP-120 policy_version 是 policy 文件内容的 SHA-256

- 规则:[参数通道](README.md#参数通道)、[输出与版本库](README.md#输出与版本库)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;`policy/sim-a.toml` 按行改动后跑一轮
- 实现:`python/pipeline/tests/test_policy.py::test_pip_120_policy_version_hash`

| 行 | 改动 | 期望 |
|---|---|---|
| PIP-120.1 | 不改 | `status.policy.version` 与上一轮相同,等于文件字节的 SHA-256 |
| PIP-120.2 | 末尾加一个空格 | 新的 SHA-256,与上一轮不同;版本库新增行(若有下发)记录新的 `policy_version` |

### PIP-121 命令发往策略实例所在 hft-core 实例的 ctl socket

- 规则:[实现说明](README.md#实现说明)、[实例配置](README.md#实例配置)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;`sim-a` 与 `live-a` 的插件输出都与生效内容不同
- 步骤:
  1. 两个实例各跑一轮
- 期望:
  - `sim-a` 的 `SetParams` 写到 `<tmp>/sim.sock`;`live-a` 的写到 `<tmp>/prod.sock`;两者 `issuer = "pipeline"`
- 实现:`python/pipeline/tests/test_publish.py::test_pip_121_route_to_core_socket`

**预热夹具**(本节用例共用):`wo_pf(n)` = `WarmupOut{batches = (WarmupBatch{local_kind = 7, blocks = (b0, …, b(n−1))},), samples = 1440, span = (T0 − 1 天, T0), source = "collector", ok = True, notes = ()}`,每块 `bk` 长 1 000 B、首字节为载荷版本 1、其余为 `k` 的确定字节模式;`FakeCtl` 对 `InjectWarmup` 默认回 `Applied`;假 collector Parquet 与假 REST K 线都可用。

### PIP-130 检查点被丢弃时自动注入一次整实例预热

- 规则:[Model 预热](README.md#model-预热)、[一轮的步骤](README.md#一轮的步骤)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`(`warmup.auto = true`)、`js0`;journal 在 seq 1 000 出现 `sim-a` 的 `Note{ModelCheckpointDiscarded}`,`v[1]` 按行;插件 `warmup` 返回 `wo_pf(3)`;插件 `compute` 输出与生效内容相同(不下发)
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_130_checkpoint_discarded_triggers`

| 行 | `v[1]` | 期望 |
|---|---|---|
| PIP-130.1 | `Missing`(1;含 hft-core 启动时没有检查点) | 这一轮调用插件 `warmup` 一次,`WarmupSnapshot.groups = None`;`FakeCtl` 依次收到 3 条 `InjectWarmup`(`chunk_no` 0、1、2,`last` 只在 2 为 true);`warmup.result = "Applied"` |
| PIP-130.2 | `Corrupt`(2) | 同上 |
| PIP-130.3 | `KeyMismatch`(3) | 同上 |
| PIP-130.4 | `SeqNotInJournal`(4) | 同上 |
| PIP-130.5 | `RestoreErr`(5) | 同上 |
| PIP-130.6 | 该 Note 属于 `sim-b`(`v[0] = 9`) | `sim-a` 不预热;`sim-b` 的那一轮预热 |

### PIP-131 整实例预热完成后不重复,之后的新 Note 再次触发

- 规则:[Model 预热](README.md#model-预热)
- 级别:unit
- 里程碑:M4
- 前置:PIP-130.1 之后(seq 1 000 的 Note 已完成整实例预热);插件 `warmup` 返回 `wo_pf(3)`
- 步骤:
  1. `sim-a` 再跑两轮(journal 无新 Note)
  2. journal 在 seq 5 000 出现新的 `Note{ModelCheckpointDiscarded, v[1] = Missing}`(hft-core 重启且检查点未写出),再跑一轮
- 期望:
  - 步骤 1:两轮都无 `InjectWarmup`,插件 `warmup` 未被调用
  - 步骤 2:一次整实例预热,3 条 `InjectWarmup`
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_131_whole_instance_once_per_note`

### PIP-132 ModelGroupReset 只为该 group 预热

- 规则:[Model 预热](README.md#model-预热)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;journal 出现 `Note{source = Advisor, code = ModelGroupReset, strategy = 7, group = 0x0700_0002, v[0] = Structural(1), v[1] = 41}`;插件 `warmup` 返回 `wo_pf(2)`
- 步骤:
  1. `sim-a` 跑一轮
- 期望:
  - 插件 `warmup` 被调用一次,`WarmupSnapshot.groups = [0x0700_0002]`;2 条 `InjectWarmup`
  - `status.json` 记下已完成键 `(0x0700_0002, 41)`
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_132_group_reset_warms_group`

### PIP-133 下发新增的 GroupId 在 Applied 之后只为新 group 预热

- 规则:[Model 预热](README.md#model-预热)、[一轮的步骤](README.md#一轮的步骤)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;插件输出 `out_pf()` 再加 `G_BTCC`(新指纹);`warmup` 返回 `wo_pf(2)`;`FakeCtl` 记录请求顺序
- 步骤:
  1. `sim-a` 跑一轮
- 期望:
  - 请求顺序:`SetParams`(`version = 42`,新增 `0x0700_0003`,`Applied`)→ 2 条 `InjectWarmup`
  - 插件 `warmup` 收到 `WarmupSnapshot.groups = [0x0700_0003]`;已完成键 `(0x0700_0003, 42)`
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_133_new_group_warmed_after_applied`

### PIP-134 同一参数版本既新增 GroupId 又出现 ModelGroupReset 时只预热一次

- 规则:[Model 预热](README.md#model-预热)
- 级别:unit
- 里程碑:M4
- 前置:PIP-133 的步骤 1 之后(键 `(0x0700_0003, 42)` 已完成);随后 journal 出现 `Note{ModelGroupReset, group = 0x0700_0003, v[0] = Fingerprint(2), v[1] = 42}`
- 步骤:
  1. `sim-a` 再跑一轮
- 期望:
  - 无 `InjectWarmup`;插件 `warmup` 未被调用
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_134_dedupe_same_version`

### PIP-135 hft-core 重启追赶时重放出的同一 ModelGroupReset 不再触发

- 规则:[Model 预热](README.md#model-预热)
- 级别:table
- 里程碑:M4
- 前置:PIP-132 之后(键 `(0x0700_0002, 41)` 已完成);hft-core 重启,journal 新段中再次出现 `ModelGroupReset`(seq 更大),字段按行
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_135_replayed_reset_ignored`

| 行 | 新出现的 Note | 期望 |
|---|---|---|
| PIP-135.1 | `group = 0x0700_0002`,`v[1] = 41` | 无 `InjectWarmup` |
| PIP-135.2 | `group = 0x0700_0002`,`v[1] = 43` | 一次只为 `0x0700_0002` 的预热;已完成键增加 `(0x0700_0002, 43)` |
| PIP-135.3 | `group = 0x0700_0001`,`v[1] = 41` | 一次只为 `0x0700_0001` 的预热 |

### PIP-136 整实例预热完成后,同一参数版本下的按 group 触发视为完成

- 规则:[Model 预热](README.md#model-预热)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`(生效 `version = 41`);同一轮内 journal 先后出现 `Note{ModelCheckpointDiscarded, v[1] = Missing}` 与按行的 group 触发;插件 `warmup` 返回 `wo_pf(2)`
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_136_whole_instance_covers_groups`

| 行 | group 触发 | 期望 |
|---|---|---|
| PIP-136.1 | `ModelGroupReset{group = 0x0700_0001, v[1] = 41}` | 只有一次整实例预热(`groups = None`);之后的轮次不再为 `0x0700_0001` 预热 |
| PIP-136.2 | 整实例预热完成之后,`SetParams` `version = 42` `Applied`,随后 `ModelGroupReset{group = 0x0700_0001, v[1] = 42}` | 整实例预热之后另有一次只为 `0x0700_0001` 的预热 |

### PIP-137 非结构参数、refdata 变化与发版不触发预热

- 规则:[Model 预热](README.md#model-预热)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;每行 journal 出现的事件(没有 `ModelCheckpointDiscarded`、`ModelGroupReset`)后 `sim-a` 跑一轮
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_137_no_trigger_events`

| 行 | 事件 | 期望 |
|---|---|---|
| PIP-137.1 | `SetParams` `Applied`,只改 `cooldown_ms`(非结构) | 无 `InjectWarmup` |
| PIP-137.2 | `ReplaceRefdata` 之后 hft-core 重启,advisor 载入了检查点(无丢弃 Note) | 无 `InjectWarmup` |
| PIP-137.3 | 新发布版本启动,载入了检查点 | 无 `InjectWarmup` |
| PIP-137.4 | `SetParams` 改 `pct_bin`(结构字段)且随后出现 `ModelGroupReset{group = 0x0700_0001, v[1] = 42}` | 一次只为 `0x0700_0001` 的预热(触发来自 Note,不来自 `SetParams` 本身) |

### PIP-138 warmup.auto = false 时只在手动时注入

- 规则:[Model 预热](README.md#model-预热)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`,`warmup.auto = false`;journal 出现 `Note{ModelCheckpointDiscarded, v[1] = Missing}`;插件 `warmup` 返回 `wo_pf(2)`
- 步骤:
  1. `sim-a` 跑两轮
  2. 经本地接口调用 `run_warmup("sim-a")`
- 期望:
  - 步骤 1:无 `InjectWarmup`
  - 步骤 2:2 条 `InjectWarmup`;返回值即 `status.json` 的 `warmup` 段,`result = "Applied"`,`batches = [{local_kind: 7, chunks: 2}]`
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_138_auto_disabled_manual_only`

### PIP-139 下发被拒时新 group 不预热,其他触发照常

- 规则:[一轮的步骤](README.md#一轮的步骤)、[Model 预热](README.md#model-预热)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;插件输出新增 `G_BTCC`;`FakeCtl` 对 `SetParams` 回 `InvalidArgs{LotMismatch}`;同一轮 journal 有 `Note{ModelGroupReset, group = 0x0700_0002, v[1] = 41}`
- 步骤:
  1. `sim-a` 跑一轮
- 期望:
  - 插件 `warmup` 只被调用一次,`groups = [0x0700_0002]`;没有为 `0x0700_0003` 的预热
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_139_rejected_publish_skips_new_groups`

### PIP-140 这一轮不下发时预热直接执行

- 规则:[一轮的步骤](README.md#一轮的步骤)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;插件输出与生效内容相同;journal 有 `Note{ModelCheckpointDiscarded, v[1] = Missing}`;插件 `warmup` 返回 `wo_pf(1)`
- 步骤:
  1. `sim-a` 跑一轮
- 期望:
  - 无 `SetParams`;1 条 `InjectWarmup`(`chunk_no = 0`,`last = true`)
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_140_warmup_without_publish`

### PIP-141 预热取数按 warmup.sources 的顺序尝试,一个来源缺数据时换下一个

- 规则:[Model 预热](README.md#model-预热)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;整实例触发;插件 `warmup_inputs` 声明 `W_col` 与 `W_rest`;插件按收到的数据设 `WarmupOut.source`
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_141_source_order`

| 行 | `warmup.sources` / 数据可用性 | 期望 |
|---|---|---|
| PIP-141.1 | `["collector", "rest"]` / 都可用 | 只读 collector Parquet,无 REST 请求;`warmup.source = "collector"` |
| PIP-141.2 | `["collector", "rest"]` / collector 前缀下无该时间窗的对象 | 读 collector 后再取 REST;`warmup.source = "rest"` |
| PIP-141.3 | `["rest", "collector"]` / 都可用 | 只发 REST 请求;`warmup.source = "rest"` |
| PIP-141.4 | `["collector", "rest"]` / collector 缺、REST 返回 429 | 插件收到两个来源都失败的 `WarmupSnapshot`;插件返回 `ok = False` 时不注入,`result = "NotInjected"` |

### PIP-142 预热取到的原始输入存入这一轮的 rounds 目录

- 规则:[Model 预热](README.md#model-预热)、[输出与版本库](README.md#输出与版本库)
- 级别:unit
- 里程碑:M4
- 前置:PIP-141.2 的条件;这一轮的 `ts` = T0
- 步骤:
  1. `sim-a` 跑一轮
- 期望:
  - `<state>/sim-a/rounds/<T0 的目录名>/warmup/` 下有 REST K 线的原始响应文件,内容与假服务器返回的字节相同
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_142_raw_inputs_saved`

### PIP-143 WarmupSnapshot.params 是当前生效的参数,热更后下一次预热用新值

- 规则:[Model 预热](README.md#model-预热)、[插件接口](README.md#插件接口)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`(`cooldown_ms` = 20 000);参考插件 `pf_ref` 的 `warmup` 按 `snap.params["cooldown_ms"]` 毫秒数 × 1 000 计算回看跨度并请求 `W_rest`
- 步骤:
  1. 整实例触发,跑一轮
  2. journal 出现 `SetParams` `Applied`(`version = 42`,`cooldown_ms` = 30 000)与新的 `Note{ModelCheckpointDiscarded}`,再跑一轮
- 期望:
  - 步骤 1:插件收到的 `snap.params["cooldown_ms"] = 20000`;REST 请求的时间窗跨度 20 000 s
  - 步骤 2:`snap.params["cooldown_ms"] = 30000`;时间窗跨度 30 000 s
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_143_params_from_effective`

### PIP-144 插件 warmup 返回 ok = False 时不注入并告警

- 规则:[Model 预热](README.md#model-预热)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`;整实例触发;插件 `warmup` 返回 `wo_pf(3)` 但 `ok = False`、`samples = 100`、`notes = ("need 1440 samples",)`
- 步骤:
  1. `sim-a` 跑一轮
  2. 再跑一轮(插件仍返回 `ok = False`)
- 期望:
  - 步骤 1:无 `InjectWarmup`;`warmup.result = "NotInjected"`,`samples = 100`;告警 `WarmupNotInjected`(P1),`detail` 含 `need 1440 samples`
  - 步骤 2:插件 `warmup` 再被调用一次(触发键未完成)
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_144_plugin_not_ok`

### PIP-145 任一块超过 4096 B 时整批不注入

- 规则:[Model 预热](README.md#model-预热)、[插件接口](README.md#插件接口)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;整实例触发;插件 `warmup` 返回一批 3 块,第 2 块(下标 1)长度按行,其余 1 000 B
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_145_block_size_limit`

| 行 | 第 2 块长度 | 期望 |
|---|---|---|
| PIP-145.1 | 4 095 B | 3 条 `InjectWarmup` |
| PIP-145.2 | 4 096 B | 3 条 `InjectWarmup`,第 2 条 payload 长 4 096 |
| PIP-145.3 | 4 097 B | 0 条 `InjectWarmup`(第 1 块也不发);`result = "NotInjected"`;告警 `WarmupNotInjected`,`detail` 含 `chunk 1` 与 `4097` |

### PIP-146 数百块的注入在限速下退避后全部 Applied,块不被切分或拼接

- 规则:[Model 预热](README.md#model-预热)
- 级别:integ
- 里程碑:M4
- 前置:`cfg_pf`;整实例触发;插件 `warmup` 返回 `wo_pf(600)`;用真实 `hftctl.py` 连接假 gateway:每秒放行 50 个 `InjectWarmup`,超出的回 `RateLimited`,第 100 块首次回 `Busy`
- 步骤:
  1. `sim-a` 跑一轮(墙钟加速)
- 期望:
  - 假 gateway 收到的已放行请求恰好 600 个,`chunk_no` 为 0–599 连续、各一次;只有 `chunk_no = 599` 的 `last = true`
  - 第 k 个已放行请求的 payload 与插件给出的 `b_k` 逐字节相同(长度都为 1 000)
  - 被 `RateLimited`/`Busy` 回绝的请求以同一 `cmd_id` 重发;`warmup.result = "Applied"`,`batches = [{local_kind: 7, chunks: 600}]`
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_146_600_blocks_rate_limited`

### PIP-147 注入中途失败时该批从 chunk_no = 0 重发一次

- 规则:[Model 预热](README.md#model-预热)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;整实例触发;插件 `warmup` 返回 `wo_pf(5)`;每行脚本化 `FakeCtl`
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_147_resend_from_zero`

| 行 | 脚本 | 期望 |
|---|---|---|
| PIP-147.1 | `chunk_no = 3` 第一次回 `InvalidArgs`,之后都 `Applied` | 请求序列 `chunk_no` = 0,1,2,3,0,1,2,3,4;`result = "Applied"`;触发键记为完成 |
| PIP-147.2 | 发到 `chunk_no = 2` 时连接断开,之后可连 | 序列 0,1,2,0,1,2,3,4;`result = "Applied"` |
| PIP-147.3 | `chunk_no = 3` 两次都回 `InvalidArgs` | 序列 0,1,2,3,0,1,2,3;`result = "Failed"`,`detail` 含 `InvalidArgs`;告警 `WarmupNotInjected`;触发键不记为完成,下一轮再次调用插件 `warmup` |

### PIP-148 多个 local_kind 按批的顺序注入,每批 chunk_no 各自从 0 起

- 规则:[Model 预热](README.md#model-预热)、[插件接口](README.md#插件接口)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`;整实例触发;插件返回两批:`local_kind = 7` 两块、`local_kind = 9` 三块
- 步骤:
  1. `sim-a` 跑一轮
- 期望:
  - 请求序列:`(7, 0, false)`、`(7, 1, true)`、`(9, 0, false)`、`(9, 1, false)`、`(9, 2, true)`(`local_kind`、`chunk_no`、`last`)
  - `warmup.batches = [{local_kind: 7, chunks: 2}, {local_kind: 9, chunks: 3}]`
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_148_batches_in_order`

### PIP-149 InjectWarmup 以 pipeline 主体发出且作用于实例

- 规则:[Model 预热](README.md#model-预热)
- 级别:unit
- 里程碑:M4
- 前置:PIP-140 的条件
- 步骤:
  1. `sim-a` 跑一轮,取 `InjectWarmup` 请求
- 期望:
  - `type = "InjectWarmup"`、`issuer = "pipeline"`、`strategy = "sim-a"`、`account` 缺省或为 `"*"`、`local_kind = "7"`、`chunk_no = "0"`、`last = true`、`payload_b64` 解码 = `b0`;写到 `<tmp>/sim.sock`
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_149_inject_request_shape`

### PIP-150 status.json 的 warmup 段记录来源、跨度、样本数、批次与结果

- 规则:[Model 预热](README.md#model-预热)、[一轮的步骤](README.md#一轮的步骤)
- 级别:unit
- 里程碑:M4
- 前置:PIP-130.1 的条件,这一轮墙钟时刻 W
- 步骤:
  1. `sim-a` 跑一轮,读 `status.json`
- 期望:
  - `warmup = {at: W, source: "collector", span: [T0 − 86 400 000 000 000, T0], samples: 1440, batches: [{local_kind: 7, chunks: 3}], result: "Applied"}`
- 实现:`python/pipeline/tests/test_warmup.py::test_pip_150_status_warmup_section`

### PIP-151 warmup 子命令可指定来源与只生成不注入

- 规则:[使用本组件](README.md#使用本组件)、[Model 预热](README.md#model-预热)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;无任何触发 Note;插件 `warmup` 返回 `wo_pf(2)`
- 实现:`python/pipeline/tests/test_cli.py::test_pip_151_warmup_cli`

| 行 | 命令 | 期望 |
|---|---|---|
| PIP-151.1 | `hft-pipeline warmup --config <cfg_pf> --instance sim-a` | 退出码 0;2 条 `InjectWarmup`;按 `warmup.sources` 先读 collector |
| PIP-151.2 | 同上加 `--source rest` | 只发 REST 请求,不读 collector;2 条 `InjectWarmup` |
| PIP-151.3 | 同上加 `--dry-run` | 退出码 0;插件 `warmup` 被调用一次;无 `InjectWarmup` |
| PIP-151.4 | `--instance nope` | 退出码 ≠ 0;stderr 含 `nope`;无任何请求 |

### PIP-160 每次下发在版本库写 blob、索引行并上传

- 规则:[输出与版本库](README.md#输出与版本库)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`(假 S3)、`js0`、`gj0`;`overrides/sim-a.toml` 版本 3、`overrides/pair-p1.toml` 版本 2;插件版本 `0.0.1`;框架版本 `V_pipe`;插件输出 `G_BTC.fields = {open_th: Decimal("0.006")}`
- 步骤:
  1. `sim-a` 跑一轮
- 期望:
  - `versions/sim-a/42.bin` 存在,其 SHA-256 = 下发 blob 的 SHA-256
  - `versions/sim-a/index.jsonl` 新增一行:`{version: 42, sha256: <同上>, policy_version: <policy SHA-256>, override_versions: {instance: 3, pair: 2}, snapshot_ts: 1790812800000000000, pipeline_version: V_pipe, plugin_version: "0.0.1", result: "Applied", source: "pipeline"}`
  - 假 S3 `s3://test-bucket/hft/h1/sim/private/params/` 下出现与 `42.bin`、`index.jsonl`、`groups.json` 内容相同的对象
- 实现:`python/pipeline/tests/test_output.py::test_pip_160_version_store`

### PIP-161 每轮的输入快照保留 7 天

- 规则:[输出与版本库](README.md#输出与版本库)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;`<state>/sim-a/rounds/` 下预置三个目录,时刻分别为 now − 7 天 + 1 s、now − 7 天 − 1 s、now − 1 h
- 实现:`python/pipeline/tests/test_output.py::test_pip_161_rounds_retention`

| 行 | 目录 | 期望(这一轮结束后) |
|---|---|---|
| PIP-161.1 | now − 7 天 + 1 s | 仍存在 |
| PIP-161.2 | now − 7 天 − 1 s | 已删除 |
| PIP-161.3 | now − 1 h | 仍存在 |
| PIP-161.4 | 这一轮新目录 | 存在,含快照 JSON 与 Parquet 片段 |

### PIP-162 explain 给出某资产这一轮每一步的输入、结论与字段推导

- 规则:[使用本组件](README.md#使用本组件)、[插件接口](README.md#插件接口)
- 级别:table
- 里程碑:M4
- 前置:PIP-030 之后(`rounds/<ts>/` 已保存);参考插件 `pf_ref` 的 `explain` 对 BTC 返回 3 个 `ExplainStep`
- 实现:`python/pipeline/tests/test_cli.py::test_pip_162_explain`

| 行 | 命令 | 期望 |
|---|---|---|
| PIP-162.1 | `hft-pipeline explain --config <cfg_pf> --instance sim-a --asset BTC` | 退出码 0;输出依次含插件的 3 个步骤、合并结果(`open_th` = `0.006`,来源"插件")、预检与下发结果;插件 `explain` 收到的快照与该轮保存的快照逐字段相同 |
| PIP-162.2 | `--asset XRP`(这一轮没有该资产) | 退出码 0;输出写明该资产不在这一轮的 group 中 |
| PIP-162.3 | `--instance nope` | 退出码 ≠ 0;stderr 含 `nope` |

### PIP-163 diff 按 schema 渲染两个参数版本的字段级差异

- 规则:[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M4
- 前置:版本库中 `sim-a` 的 41(`js0` 内容)与 42(PIP-030 的结果)
- 实现:`python/pipeline/tests/test_cli.py::test_pip_163_diff`

| 行 | 命令 | 期望 |
|---|---|---|
| PIP-163.1 | `hft-pipeline diff --instance sim-a --from 41 --to 42` | 退出码 0;恰好一行差异:group `0x0700_0001` 的 `open_th` `0.5%` → `0.6%`(按 `Rate` 渲染) |
| PIP-163.2 | `--from 41 --to 41` | 退出码 0;无差异行 |
| PIP-163.3 | `--from 41 --to 99`(不存在) | 退出码 ≠ 0;stderr 含 `99` |

### PIP-164 schema 子命令打印框架看到的 schema 及其来源与版本

- 规则:[使用本组件](README.md#使用本组件)、[schema 校验与编码](README.md#schema-校验与编码)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`D_schemas`
- 实现:`python/pipeline/tests/test_cli.py::test_pip_164_schema_cli`

| 行 | 命令 | 期望 |
|---|---|---|
| PIP-164.1 | `hft-pipeline schema --strategy 65296` | 退出码 0;输出 `schema_id = 65296`、`schema_version = 2`、来源 = `D_schemas` 的路径、9 个字段的名称、单位、范围与默认值 |
| PIP-164.2 | `--strategy pip-fix` | 同上 |
| PIP-164.3 | `--strategy 1234`(不在 `index.json`) | 退出码 ≠ 0 |

### PIP-165 once 立即跑一轮,--dry-run 只写出 blob 不发命令

- 规则:[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`、`gj0`;插件输出 `G_BTC.fields = {open_th: Decimal("0.006")}`
- 实现:`python/pipeline/tests/test_cli.py::test_pip_165_once`

| 行 | 命令 | 期望 |
|---|---|---|
| PIP-165.1 | `hft-pipeline once --config <cfg_pf> --instance sim-a --dry-run` | 退出码 0;`<out>/` 下写出一个 blob 文件,解码后 `G1.open_th` = 6 000 000;`FakeCtl` 无请求;`groups.json` 与 `index.jsonl` 不变 |
| PIP-165.2 | 同上不带 `--dry-run` | 退出码 0;一条 `SetParams`(`version = 42`);`index.jsonl` 新增一行 |
| PIP-165.3 | `--instance nope` | 退出码 ≠ 0;无请求 |

### PIP-166 run 按各实例的 cadence_min 调度

- 规则:[使用本组件](README.md#使用本组件)、[实例配置](README.md#实例配置)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`,`sim-a.cadence_min = 10`、`sim-b.cadence_min = 15`、`live-a.cadence_min = 10`;假时钟从 t = 0 起,每轮耗时 0
- 步骤:
  1. `hft-pipeline run`,假时钟推进到 t = 30 min
- 期望:
  - `sim-a` 的轮次起点为 t = 0、10、20、30 min;`sim-b` 为 0、15、30 min;`live-a` 为 0、10、20、30 min
- 实现:`python/pipeline/tests/test_scheduler.py::test_pip_166_cadence`

### PIP-167 交易宇宙候选每个 UTC 日汇总一次

- 规则:[交易宇宙候选](README.md#交易宇宙候选)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;`sim-a`、`sim-b` 的插件每轮输出 `universe_candidates = ((vA-spot, Spot, XRP/vA-spot),)`;假时钟
- 实现:`python/pipeline/tests/test_universe.py::test_pip_167_daily_once`

| 行 | 轮次时刻 | 期望 |
|---|---|---|
| PIP-167.1 | 2026-10-01 23:50 的一轮之后 | 已有 `out/universe-2026-10-01.toml`(当日最早的一轮之后写出);这一轮不再改写它 |
| PIP-167.2 | 2026-10-02 00:00 的一轮结束后 | 写出 `out/universe-2026-10-02.toml` 与 `out/universe-2026-10-02.diff.json` |
| PIP-167.3 | 2026-10-02 00:10 的一轮之后 | 两个 2026-10-02 文件的 mtime 不变 |

### PIP-168 宇宙候选文件与差异报告的内容

- 规则:[交易宇宙候选](README.md#交易宇宙候选)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`rd_pf`;两个插件的候选:`XRP/vA-spot`、`XRP/vB-perp`(新增);当前名单中的 `ETH/vB-perp` 不再出现在任何插件的候选或当前 group 中;账户 `sim-pf-1` 在 `ETH/vB-perp` 有账户级持仓 −0.004,`G2` 使用 `ETH/vB-perp`
- 步骤:
  1. 生成 2026-10-02 的宇宙候选
- 期望:
  - `universe-2026-10-02.toml`:每个 inst 一项 `{inst, asset, venue, market_kind}`,含 `XRP/vA-spot`(XRP,`vA-spot`,Spot)与 `XRP/vB-perp`(XRP,`vB-perp`,Perp);能被 `hft-cfg` 的宇宙输入解析
  - `universe-2026-10-02.diff.json`:`added` 含两个 XRP inst;`removed` 含 `ETH/vB-perp`,该项列出账户 `sim-pf-1` 受影响的 group `0x0700_0002` 与持仓 −0.004,且 `has_position = true`
  - 无 refdata blob 写出;`FakeCtl` 无 `ReplaceRefdata`、无 `VenueNotice`
- 实现:`python/pipeline/tests/test_universe.py::test_pip_168_universe_outputs`

### PIP-169 插件标记的交易状态异常写进状态并告警,pipeline 不发 VenueNotice

- 规则:[交易宇宙候选](README.md#交易宇宙候选)
- 级别:unit
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;插件输出 `out_pf()` 加 `notes = ("delisting ETH/vB-perp 2026-10-05T00:00:00Z",)`
- 步骤:
  1. `sim-a` 跑一轮;`hft-pipeline explain --instance sim-a --asset ETH`
- 期望:
  - 告警 `Plugin`(P1),`detail` 含 `ETH/vB-perp`;explain 输出含同一条
  - `FakeCtl` 无 `VenueNotice` 请求
- 实现:`python/pipeline/tests/test_universe.py::test_pip_169_plugin_flag_no_venue_notice`

### PIP-170 本地接口 socket 的属主与权限

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M4
- 前置:Linux CI,以测试用户启动本地接口,配置的属组为测试组 `G_api`
- 步骤:
  1. 启动后 `stat` socket 文件
  2. 以不在 `G_api` 组的另一测试用户连接
- 期望:
  - 步骤 1:模式 `0660`,属组 `G_api`
  - 步骤 2:`connect` 返回 `EACCES`;审计日志无该连接的请求行
- 实现:`python/pipeline/tests/test_api.py::test_pip_170_socket_permissions`

### PIP-171 本地接口每个请求写一行审计日志

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;本地接口已启动;日志采集替身记录 journald 输出
- 实现:`python/pipeline/tests/test_api.py::test_pip_171_audit_lines`

| 行 | 请求 | 期望审计行 |
|---|---|---|
| PIP-171.1 | `status("sim-a")` | 一行:方法 `status`、对端 uid、参数摘要含 `sim-a`、结果 `ok` |
| PIP-171.2 | `apply_override(…, expect_override_version = 旧值)` | 一行:方法 `apply_override`、结果 `VersionChanged` |
| PIP-171.3 | `preview_override(target = "instance:nope", …)` | 一行:结果 `UnknownTarget` |
| PIP-171.4 | `run_now("sim-a")` | 一行:方法 `run_now`、结果 `ok` |

### PIP-172 status、run_now 与 run_warmup 的返回内容

- 规则:[实现说明](README.md#实现说明)、[一轮的步骤](README.md#一轮的步骤)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`js0`;PIP-030 之后
- 实现:`python/pipeline/tests/test_api.py::test_pip_172_api_returns`

| 行 | 调用 | 期望 |
|---|---|---|
| PIP-172.1 | `status("sim-a")` | `ok = true`;其余内容与 `<state>/sim-a/status.json` 逐字段相同 |
| PIP-172.2 | `run_now("sim-a")` | 跑一轮后返回 `ok = true` 与这一轮的 `status.json` 内容(`round_at` 晚于调用时刻) |
| PIP-172.3 | `run_warmup("sim-a")`(插件返回 `wo_pf(2)`) | 返回 `ok = true` 与 `warmup` 段(`result = "Applied"`) |
| PIP-172.4 | `status("nope")` | `ok = false`,`error = UnknownTarget` |

### PIP-173 状态文件与覆盖项文件写入中途崩溃时旧文件完整

- 规则:[一轮的步骤](README.md#一轮的步骤)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;在写入流程的给定点注入 `os._exit`,重启后读取
- 实现:`python/pipeline/tests/test_output.py::test_pip_173_atomic_writes`

| 行 | 文件与终止点 | 期望 |
|---|---|---|
| PIP-173.1 | `status.json`:`.tmp` 写一半 | `status.json` 为上一轮内容,可解析 |
| PIP-173.2 | `status.json`:`os.replace` 之后 | `status.json` 为这一轮内容 |
| PIP-173.3 | `overrides/sim-a.toml`:`.tmp` 写一半 | 原文件内容与 `override_version` 不变;`apply_override` 返回前进程已终止,客户端得到连接中断 |
| PIP-173.4 | `overrides/sim-a.toml`:`os.replace` 之后、响应之前 | 文件为新内容、版本 +1;客户端用旧期望版本重试得到 `VersionChanged` |

### PIP-174 S3 凭证文件的 mtime 变化时重建 DuckDB 与 polars 的凭证

- 规则:[依赖](README.md#依赖)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;凭证文件 `<tmp>/creds/hft-pipeline/credentials`(替代 `/run/hft/creds/hft-pipeline/credentials`);DuckDB S3 secret 与 polars `storage_options` 的构造函数打桩计数
- 实现:`python/pipeline/tests/test_creds.py::test_pip_174_credential_refresh`

| 行 | 两次 S3 读之间 | 期望 |
|---|---|---|
| PIP-174.1 | 凭证文件未变 | 第二次读不重建(构造计数不变) |
| PIP-174.2 | 凭证文件被替换(mtime 改变) | 第二次读之前 DuckDB secret 与 polars `storage_options` 各重建一次,使用新内容 |
| PIP-174.3 | 凭证文件被替换,写 S3(版本库上传) | boto3 经共享凭证文件读到新内容;上传成功 |

### PIP-175 禁止项:不持 key、不写 hft 文件、不在本包放插件代码

- 规则:[依赖](README.md#依赖)、[概要](README.md#概要)
- 级别:table
- 里程碑:M4
- 前置:`python/pipeline/` 的源码、`pyproject.toml` 与 `requirements.lock`
- 实现:`python/pipeline/tests/test_static.py::test_pip_175_forbidden`

| 行 | 检查 | 期望 |
|---|---|---|
| PIP-175.1 | 源码 import `hmac` 或读取 `/run/hft/*/secrets` 下的文件 | 零处命中 |
| PIP-175.2 | 源码中写文件的调用(`open(…, "w"/"a")`、`os.replace`、`Path.write_*`)的目标路径 | 只在 `paths.state`、`paths.out`、`paths.versions`、`paths.overrides` 之下 |
| PIP-175.3 | `src/hft_pipeline/` 中定义 `Plugin` 协议实现(含 `compute` 的类) | 零个(插件只在 `strategies/<name>/pipeline/` 与测试夹具) |
| PIP-175.4 | 依赖锁中出现交易所 SDK 包 | 零个 |

### PIP-176 一轮墙钟超过 cadence_min 的一半时告警

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`,`sim-a.cadence_min = 10`(一半 = 300 s);假单调时钟,插件 `compute` 内推进给定时长,其余步骤耗时 0
- 实现:`python/pipeline/tests/test_perf.py::test_pip_176_round_slow_alert`

| 行 | 一轮墙钟 | 期望 |
|---|---|---|
| PIP-176.1 | 299.999 s | 无 `RoundSlow` |
| PIP-176.2 | 300.000 s | 无 `RoundSlow` |
| PIP-176.3 | 300.001 s | 告警 `RoundSlow`(P1),`detail` 含 `300.001` 与 `600`(cadence 秒数);这一轮下发照常 |

### PIP-177 64 个 group 时框架自身开销每实例每轮少于 2 s

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:integ
- 里程碑:M4
- 前置:`cfg_pf`;参考插件 `pf_ref` 输出 64 个 group(每个 2 腿、3 个 per-group 字段),`compute` 本身耗时计入插件;外部输入从本地快照读取(无网络);`FakeCtl` 即时回 `Applied`
- 步骤:
  1. 连续跑 10 轮,每轮读 `status.json` 的 `step_ms`
- 期望:
  - 每轮 `step_ms` 中步骤 1、3–8 之和 < 2 000 ms(外部输入取自本地夹具,不经网络)
- 实现:`python/pipeline/tests/test_perf.py::test_pip_177_framework_overhead`

### PIP-178 预热取数受同一令牌桶约束,全部取完后才注入

- 规则:[实现说明](README.md#实现说明)、[延迟与资源契约](README.md#延迟与资源契约)、[Model 预热](README.md#model-预热)
- 级别:integ
- 里程碑:M4
- 前置:`cfg_pf`(`ex.test` 令牌桶 `rate_per_s = 10`、`burst = 20`);整实例触发;`warmup.sources = ["rest"]`;插件 `warmup_inputs` 需要 300 个 `https://ex.test/kline` 请求;同时 `sim-b` 的参数轮也在请求 `ex.test`
- 步骤:
  1. 运行到预热完成
- 期望:
  - 任一 1 s 窗口内 `ex.test` 的请求数(含 `sim-b`)≤ 20
  - 第一条 `InjectWarmup` 晚于第 300 个 K 线请求的响应
- 实现:`python/pipeline/tests/test_perf.py::test_pip_178_warmup_fetch_bucketed`

### PIP-179 pipeline 的外部请求走管理出口地址

- 规则:[实现说明](README.md#实现说明)
- 级别:host
- 里程碑:M4
- 环境:`dev2`,`hft-pipeline.service` 运行中,`sim` 实例常驻
- 授权:无
- 步骤:
  1. 在一轮的取数期间执行 `ss -tnp | grep hft-pipeline`
  2. 读 deploy 主机清单中的 ENI-A 主私有 IP 与 hft-core 交易连接所用的 `gw.src_ip`
- 通过标准:全部 `hft-pipeline` 外连的源地址等于 ENI-A 主私有 IP,且不等于任何 `gw.src_ip`
- 证据:`/var/lib/hft/pipeline/evidence/PIP-179-<UTC 日期>.txt`(两条命令的输出)
- 实现:`python/pipeline/tests/host/PIP-179.sh`

### PIP-180 整轮重放:用保存的快照重跑得到逐字节相同的 blob

- 规则:[实现说明](README.md#实现说明)、[输出与版本库](README.md#输出与版本库)
- 级别:golden
- 里程碑:M4
- 输入:`python/pipeline/tests/fixtures/rounds/pip-180/`(一轮保存的快照 JSON + Parquet 片段、当时的 policy、覆盖项文件、`groups.json`、生效参数 blob;插件 `pf_ref`)
- 期望文件:`python/pipeline/tests/fixtures/rounds/pip-180/expected.bin`(该轮下发的 blob);重放时禁用网络(socket 打桩为抛错),重放不发命令、不写版本库
- 实现:`python/pipeline/tests/test_golden.py::test_pip_180_round_replay`

### PIP-181 下发的 ConfigBlob 与 InjectWarmup 进入 journal,验证回放通过

- 规则:[参数通道](README.md#参数通道)、[Model 预热](README.md#model-预热)
- 级别:replay
- 里程碑:M4
- 输入:`fixtures/replay/pip-181.pointer.toml`(由 hft-testkit 的进程内 hft-core(`TestStrategies`,`sim` 实例)与本包联合运行 2 轮生成的 seq journal:实例 `sim-tp`(`S_tp`,测试插件 `pf_tp`),含一条 issuer `pipeline` 的 `SetParams` 与其 `ConfigBlob`、一批 3 块的 `InjectWarmup`)
- 期望:journal 中 `ControlCmd{SetParams}` 的 issuer 为 `pipeline`、`CmdResult` 为 `Applied`;3 条 `InjectWarmup` 的 `chunk_no` 为 0、1、2;`replayer verify` 退出码 0(意图流与状态哈希逐字节一致)
- 实现:`python/pipeline/tests/test_replay.py::test_pip_181_replay_verify`

### PIP-182 合并与比较以 journal 中的生效参数为准,不以版本库为准

- 规则:[输出与版本库](README.md#输出与版本库)、[一轮的步骤](README.md#一轮的步骤)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`、`gj0`;版本库有 `42.bin`(`G1.open_th` = 6 000 000,SHA-256 = `S42`),`index.jsonl` 中 42 的 `result = "Unreachable"`;每行设定 journal 中的生效参数;插件输出 `G_BTC.fields = {open_th: Decimal("0.006")}`
- 实现:`python/pipeline/tests/test_publish.py::test_pip_182_effective_from_journal`

| 行 | journal 生效 | 期望 |
|---|---|---|
| PIP-182.1 | `version = 42`,内容 SHA-256 = `S42`(上次下发其实已执行) | 无 `SetParams`(内容不变);之后的下发 `base_version = "42"` |
| PIP-182.2 | `version = 41`(42 未被执行) | 一条 `SetParams`,`version = 43`,`base_version = "41"`,内容与 `42.bin` 的 `GroupSpec` 与 body 相同 |
| PIP-182.3 | journal 读取失败(段损坏,`hft_py.JournalError`) | `status.ok = false`;无 `SetParams`;进程不退出,下一轮重试读取 |

### PIP-183 adopt 采纳当前生效的外来版本后解除暂停并以它为 base_version 下发

- 规则:[参数通道](README.md#参数通道)、[使用本组件](README.md#使用本组件)、[输出与版本库](README.md#输出与版本库)
- 级别:integ
- 里程碑:M4
- 前置:PIP-112 步骤 2 之后的状态(`sim-a` 暂停,journal 生效版本 50 由 `cli:alice` 发出,`G1.open_th` = 9 000 000);Admin 已用 `/params sim-a set open_th G1 0.009` 写入覆盖项;插件输出 `G_BTC.fields = {open_th: Decimal("0.006")}`;`FakeCtl` 对 `base_version = 50` 的 `SetParams` 回 `Applied`
- 步骤:
  1. `hft-pipeline adopt --config … --instance sim-a --version 50`
- 期望:
  - 退出 0;`index.jsonl` 追加一行 `version = 50`、`source = external`
  - 随即一轮:`SetParams` 带 `base_version = "50"`、`version = 51`,`G1.open_th` = 9 000 000(覆盖项盖过插件);返回 `Applied`;`publish.result = "Applied"`;之后的轮次不再是 `Paused`
- 实现:`python/pipeline/tests/test_publish.py::test_pip_183_adopt_resumes`

### PIP-184 pipeline 编码的 blob 按 hft-cfg 的水位规则写 group_ctr_hwm

- 规则:[GroupId 分配](README.md#groupid-分配)
- 级别:table
- 里程碑:M4
- 前置:`cfg_pf`;journal 中 `sim-a` 当前生效 blob 的 `group_ctr_hwm` 与本实例的最大 `n` 按行;`groups.json` 的 `next_ctr` 按行;按行决定插件是否输出一个新指纹;解码下发 blob 的头
- 实现:`python/pipeline/tests/test_groupid.py::test_pip_184_blob_hwm`

| 行 | 生效 `group_ctr_hwm` / 生效最大 `n` / `next_ctr` / 新指纹 | 期望 |
|---|---|---|
| PIP-184.1 | 5 / 2 / 3 / 无 | 下发 blob 的 `group_ctr_hwm = 5` |
| PIP-184.2 | 5 / 2 / 3 / 有 | 新 group 的 `n = 6`;`group_ctr_hwm = 6` |
| PIP-184.3 | 5 / 2 / 8 / 有 | 新 group 的 `n = 8`;`group_ctr_hwm = 8` |
| PIP-184.4 | 无生效参数 / — / 不存在 / 有 | 第一个 group 的 `n = 1`;`group_ctr_hwm` = 本 blob 的最大 `n` |

### PIP-185 暂停期间每轮照常计算、不下发、不再告警

- 规则:[参数通道](README.md#参数通道)、[一轮的步骤](README.md#一轮的步骤)
- 级别:unit
- 里程碑:M4
- 前置:PIP-112 步骤 1 之后的状态(`sim-a` 暂停);另一实例 `sim-b` 未暂停
- 步骤:
  1. `sim-a` 与 `sim-b` 各跑一轮,插件对两者都输出改动的字段
- 期望:
  - `sim-a`:`status.json` 写出,`field_changes` 非空,`publish.result = "Paused"`、`publish.detail = "50"`;`FakeCtl` 没有收到 `sim-a` 的 `SetParams`;`alerts` 中没有新的 `VersionConflict`;`overrides/sim-a.toml` 不变
  - `sim-b`:照常下发并 `Applied`
- 实现:`python/pipeline/tests/test_publish.py::test_pip_185_paused_round`

### PIP-186 adopt 的版本不是当前生效版本或实例未暂停时拒绝

- 规则:[参数通道](README.md#参数通道)
- 级别:table
- 里程碑:M4
- 前置:`sim-a` 暂停,journal 生效版本 50;`sim-b` 未暂停;每行调用一次本地接口 `adopt_external`
- 实现:`python/pipeline/tests/test_publish.py::test_pip_186_adopt_rejects`

| 行 | 调用 | 期望 |
|---|---|---|
| PIP-186.1 | `adopt_external("sim-a", 49)` | `{ok: false, error: "VersionChanged"}`;`index.jsonl` 不变;仍暂停 |
| PIP-186.2 | `adopt_external("sim-a", 51)` | 同上 |
| PIP-186.3 | `adopt_external("nosuch", 50)` | `{ok: false, error: "UnknownTarget"}` |
| PIP-186.4 | `adopt_external("sim-b", 41)`(未暂停,41 已由本包下发) | `{ok: true}`;`index.jsonl` 不变(版本库已认得 41) |

## 验收

- 本文件中里程碑 ≤ 当前里程碑的全部用例有实现且在 CI 的 `ops-py` 作业(离线 `pytest`)中通过;`prop` 用例的大例数运行(PIP-019、PIP-020 每夜 5 000 例)零失败。
- 各插件自己的金样与用例(选币规则、字段推导、预热载荷与策略 crate 解码一致)写在各插件的规格中(`strategies/<name>/pipeline/README.md`),在 CI 的 `ops-py` 作业里与本文件的框架测试一起跑;它们与本文件同为插件上线的前提。
- 上线门槛 ⑦([testing §8](../../docs/testing.md#8-上线门槛))期间,每个上线策略的插件在模拟账户运行中每轮成功率 ≥ 99 %(`status.json` 的 `ok = true` 且 `publish.result ∈ {Applied, Unchanged}` 的轮数 ÷ 总轮数);每个参数版本可在 journal 中追溯到 `index.jsonl` 记录的输入快照时刻与 `policy_version`、`override_versions`。
- 新实例上线时一次预热完成后,策略报告的样本数达到该策略的要求([launch Model 预热](../../docs/runbooks/launch.md#model-预热))。
