---
component: hft-ctl-proto
prefix: CTP
spec: README.md
---

# hft-ctl-proto 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。本文件覆盖本 crate 的帧、请求/响应模式、规范化、`cmd_hash`、确认字段检查与权限表,以及本 crate 执行的 [control-plane](../../docs/subsystems/control-plane.md) 账户语义(第 1–9 条)与 `"*"`/group 编码;命令到达 sequencer 之后的语义与结果码由 [hft-ctl](../hft-ctl/TESTS.md) 的端到端用例与 hft-sequencer 的用例覆盖。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | CTP-057、CTP-094、CTP-173 |
| 目录 | 不适用:非规范 |
| 使用本组件 | CTP-001–CTP-004、CTP-090、CTP-162、CTP-170 |
| 请求与响应 | CTP-010–CTP-025、CTP-040–CTP-053、CTP-070–CTP-072、CTP-140–CTP-161、CTP-177、CTP-178、CTP-179、CTP-180 |
| 二次确认 | CTP-001、CTP-011、CTP-016、CTP-071、CTP-072、CTP-080–CTP-082 |
| 策略参数 schema JSON | CTP-175 |
| 权限表 | CTP-081、CTP-082、CTP-090、CTP-093、CTP-095、CTP-100–CTP-139、CTP-176 |
| 主体与角色码 | CTP-091、CTP-092、CTP-100–CTP-138、CTP-176 |
| 实现说明 | CTP-018、CTP-054–CTP-058、CTP-093、CTP-094、CTP-171、CTP-174 |
| 延迟与资源契约 | CTP-172 |
| 测试与验收 | 不适用:非规范(README 中只保留指向本文件的链接) |
| 依赖 | CTP-173、CTP-174 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | CTP-016、CTP-018、CTP-161 |

## 用例

**基线 `P0`**(`crates/hft-ctl-proto/tests/common/mod.rs::p0()`;用例 `前置` 写"P0"时以下取值都显式生效,用例另列的取值覆盖它们;不依赖任何默认值):

- refdata `rd-ctl`(测试构建,`fixtures/refdata/ctl-p0.blob`):venue `binance-usdm`(`VenueId 1`)、`binance-spot`(2)、`bitget-usdt-futures`(10)、`bitget-spot`(11);资产 `USDT`(`AssetId 1`,数量指数 −6)、`BTC`(`AssetId 2`,数量指数 −8);inst `binance-usdm:BTCUSDT`(`InstId 1`)、`binance-spot:BTCUSDT`(2)、`bitget-usdt-futures:BTCUSDT`(3)、`bitget-spot:BTCUSDT`(4);策略实例 `s1-main`(`StrategyId 1`,kind_id 0xFF01)、`s2-main`(2,0xFF02)、`s9-sim`(3,0xFF01);账户 `acct-a`(`AccountId 1`,`Live`,`is_prod = false`,绑定 `s1-main`,venue 1、11)、`acct-b`(2,`Live`,`is_prod = false`,绑定 `s2-main`,venue 2、10)、`sim-a`(3,`Sim`,绑定 `s9-sim`,venue 1、11)。资产、inst、账户与实例的 id 按 [hft-refdata 实现说明](../hft-refdata/README.md#实现说明)加载校验第 3 条从 1 起稠密(名字 `s9-sim`、`sim-a` 只是标签)。
- group:`g1` = `"16777317"`(`GroupId 0x0100_0065` = `(1 << 24) | 101`,属 `s1-main`,腿 `binance-usdm:BTCUSDT` + `bitget-spot:BTCUSDT`)、`g2` = `"33554533"`(`(2 << 24) | 101`,属 `s2-main`,腿 `binance-spot:BTCUSDT` + `bitget-usdt-futures:BTCUSDT`)。
- 信封 `E0`:`v = 2`、`cmd_id = "00000000-0000-4000-8000-000000000001"`、`issuer = "cli:alice"`、`issued_at_ns = 1790000000000000000`、`expires_at_ns = 1790000030000000000`(签发后 30 s)、`preview = false`、`confirmed = false`、无 `expect_state_version`;issuer 为 `tg:*` 的用例另带 `chat_type = "private"`。
- 规范化入口:`normalize(&req, &rd-ctl)`;"记录"指 `Normalized.record` 的 `CmdHdr` + 载荷字节(布局归 [hft-event 控制命令编码](../hft-event/README.md#控制命令编码))。
- 权限入口:`allowed(roles, principal, chat, &cmd)`;主体列记号:`tgV`/`tgT`/`tgA` = `tg:1001`/`tg:1002`/`tg:1003`(角色集合分别为 {Viewer}/{Trader}/{Admin})私聊,后缀 `g` 为同一人在群聊;`cliV`/`cliT`/`cliA` = `cli:vera`/`cli:tom`/`cli:alice`(角色集合 {Viewer}/{Trader}/{Admin},无聊天类型);`bot` = `ops_bot`、`pipe` = `pipeline`、`recon` = `reconciler`、`sysd` = `cli:systemd`(进程主体,角色集合为空)。

### CTP-001 协议常量的取值

- 规则:[使用本组件](README.md#使用本组件)、[二次确认](README.md#二次确认)
- 级别:table
- 里程碑:M2
- 前置:无参数
- 实现:`crates/hft-ctl-proto/tests/consts.rs::ctp_001_protocol_constants`

| 行 | 常量 | 期望 |
|---|---|---|
| CTP-001.1 | `PROTO_VERSION` | `2` |
| CTP-001.2 | `MAX_FRAME` | `1 048 576`(`1 << 20`) |
| CTP-001.3 | `MAX_BLOB` | `786 432`,且等于 `MAX_FRAME * 3 / 4` |
| CTP-001.4 | `VIEWER_CONFIRM_TTL_MS` | `60 000` |
| CTP-001.5 | 对 `MAX_BLOB` 字节做 base64 编码 | 长度 `1 048 576`,即 base64 文本恰好等于一帧上限(加上 JSON 信封后放不进一帧,所以经 socket 到达的 blob 实际更小) |

### CTP-002 frame_len 按大端读长度并在上限处分界

- 规则:[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M2
- 前置:`MAX_FRAME = 1 048 576`
- 实现:`crates/hft-ctl-proto/tests/frame.rs::ctp_002_frame_len_bounds`

| 行 | `hdr` | 期望 |
|---|---|---|
| CTP-002.1 | `00 00 00 00` | `Ok(0)` |
| CTP-002.2 | `00 00 00 01` | `Ok(1)` |
| CTP-002.3 | `00 00 10 00` | `Ok(4 096)`(大端;按小端读会得到 1 048 576) |
| CTP-002.4 | `00 0F FF FF` | `Ok(1 048 575)`(上限 − 1) |
| CTP-002.5 | `00 10 00 00` | `Ok(1 048 576)`(上限) |
| CTP-002.6 | `00 10 00 01` | `Err(FrameErr::TooLong)`(上限 + 1) |
| CTP-002.7 | `FF FF FF FF` | `Err(FrameErr::TooLong)` |

### CTP-003 encode_frame 在输出尾部追加大端长度与原字节

- 规则:[使用本组件](README.md#使用本组件)
- 级别:unit
- 里程碑:M2
- 前置:`out` 预先含 1 字节 `0x78`
- 步骤:
  1. `encode_frame(b"{}", &mut out)`
  2. `encode_frame(b"", &mut out)`
- 期望:
  - 步骤 1 后 `out = 78 00 00 00 02 7B 7D`(原有字节保留,不清空)
  - 步骤 2 后 `out = 78 00 00 00 02 7B 7D 00 00 00 00`
- 实现:`crates/hft-ctl-proto/tests/frame.rs::ctp_003_encode_frame_appends`

### CTP-004 任意 JSON 字节编码成帧后按任意切分读回原字节

- 规则:[使用本组件](README.md#使用本组件)
- 级别:prop
- 里程碑:M2
- 性质:`encode_frame(j)` 的前 4 字节交给 `frame_len` 得到 `j.len()`,其后 `j.len()` 字节等于 `j`;把多帧拼接后按随机切点分批喂给测试中的增量读取器(每批调用一次 `frame_len`/取体),读出的帧序列与写入序列逐字节相同
- 生成:1–8 帧;每帧 0–65 536 字节的任意字节;切点 0–16 个,位置均匀
- 参考:写入的帧列表
- 例数:PR 1 000
- 实现:`crates/hft-ctl-proto/tests/frame.rs::ctp_004_frame_roundtrip_any_split`

### CTP-010 合法请求解析为 CmdRequest 的各字段

- 规则:[请求与响应](README.md#请求与响应)
- 级别:unit
- 里程碑:M2
- 前置:P0;请求 JSON = `{"v":2,"cmd_id":"5b0a0000-0000-4000-8000-000000000002","issuer":"tg:1002","issued_at_ns":1790000000000000000,"expires_at_ns":1790000030000000000,"account":"acct-a","preview":false,"confirmed":true,"expect_state_version":4182,"view_account":"acct-b","chat_type":"private","cmd":{"type":"SetMode","strategy":"s1-main","group":"*","mode":"Live"}}`
- 步骤:
  1. 按模式解析该 JSON
- 期望:
  - `v = 2`、`cmd_id = 5b0a0000-0000-4000-8000-000000000002`、`issuer = "tg:1002"`、`issued_at_ns = 1790000000000000000`、`expires_at_ns = 1790000030000000000`
  - `account = AccountSel::Name("acct-a")`、`preview = false`、`confirmed = true`、`expect_state_version = Some(4182)`、`view_account = Some("acct-b")`、`chat_type = Some(Private)`
  - `cmd = CmdBody::SetMode{strategy: "s1-main", group: "*", mode: Live}`
- 实现:`crates/hft-ctl-proto/tests/schema.rs::ctp_010_parse_valid_request`

### CTP-011 请求与响应模式里没有任何令牌字段

- 规则:[二次确认](README.md#二次确认)
- 级别:unit
- 里程碑:M2
- 前置:P0;从 `CmdRequest`、`CmdResponse` 的 serde 描述导出字段名列表
- 步骤:
  1. 列出 `CmdRequest` 的顶层字段
  2. 解析 `E0` + `SetMode→Live` 的请求,另加一个顶层字段 `"confirm_token":"abc"`
- 期望:
  - 步骤 1 的字段集合恰为 {`v`、`cmd_id`、`issuer`、`issued_at_ns`、`expires_at_ns`、`account`、`preview`、`confirmed`、`expect_state_version`、`view_account`、`chat_type`、`cmd`};`CmdRequest` 与 `CmdResponse` 中没有名字含 `token` 的字段
  - 步骤 2 的结果为模式错误(未知字段),渲染为 `status = invalid_request`、`code = InvalidArgs`、无 `detail`
- 实现:`crates/hft-ctl-proto/tests/schema.rs::ctp_011_no_token_fields`

### CTP-012 模式层错误一律 invalid_request

- 规则:[请求与响应](README.md#请求与响应)
- 级别:table
- 里程碑:M2
- 前置:P0;每行以 `E0` + `{"type":"SetMode","strategy":"s1-main","group":"*","mode":"CloseOnly"}` 为底,只改列出的一处;期望列"IR"= 解析失败并渲染为 `status = invalid_request`、`code = InvalidArgs`、`cmd_hash` 省略、不调用 `normalize`
- 实现:`crates/hft-ctl-proto/tests/schema.rs::ctp_012_schema_errors`

| 行 | 改动 | 期望 |
|---|---|---|
| CTP-012.1 | 帧体为字节 `FF FE 7B 7D`(非 UTF-8) | IR,无 `detail`,`cmd_id` 全零 |
| CTP-012.2 | 帧体为 `{`(不完整 JSON) | IR,无 `detail`,`cmd_id` 全零 |
| CTP-012.3 | 顶层是 JSON 数组 `[]` | IR,无 `detail` |
| CTP-012.4 | `"v":1` | IR,无 `detail` |
| CTP-012.5 | `"v":3` | IR,无 `detail` |
| CTP-012.6 | 删去 `cmd_id` | IR,无 `detail`,`cmd_id` 全零 |
| CTP-012.7 | 删去 `expires_at_ns` | IR,无 `detail` |
| CTP-012.8 | 删去 `cmd` | IR,无 `detail` |
| CTP-012.9 | 顶层多一个 `"foo":1` | IR,无 `detail` |
| CTP-012.10 | `cmd` 内多一个 `"bar":"1"` | IR,无 `detail` |
| CTP-012.11 | `"issued_at_ns":"1790000000000000000"`(信封整数写成字符串) | IR,无 `detail` |
| CTP-012.12 | `"expect_state_version":4182.0`(浮点数) | IR,无 `detail` |
| CTP-012.13 | `cmd` 换成 `SmokeOrder`,`"ttl_ms":10000`(命令参数写成 JSON 数字) | IR,无 `detail` |
| CTP-012.14 | `cmd` 换成 `SmokeOrder`,`"ttl_ms":"1e4"` | IR,无 `detail` |
| CTP-012.15 | `"type":"Foo"` | IR,`detail = UnknownDiscriminant` |
| CTP-012.16 | `"mode":"Paper"` | IR,`detail = UnknownDiscriminant` |
| CTP-012.17 | `"preview":true,"confirmed":true` | IR,无 `detail` |
| CTP-012.18 | `expires_at_ns = issued_at_ns` | IR,无 `detail` |
| CTP-012.19 | `expires_at_ns = issued_at_ns − 1` | IR,无 `detail` |
| CTP-012.20 | `expires_at_ns − issued_at_ns = 300 000 000 001` | IR,无 `detail` |
| CTP-012.21 | `expires_at_ns − issued_at_ns = 300 000 000 000` | 解析成功 |
| CTP-012.22 | `expires_at_ns − issued_at_ns = 299 999 999 999` | 解析成功 |
| CTP-012.23 | `"issuer":"tg:1002"` 且无 `chat_type` | IR,无 `detail` |
| CTP-012.24 | `"issuer":"tg:1002","chat_type":"channel"` | IR,`detail = UnknownDiscriminant` |
| CTP-012.25 | `"issuer":"cli:abcdefghijklmnopqrstu"`(25 字节) | IR,无 `detail` |
| CTP-012.26 | `"issuer":"cli:abcdefghijklmnopqrst"`(24 字节) | 解析成功 |
| CTP-012.27 | `cmd` 换成 `{"type":"GetSchema","strategy_kind":"probe-taker","kind_id":"65281"}`(两者都给) | IR,无 `detail` |
| CTP-012.28 | `cmd` 换成 `{"type":"GetSchema"}`(两者都不给) | IR,无 `detail` |
| CTP-012.29 | `cmd` 换成 `{"type":"ReconcileReport","report_id":"7","items":[]}` | IR,无 `detail` |

### CTP-013 status 与 code 的对应

- 规则:[请求与响应](README.md#请求与响应)
- 级别:table
- 里程碑:M2
- 前置:P0;每行构造一个 `CmdResponse` 并序列化为 JSON
- 实现:`crates/hft-ctl-proto/tests/response.rs::ctp_013_status_code_mapping`

| 行 | 构造 | 期望 JSON |
|---|---|---|
| CTP-013.1 | `ok`,结果码 `Applied` | `"status":"ok","code":"Applied"`,无 `detail` 键 |
| CTP-013.2 | `accepted` | `"status":"accepted"`,没有 `code` 键 |
| CTP-013.3 | `invalid_request` | `"status":"invalid_request","code":"InvalidArgs"` |
| CTP-013.4 | `unauthorized` | `"status":"unauthorized","code":"Unauthorized"` |
| CTP-013.5 | `busy` | `"status":"busy","code":"Busy"` |
| CTP-013.6 | `internal_error` | `"status":"internal_error","code":"Internal"` |
| CTP-013.7 | `ok`,`Forbidden` | `"status":"ok","code":"Forbidden"` |
| CTP-013.8 | `ok`,`RateLimited` | `"status":"ok","code":"RateLimited"` |
| CTP-013.9 | `ok`,`Expired` | `"status":"ok","code":"Expired"` |
| CTP-013.10 | `ok`,`InvalidArgs`,detail `AccountRequired`(101) | `"code":"InvalidArgs","detail":{"name":"AccountRequired"}` |

### CTP-014 CmdResult.detail 渲染为名字与 arg

- 规则:[请求与响应](README.md#请求与响应)、[control-plane 结果码](../../docs/subsystems/control-plane.md#结果码)
- 级别:table
- 里程碑:M2
- 前置:P0;输入是 `CmdResult.detail: u32 = code | arg << 16`
- 实现:`crates/hft-ctl-proto/tests/response.rs::ctp_014_detail_rendering`

| 行 | `detail` | 期望 `detail` JSON |
|---|---|---|
| CTP-014.1 | `0` | 键省略 |
| CTP-014.2 | `1` | `{"name":"ShadowDeferred"}` |
| CTP-014.3 | `110 \| (2 << 16)` | `{"name":"ParamInvalid","arg":"ModelMemBudget"}` |
| CTP-014.4 | `110 \| (3 << 16)` | `{"name":"ParamInvalid","arg":"NonProdOnly"}` |
| CTP-014.5 | `110 \| ((1 \| 0xFFF << 4) << 16)` | `{"name":"ParamInvalid","arg":"Strategy"}` |
| CTP-014.6 | `110 \| ((1 \| 3 << 4) << 16)` | `{"name":"ParamInvalid","arg":"Strategy:3"}` |
| CTP-014.7 | `402 \| (2 << 16)` | `{"name":"RefdataAssembly","arg":"MissingCredential"}` |
| CTP-014.8 | `401 \| (1 << 16)` | `{"name":"RefdataRecheck","arg":"Structure"}` |
| CTP-014.9 | `124 \| (6 << 16)` | `{"name":"GroupCheck","arg":"DanglingRef"}` |
| CTP-014.10 | `403 \| (5 << 16)` | `{"name":"SmokeRiskReject","arg":"HARD"}` |
| CTP-014.11 | `200` | `{"name":"StateChanged"}` |
| CTP-014.12 | `201` | `{"name":"NoValidPreview"}` |
| CTP-014.13 | `999` | `{"name":"999"}` |
| CTP-014.14 | `306` | `{"name":"UnknownPresent"}` |
| CTP-014.15 | `316`、`317`、`318`、`319`、`320` | 依次 `{"name":"NotQuiescent"}`、`{"name":"Hard"}`、`{"name":"NothingToFix"}`、`{"name":"QuoteUnavailable"}`、`{"name":"ChainLeg"}` |

### CTP-015 响应带规范化时算出的 cmd_hash,只读与规范化前被拒的不带

- 规则:[请求与响应](README.md#请求与响应)
- 级别:table
- 里程碑:M2
- 前置:P0;`cmd_hash` 按 CTP-070 的定义取 32 字节 `H`
- 实现:`crates/hft-ctl-proto/tests/response.rs::ctp_015_response_cmd_hash`

| 行 | 响应来源 | 期望 |
|---|---|---|
| CTP-015.1 | `SetMode→Live` 规范化成功后的响应 | `"cmd_hash"` = `H` 的 64 个小写十六进制字符 |
| CTP-015.2 | `GetSchema` 的响应 | 没有 `cmd_hash` 键 |
| CTP-015.3 | 模式层错误(CTP-012.4)的响应 | 没有 `cmd_hash` 键 |
| CTP-015.4 | `allowed` 拒绝(`Forbidden`)的响应 | 没有 `cmd_hash` 键 |
| CTP-015.5 | 规范化失败(`InvalidArgs{UnknownName}`)的响应 | 没有 `cmd_hash` 键 |
| CTP-015.6 | `ReconcileReport` 写环后的响应 | 没有 `cmd_hash` 键 |

### CTP-016 预览体由 CmdPreview 渲染,PREVIEW_TTL 时带 60 s 确认时限

- 规则:[开发备忘](README.md#开发备忘)、[二次确认](README.md#二次确认)
- 级别:table
- 里程碑:M2
- 前置:P0;`CmdPreview{target_account = 1, target_kind = 1, affected_cells = 3, open_orders = 2, …}`
- 实现:`crates/hft-ctl-proto/tests/response.rs::ctp_016_preview_rendering`

| 行 | `CmdResult.rflags` | 期望 |
|---|---|---|
| CTP-016.1 | `0` | 响应没有 `preview` 键 |
| CTP-016.2 | `PREVIEW`(1) | `preview` 存在,`target_account = "acct-a"`、`target_kind = "Live"`;没有 `confirm_within_s` 键 |
| CTP-016.3 | `PREVIEW \| PREVIEW_TTL`(5) | `preview.confirm_within_s = 60` |
| CTP-016.4 | `PREVIEW \| NEEDS_CONFIRM`(3) | `preview` 存在;没有 `confirm_within_s` 键 |
| CTP-016.5 | `PREVIEW \| NEEDS_CONFIRM`(3),`CmdPreview` 另有 `wo_venue = 11`、`wo_leg_ix = 1`、`wo_inst` = `bitget-spot:BTCUSDT`(`InstId 4`)、`wo_qty = 0.0004`(raw 40000,−8) | `preview.wo_venue = "bitget-spot"`、`wo_leg_ix = "1"`、`wo_inst = "bitget-spot:BTCUSDT"`、`wo_qty = "0.0004"` |
| CTP-016.6 | `PREVIEW`(1),`wo_venue = 0` | `preview` 中没有任何 `wo_` 开头的键 |

### CTP-017 十进制字符串按资产指数转为定点

- 规则:[请求与响应](README.md#请求与响应)
- 级别:table
- 里程碑:M2
- 前置:P0;`E0` + `{"type":"SimTransfer","from_venue":"binance-usdm","to_venue":"bitget-spot","asset":"BTC","amount":<列值>}`,`account = "sim-a"`
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_017_decimal_to_fixed`

| 行 | `amount` | 期望 |
|---|---|---|
| CTP-017.1 | `"0.5"` | 载荷 `amount = Qty(50 000 000)`(0.5,raw 50 000 000,指数 −8) |
| CTP-017.2 | `"0.00000001"` | `Qty(1)` |
| CTP-017.3 | `"0.000000001"` | `InvalidArgs{BadValue}`(多一位小数) |
| CTP-017.4 | `"46116860184.27387903"` | `Qty(4611686018427387903)`(`MAX_RAW`) |
| CTP-017.5 | `"46116860184.27387904"` | `InvalidArgs{BadValue}`(超过 `MAX_RAW`) |
| CTP-017.6 | `"abc"` | 模式层错误,`invalid_request` |
| CTP-017.7 | `0.5`(JSON 数字) | 模式层错误,`invalid_request` |

### CTP-018 名字解析为稠密 id,未知名字为 UnknownName

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)、[开发备忘](README.md#开发备忘)
- 级别:table
- 里程碑:M2
- 前置:P0;每行把列出的字段放进一条能用该字段的命令(strategy:`SetMode`;account:`HedgeFix`;venue、inst:`SmokeOrder`;asset:`SimTransfer`),其余字段合法
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_018_name_resolution`

| 行 | 字段 = 值 | 期望 |
|---|---|---|
| CTP-018.1 | `strategy = "s1-main"` | `scope.strategy = 1` |
| CTP-018.2 | `strategy = "1"`(数字 id 字符串) | `scope.strategy = 1` |
| CTP-018.3 | `strategy = "S1-MAIN"` | `InvalidArgs{UnknownName}`(不做大小写折叠) |
| CTP-018.4 | `strategy = "nope"` | `InvalidArgs{UnknownName}` |
| CTP-018.5 | `account = "acct-b"` | `CmdHdr.account = 2` |
| CTP-018.6 | `account = "2"` | `CmdHdr.account = 2` |
| CTP-018.7 | `account = "nope"` | `InvalidArgs{UnknownName}` |
| CTP-018.8 | `venue = "bitget-spot"` | 载荷 `venue = 11` |
| CTP-018.9 | `venue = "kraken"` | `InvalidArgs{UnknownName}` |
| CTP-018.10 | `inst = "binance-spot:BTCUSDT"` | 载荷 `inst = 2` |
| CTP-018.11 | `inst = "binance-spot:NOPE"` | `InvalidArgs{UnknownName}` |
| CTP-018.12 | `asset = "BTC"` | 载荷 `asset = 2` |
| CTP-018.13 | `asset = "XYZ"` | `InvalidArgs{UnknownName}` |

### CTP-019 "*" 与 "none" 的编码

- 规则:[请求与响应](README.md#请求与响应)、[control-plane 账户语义](../../docs/subsystems/control-plane.md#账户语义)
- 级别:table
- 里程碑:M2
- 前置:P0
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_019_star_and_none`

| 行 | 请求(`E0` + 命令) | 期望 |
|---|---|---|
| CTP-019.1 | `SetMode{strategy "*", group "*", mode CloseOnly}`,`account "*"` | `scope = {strategy 0, venue 0, account 0, group 0xFFFF_FFFF, inst 0}`,`CmdHdr.account = 0` |
| CTP-019.2 | `CancelAll{venue "*"}`,无 `inst`,`account "acct-a"` | `scope = {strategy 0, venue 0, account 1, group 0xFFFF_FFFF, inst 0}` |
| CTP-019.3 | `StrategyCmd{strategy "s1-main", group "none", reduce_only true, payload_b64 "AQ=="}`,`account "acct-a"` | 载荷 `group = 0`(`GROUP_NONE`) |
| CTP-019.4 | `AdoptExchange{venue "bitget-spot", report_id "7", item "Balance", asset "BTC"}`(无 `group`),`account "acct-a"` | 载荷 `group = 0` |
| CTP-019.5 | 同 019.4 另加 `group "none"` | 载荷 `group = 0` |
| CTP-019.6 | `SetMode{strategy "s1-main", group "none", mode CloseOnly}` | `InvalidArgs{BadGroup}` |
| CTP-019.7 | `StrategyCmd{…, group "*"}` | `InvalidArgs{BadGroup}` |

### CTP-020 group 写十进制 GroupId,构造规则之外的值为 BadGroup

- 规则:[请求与响应](README.md#请求与响应)、[control-plane 账户语义](../../docs/subsystems/control-plane.md#账户语义)
- 级别:table
- 里程碑:M2
- 前置:P0;`E0` + `SetMode{strategy <列>, group <列>, mode CloseOnly}`,`account "acct-a"`(strategy 为 `s2-main` 的行 `account "acct-b"`)
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_020_group_decimal`

| 行 | strategy / group | 期望 |
|---|---|---|
| CTP-020.1 | `s1-main` / `"16777317"` | `scope.group = 0x0100_0065` |
| CTP-020.2 | `s1-main` / `"33554430"`(n = `0xFF_FFFE`,上限) | `scope.group = 0x01FF_FFFE` |
| CTP-020.3 | `s1-main` / `"33554431"`(n = `0xFF_FFFF`,上限 + 1) | `InvalidArgs{BadGroup}` |
| CTP-020.4 | `s1-main` / `"16777216"`(n = 0) | `InvalidArgs{BadGroup}` |
| CTP-020.5 | `s1-main` / `"16777217"`(n = 1) | `scope.group = 0x0100_0001` |
| CTP-020.6 | `s2-main` / `"16777317"`(高 8 位 = 1 ≠ 2) | `InvalidArgs{BadGroup}` |
| CTP-020.7 | `"*"` / `"16777317"` | `scope.strategy = 0`、`scope.group = 0x0100_0065`(实例由 group 指明) |
| CTP-020.12 | `"*"` / `"4278190081"`(高 8 位 = 255,不是实例) | `InvalidArgs{BadGroup}` |
| CTP-020.13 | `"*"` / `"67108865"`(高 8 位 = 4,refdata 中没有实例 4) | `InvalidArgs{BadGroup}` |
| CTP-020.8 | `s1-main` / `"0"` | `InvalidArgs{BadGroup}` |
| CTP-020.9 | `s1-main` / `"4294967295"` | `InvalidArgs{BadGroup}`(全部只能写 `"*"`) |
| CTP-020.10 | `s1-main` / `"4294967296"` | `InvalidArgs{BadValue}`(超出 u32) |
| CTP-020.11 | `s1-main` / `"0x01000065"` | 模式层错误,`invalid_request` |

### CTP-021 降级命令缺账户作用于全部,升级与中性命令缺账户即拒

- 规则:[请求与响应](README.md#请求与响应)、[control-plane 账户语义](../../docs/subsystems/control-plane.md#账户语义)
- 级别:table
- 里程碑:M2
- 前置:P0;"缺"= 请求不带 `account` 键;其余字段合法
- 实现:`crates/hft-ctl-proto/tests/accounts.rs::ctp_021_missing_account_by_class`

| 行 | 命令 | `account` | 期望 |
|---|---|---|---|
| CTP-021.1 | `Kill{level Global}` | 缺 | `CmdHdr.account = 0` |
| CTP-021.2 | `CancelAll{venue "*"}` | 缺 | `CmdHdr.account = 0`、`scope.account = 0` |
| CTP-021.3 | `SetMode→CloseOnly{strategy "*", group "*"}` | 缺 | `CmdHdr.account = 0` |
| CTP-021.4 | `VenueNotice{venue "binance-spot", kind Delisting, at_ns "1790100000000000000"}` | 缺 | `CmdHdr.account = 0` |
| CTP-021.5 | `Drain{}` | 缺 | `CmdHdr.account = 0`,载荷 `cancel_resting = 1`(缺省 true) |
| CTP-021.6 | `InjectWarmup{strategy "s2-main", …}` | 缺 | `CmdHdr.account = 0` |
| CTP-021.7 | `SetMode→Live{strategy "*", group "*"}` | 缺 | `InvalidArgs{AccountRequired}` |
| CTP-021.8 | `SetMode→Live{strategy "*", group "*"}` | `"*"` | `CmdHdr.account = 0` |
| CTP-021.9 | `Resume{source Kill}` | 缺 | `InvalidArgs{AccountRequired}` |
| CTP-021.10 | `Resume{source Kill}` | `"*"` | `CmdHdr.account = 0` |
| CTP-021.11 | `SetRisk{blob_b64 …}` | 缺 | `InvalidArgs{AccountRequired}`(静态类别为中性) |
| CTP-021.12 | `SetRisk{blob_b64 …}` | `"*"` | `CmdHdr.account = 0` |
| CTP-021.13 | `Annotate{kind OpsNote, text "x"}` | 缺 | `InvalidArgs{AccountRequired}` |
| CTP-021.14 | `Annotate{kind OpsNote, text "x"}` | `"*"` | `CmdHdr.account = 0` |
| CTP-021.15 | `VenueNotice{venue "binance-spot", kind FundingInterval, new_s "14400"}` | 缺 | `InvalidArgs{AccountRequired}` |
| CTP-021.16 | 同 021.15 | `"*"` | `CmdHdr.account = 0` |
| CTP-021.17 | `ReplaceRefdata{blob_b64 …, version "8"}` | 缺 | `InvalidArgs{AccountRequired}` |

### CTP-022 必须给具体账户的命令写 "*" 或缺省时为 AccountRequired

- 规则:[control-plane 账户语义](../../docs/subsystems/control-plane.md#账户语义)
- 级别:table
- 里程碑:M2
- 前置:P0;每行命令的其余字段合法(group 用 `g1`,venue 与 inst 用 `bitget-spot` 与 `bitget-spot:BTCUSDT`,实例 `s1-main`)
- 实现:`crates/hft-ctl-proto/tests/accounts.rs::ctp_022_concrete_account_required`

| 行 | 命令,`account = "*"` | 期望 |
|---|---|---|
| CTP-022.1 | `SetParams` | `InvalidArgs{AccountRequired}` |
| CTP-022.2 | `HedgeFix` | `InvalidArgs{AccountRequired}` |
| CTP-022.3 | `WriteOffResidual` | `InvalidArgs{AccountRequired}` |
| CTP-022.4 | `SmokeOrder` | `InvalidArgs{AccountRequired}` |
| CTP-022.5 | `AdoptExchange` | `InvalidArgs{AccountRequired}` |
| CTP-022.6 | `SetBalanceBaseline` | `InvalidArgs{AccountRequired}` |
| CTP-022.7 | `SimReset` | `InvalidArgs{AccountRequired}` |
| CTP-022.8 | `SimTransfer` | `InvalidArgs{AccountRequired}` |
| CTP-022.9 | `RecordTransfer` | `InvalidArgs{AccountRequired}` |
| CTP-022.10 | `StrategyCmd` | `InvalidArgs{AccountRequired}` |
| CTP-022.11 | `Kill{level Account}` | `InvalidArgs{AccountRequired}` |
| CTP-022.12 | `Kill{level Account}`,不带 `account` | `InvalidArgs{AccountRequired}`(降级缺省为 `*`,而该级别要具体账户) |
| CTP-022.13 | `SmokeOrder`,不带 `account` | `InvalidArgs{AccountRequired}` |
| CTP-022.14 | `WriteOffResidual`,不带 `account` | `InvalidArgs{AccountRequired}` |
| CTP-022.15 | `SimFault`(`venue = binance-usdm`,`fault = RejectNew`) | `InvalidArgs{AccountRequired}` |

### CTP-023 只能作用于全部账户的命令给出具体账户时为 AccountNotAllowed

- 规则:[control-plane 账户语义](../../docs/subsystems/control-plane.md#账户语义)
- 级别:table
- 里程碑:M2
- 前置:P0;每行 `account = "acct-a"`,其余字段合法
- 实现:`crates/hft-ctl-proto/tests/accounts.rs::ctp_023_account_not_allowed`

| 行 | 命令 | 期望 |
|---|---|---|
| CTP-023.1 | `InjectWarmup{strategy "s1-main", local_kind "3", chunk_no "0", last false, payload_b64 "AQ=="}` | `InvalidArgs{AccountNotAllowed}` |
| CTP-023.2 | `Drain{}` | `InvalidArgs{AccountNotAllowed}` |
| CTP-023.3 | `VenueNotice{venue "binance-spot", kind Delisting, at_ns "1790100000000000000"}` | `InvalidArgs{AccountNotAllowed}` |
| CTP-023.4 | `ReconcileReport{report_id "7", items [一条]}` | `InvalidArgs{AccountNotAllowed}` |
| CTP-023.5 | `Kill{level Global}` | `InvalidArgs{AccountNotAllowed}` |
| CTP-023.6 | `InjectWarmup{…}`,`account "*"` | 成功,`CmdHdr.account = 0` |

### CTP-024 view_account 与 chat_type 不进命令与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)
- 级别:unit
- 里程碑:M2
- 前置:P0;请求 R1 = `E0` + `HedgeFix{group g1}`,`account "acct-a"`;R2 = R1 另加 `"view_account":"acct-b","chat_type":"group"`(issuer 仍为 `cli:alice`);R3 = R1 另加 `"view_account":"zzz"`
- 步骤:
  1. 分别 `normalize` R1、R2、R3
- 期望:
  - 三者的记录字节与 `cmd_hash` 逐字节相同;`CmdHdr.account = 1`(不取 `view_account`)
  - R3 规范化成功(`view_account` 解析不出不拒绝)
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_024_view_account_audit_only`

### CTP-025 信封与命令体都写账户时必须相同

- 规则:[请求与响应](README.md#请求与响应)、[control-plane 账户语义](../../docs/subsystems/control-plane.md#账户语义)
- 级别:table
- 里程碑:M2
- 前置:P0;`E0` + `SmokeOrder{venue bitget-spot, inst bitget-spot:BTCUSDT, side Buy}`,按行在信封与命令体里写 `account`
- 实现:`crates/hft-ctl-proto/tests/accounts.rs::ctp_025_envelope_vs_body_account`

| 行 | 信封 `account` / 命令体 `account` | 期望 |
|---|---|---|
| CTP-025.1 | `acct-a` / `acct-a` | `CmdHdr.account = 1`、载荷 `account = 1` |
| CTP-025.2 | `acct-a` / `acct-b` | `InvalidArgs{AccountMismatch}` |
| CTP-025.3 | 缺 / `acct-a` | `CmdHdr.account = 1` |
| CTP-025.4 | `acct-a` / 缺 | `CmdHdr.account = 1` |
| CTP-025.5 | `"*"` / `acct-a` | `InvalidArgs{AccountMismatch}` |
| CTP-025.6 | 缺 / 缺 | `InvalidArgs{AccountRequired}` |

### CTP-040 SetMode 的规范化

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)、[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)
- 级别:table
- 里程碑:M2
- 前置:P0;`E0` + `{"type":"SetMode", …}`;记录布局按 hft-event(`SetMode` 载荷 24 B)
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_040_set_mode`

| 行 | 命令字段,`account` | 期望 |
|---|---|---|
| CTP-040.1 | `strategy "s1-main", group "16777317", mode "Live"`,`"acct-a"` | `cmd = 0x0001`、`cmd_ver = 1`、`payload_len = 24`;`scope = {strategy 1, venue 0, account 1, group 0x0100_0065, inst 0}`、`mode = 4` |
| CTP-040.2 | 同上,`mode "Shadow"` | `mode = 1` |
| CTP-040.3 | 同上,`mode "CloseOnly"` | `mode = 3` |
| CTP-040.4 | `strategy "s1-main", group "*", mode "Live"`,`"acct-b"` | `InvalidArgs{AccountBound}`(`acct-b` 绑定 `s2-main`) |
| CTP-040.5 | `strategy "*", group "*", mode "CloseOnly"`,`"acct-a"` | `scope = {0, 0, 1, 0xFFFF_FFFF, 0}` |
| CTP-040.6 | `strategy "s9-sim", group "*", mode "Live"`,`"sim-a"` | `scope = {3, 0, 3, 0xFFFF_FFFF, 0}`(模拟账户同一编码) |

### CTP-041 SetParams 的规范化

- 规则:[请求与响应](README.md#请求与响应)
- 级别:table
- 里程碑:M2
- 前置:P0;blob `B` = 测试辅助按 hft-event 编码的 SPB1:`StrategyParamsHdr{strategy 1, account 1, version 42, kind_id 0xFF01}` + 1 个 `GroupSpec{id g1}` + 16 B body;`E0` + `{"type":"SetParams","strategy":"s1-main","blob_b64":<列>}`,`account "acct-a"`
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_041_set_params`

| 行 | 输入 | 期望 |
|---|---|---|
| CTP-041.1 | `blob_b64 = base64(B)` | `cmd = 0x0002`、`payload_len = 64`;`scope = {1, 0, 1, 0xFFFF_FFFF, 0}`;`BlobRef{kind = StrategyParams, encoding = 1, len = B.len(), version = 42, sha256 = SHA-256(B)}`;`params_blob = B`;`blobs` 依序拼接等于 `B` |
| CTP-041.2 | 同 041.1,另加 `"base_version":"41"` | `CmdHdr.base_version = 41` |
| CTP-041.3 | 同 041.1,无 `base_version` | `CmdHdr.base_version = 0` |
| CTP-041.4 | `B` 的头 `strategy = 2` | `InvalidArgs{ScopeMismatch}` |
| CTP-041.5 | `B` 的头 `account = 2` | `InvalidArgs{ScopeMismatch}` |
| CTP-041.6 | `blob_b64 = "%%%"` | `InvalidArgs{BadValue}` |
| CTP-041.7 | blob 只有 8 字节(短于 `StrategyParamsHdr`) | `InvalidArgs{BadValue}` |
| CTP-041.8 | `B` 的 body 补齐到总长 `786 432`(`MAX_BLOB`) | 成功,`BlobRef.len = 786 432` |
| CTP-041.9 | 总长 `786 433` | `InvalidArgs{TooLong}` |
| CTP-041.10 | `account "acct-b"`(头仍为 account 1) | `InvalidArgs{AccountBound}`(第 ③ 步先于头部核对) |

### CTP-042 SetRisk 的规范化

- 规则:[请求与响应](README.md#请求与响应)
- 级别:table
- 里程碑:M2
- 前置:P0;blob `R` = 测试辅助产出的 `RiskParams` 文档字节(2 048 B);`E0` + `{"type":"SetRisk","blob_b64":<列>}`
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_042_set_risk`

| 行 | 输入,`account` | 期望 |
|---|---|---|
| CTP-042.1 | `base64(R)`,`"acct-a"` | `cmd = 0x0003`、`payload_len = 48`、`CmdHdr.account = 1`;`BlobRef{kind = RiskParams, len = 2 048, sha256 = SHA-256(R)}`;`blobs` 拼接等于 `R` |
| CTP-042.2 | `base64(R)`,`"*"` | `CmdHdr.account = 0` |
| CTP-042.3 | 786 433 字节,`"acct-a"` | `InvalidArgs{TooLong}` |
| CTP-042.4 | `"!!"`,`"acct-a"` | `InvalidArgs{BadValue}` |

### CTP-043 Kill 的级别、作用域与参数组合

- 规则:[请求与响应](README.md#请求与响应)、[control-plane 账户语义](../../docs/subsystems/control-plane.md#账户语义)、[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)
- 级别:table
- 里程碑:M2
- 前置:P0;`E0` + `{"type":"Kill", …}`;载荷字段顺序 `scope、level、flatten、cancel_resting、hard`(24 B,`cmd_ver = 2`)
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_043_kill_combinations`

| 行 | 命令字段,`account` | 期望 |
|---|---|---|
| CTP-043.1 | `level "Global"`,缺 | `cmd = 0x0004`、`cmd_ver = 2`、`payload_len = 24`;`scope = {0, 0, 0, 0xFFFF_FFFF, 0}`、`level = 4`、`flatten = 0`、`cancel_resting = 1`、`hard = 0` |
| CTP-043.2 | `level "Account"`,`"acct-a"` | `scope.account = 1`、`level = 3`、`cancel_resting = 1` |
| CTP-043.3 | `level "Venue", venue "bitget-spot"`,`"*"` | `scope = {0, 11, 0, 0xFFFF_FFFF, 0}`、`level = 2` |
| CTP-043.4 | `level "Venue", venue "bitget-spot"`,`"acct-a"` | `scope = {0, 11, 1, 0xFFFF_FFFF, 0}` |
| CTP-043.5 | `level "Venue"`(无 venue),`"*"` | `InvalidArgs{BadCombination}` |
| CTP-043.6 | `level "Venue", venue "*"`,`"*"` | `InvalidArgs{BadCombination}` |
| CTP-043.7 | `level "Global", hard true`,缺 | `hard = 1`、`level = 4` |
| CTP-043.8 | `level "Account", hard true`,`"acct-a"` | `hard = 1`、`level = 3` |
| CTP-043.9 | `level "Venue", venue "bitget-spot", hard true`,`"*"` | `InvalidArgs{BadCombination}` |
| CTP-043.10 | `level "Account", flatten true`,`"acct-a"` | `flatten = 1` |
| CTP-043.11 | `level "Global", flatten true`,缺 | `InvalidArgs{BadCombination}` |
| CTP-043.12 | `level "Venue", venue "bitget-spot", flatten true`,`"acct-a"` | `InvalidArgs{BadCombination}` |
| CTP-043.13 | `level "Account", flatten true, hard true`,`"acct-a"` | `InvalidArgs{BadCombination}` |
| CTP-043.14 | `level "Account", cancel_resting false`,`"acct-a"` | `cancel_resting = 0` |
| CTP-043.15 | `level "Strategy"`,缺 | 模式层错误,`invalid_request`,`detail = UnknownDiscriminant` |
| CTP-043.16 | 无 `level`,缺 | 模式层错误,`invalid_request` |
| CTP-043.17 | `level "Account", hard "true"`(字符串),`"acct-a"` | 模式层错误,`invalid_request` |

### CTP-044 Resume 的作用域与来源编码

- 规则:[请求与响应](README.md#请求与响应)、[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)
- 级别:table
- 里程碑:M2
- 前置:P0;`E0` + `{"type":"Resume", …}`;`source` 的码值按 hft-types `CapSource`
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_044_resume`

| 行 | 命令字段,`account` | 期望 |
|---|---|---|
| CTP-044.1 | `source "Kill"`,`"acct-a"` | `cmd = 0x000A`、`payload_len = 24`;`scope = {0, 0, 1, 0xFFFF_FFFF, 0}`、`source = 1` |
| CTP-044.2 | `source "Kill"`,`"*"` | `scope.account = 0`(解除 `Kill{Global}` 用) |
| CTP-044.3 | `source "Recon", venue "bitget-spot"`,`"acct-a"` | `scope = {0, 11, 1, 0xFFFF_FFFF, 0}`、`source = 3` |
| CTP-044.4 | `source "Halted", strategy "s1-main", group "16777317"`,`"acct-a"` | `scope = {1, 0, 1, 0x0100_0065, 0}`、`source = 8` |
| CTP-044.5 | `source "Broken", group "16777317"`,`"acct-a"` | `scope = {0, 0, 1, 0x0100_0065, 0}`、`source = 5` |
| CTP-044.6 | `source "Delisting", venue "binance-spot", inst "binance-spot:BTCUSDT"`,`"*"` | `scope = {0, 2, 0, 0xFFFF_FFFF, 2}`、`source = 14` |
| CTP-044.7 | `source "ClockBad"`,`"*"` | 规范化成功,`source = 10`(是否可清除由 sequencer 判定) |
| CTP-044.8 | `source "Nope"`,`"acct-a"` | 模式层错误,`invalid_request`,`detail = UnknownDiscriminant` |

### CTP-045 CancelAll、Drain、HedgeFix、WriteOffResidual、SetBalanceBaseline、SimReset 的编码

- 规则:[请求与响应](README.md#请求与响应)、[control-plane 账户语义](../../docs/subsystems/control-plane.md#账户语义)
- 级别:table
- 里程碑:M2
- 前置:P0;每行 `E0` + 列出的命令
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_045_small_commands`

| 行 | 命令,`account` | 期望 |
|---|---|---|
| CTP-045.1 | `CancelAll{venue "bitget-spot", inst "bitget-spot:BTCUSDT"}`,`"acct-a"` | `cmd = 0x000F`、`payload_len = 16`、`scope = {0, 11, 1, 0xFFFF_FFFF, 4}` |
| CTP-045.2 | `CancelAll{}`(venue、inst 省略),`"*"` | `scope = {0, 0, 0, 0xFFFF_FFFF, 0}` |
| CTP-045.3 | `Drain{}`,缺 | `cmd = 0x0010`、`payload_len = 8`、`cancel_resting = 1` |
| CTP-045.4 | `Drain{cancel_resting false}`,缺 | `cancel_resting = 0` |
| CTP-045.5 | `HedgeFix{group "16777317"}`,`"acct-a"` | `cmd = 0x000E`、`payload_len = 16`、`scope = {0, 0, 1, 0x0100_0065, 0}` |
| CTP-045.6 | `HedgeFix{group "*"}`,`"acct-a"` | `InvalidArgs{BadGroup}` |
| CTP-045.7 | `HedgeFix{}`(无 group),`"acct-a"` | `InvalidArgs{BadGroup}`(group 省略即 `*`) |
| CTP-045.8 | `WriteOffResidual{group "16777317"}`,`"acct-a"` | `cmd = 0x001B`、`payload_len = 8`、载荷 `{account 1, group 0x0100_0065}` |
| CTP-045.9 | `WriteOffResidual{group "*"}`,`"acct-a"` | `InvalidArgs{BadGroup}` |
| CTP-045.10 | `SetBalanceBaseline{venue "bitget-spot"}`,`"acct-a"` | `cmd = 0x0017`、`payload_len = 8`、载荷 `{account 1, venue 11}` |
| CTP-045.11 | `SimReset{}`,`"sim-a"` | `cmd = 0x0014`、`payload_len = 8`、载荷 `{account 3}` |
| CTP-045.12 | `SimReset{}`,`"acct-a"` | 规范化成功(`NotSim` 由 gateway 判定,见 hft-ctl) |

### CTP-046 SmokeOrder 的编码与 ttl_ms 缺省值、边界

- 规则:[请求与响应](README.md#请求与响应)、[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)
- 级别:table
- 里程碑:M2
- 前置:P0;`E0` + `{"type":"SmokeOrder","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","side":"Buy", …}`,`account "acct-a"`;`ttl_ms` 范围 1..=60 000,缺省 10 000(control-plane)
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_046_smoke_order`

| 行 | `ttl_ms` / 其他 | 期望 |
|---|---|---|
| CTP-046.1 | 省略 | `cmd = 0x001A`、`payload_len = 16`、载荷 `{account 1, venue 11, inst 4, side 1, ttl_ms 10 000}` |
| CTP-046.2 | `"1"` | `ttl_ms = 1` |
| CTP-046.3 | `"0"` | `InvalidArgs{BadValue}` |
| CTP-046.4 | `"60000"` | `ttl_ms = 60 000` |
| CTP-046.5 | `"60001"` | `InvalidArgs{BadValue}` |
| CTP-046.6 | `"59999"` | `ttl_ms = 59 999` |
| CTP-046.7 | `"4294967296"` | `InvalidArgs{BadValue}` |
| CTP-046.8 | `side "Sell"` | `side = 2` |
| CTP-046.9 | `side "Hold"` | 模式层错误,`invalid_request`,`detail = UnknownDiscriminant` |
| CTP-046.10 | `inst "binance-usdm:BTCUSDT"`(不属于 `bitget-spot`) | 规范化成功(归属由 gateway 判定,见 hft-ctl) |

### CTP-047 AdoptExchange 按 item 编码

- 规则:[请求与响应](README.md#请求与响应)、[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)
- 级别:table
- 里程碑:M2
- 前置:P0;`E0` + `{"type":"AdoptExchange","venue":"binance-usdm","report_id":"7", …}`,`account "acct-a"`;载荷 `cmd_ver = 2`(32 B)
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_047_adopt_exchange`

| 行 | 命令字段 | 期望 |
|---|---|---|
| CTP-047.1 | `item "Position", inst "binance-usdm:BTCUSDT"` | `cmd = 0x0013`、`cmd_ver = 2`、`payload_len = 32`;`{account 1, venue 1, inst 1, report_id 7, item 0, asset 0, group 0, entry_px 0}` |
| CTP-047.2 | `venue "bitget-spot", item "Balance", asset "BTC", group "16777317"` | `venue = 11`、`item = 1`、`asset = 2`、`group = 0x0100_0065`、`inst = 0` |
| CTP-047.3 | `venue "bitget-spot", item "Balance", asset "BTC"` | `group = 0`(缺省 unassigned) |
| CTP-047.4 | `item "Position"`(无 inst) | 模式层错误,`invalid_request` |
| CTP-047.5 | `venue "bitget-spot", item "Balance"`(无 asset) | 模式层错误,`invalid_request` |
| CTP-047.6 | `item "Position", inst "binance-usdm:BTCUSDT", report_id "x7"` | 模式层错误,`invalid_request` |

### CTP-048 VenueNotice 按 kind 编码

- 规则:[请求与响应](README.md#请求与响应)
- 级别:table
- 里程碑:M2
- 前置:P0;`E0` + `{"type":"VenueNotice","venue":"binance-spot", …}`
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_048_venue_notice`

| 行 | 命令字段,`account` | 期望 |
|---|---|---|
| CTP-048.1 | `inst "binance-spot:BTCUSDT", kind "Delisting", at_ns "1790100000000000000"`,缺 | `cmd = 0x0016`、`payload_len = 24`;`{venue 2, kind 1, inst 2, a 1790100000000000000, b 0}` |
| CTP-048.2 | 同上去掉 `inst` | `inst = 0` |
| CTP-048.3 | `kind "FundingInterval", new_s "14400"`,`"*"` | `kind = 2`、`a = 14400`、`b = 0` |
| CTP-048.4 | `kind "Maintenance", from_ns "1790100000000000000", to_ns "1790103600000000000"`,`"*"` | `kind = 3`、`a = 1790100000000000000`、`b = 1790103600000000000` |
| CTP-048.5 | `kind "Halt"`,`"*"` | 模式层错误,`invalid_request`,`detail = UnknownDiscriminant` |
| CTP-048.6 | `kind "Delisting"`(无 `at_ns`),缺 | 模式层错误,`invalid_request` |

### CTP-049 SimTransfer、RecordTransfer、Annotate 的编码与长度上限

- 规则:[请求与响应](README.md#请求与响应)
- 级别:table
- 里程碑:M2
- 前置:P0
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_049_transfers_and_annotate`

| 行 | 请求(`E0` + 命令),`account` | 期望 |
|---|---|---|
| CTP-049.1 | `SimTransfer{from_venue "binance-usdm", to_venue "bitget-spot", asset "BTC", amount "0.5"}`,`"sim-a"` | `cmd = 0x0015`、`payload_len = 16`;`{account 3, from 1, to 11, asset 2, amount 50 000 000}` |
| CTP-049.2 | `RecordTransfer{from "binance-usdm", to "bitget-spot", asset "USDT", amount "100", ext_ref "TX123"}`,`"acct-a"` | `cmd = 0x0011`、`payload_len = 80`;`{account 1, from 1, to 11, asset 1, amount 100 000 000, reference = "TX123" 后补 59 个 0}` |
| CTP-049.3 | 同 049.2,`ext_ref` 为 64 字节 | 成功,`reference` 64 字节无补零 |
| CTP-049.4 | 同 049.2,`ext_ref` 为 65 字节 | `InvalidArgs{TooLong}` |
| CTP-049.5 | `Annotate{kind "OpsNote", text "hi"}`,`"*"` | `cmd = 0x0012`、`payload_len = 296`;`{kind 1, text_len 2, digest 全 0, text = "hi" 后补 254 个 0}` |
| CTP-049.6 | `Annotate{kind "LocalWrite", text "x", digest <64 个十六进制字符 "ab…">}`,`"*"` | `kind = 2`、`digest` 为对应 32 字节 |
| CTP-049.7 | `Annotate{kind "Incident", text <256 字节 UTF-8>}`,`"*"` | `kind = 3`、`text_len = 256` |
| CTP-049.8 | `Annotate{kind "OpsNote", text <257 字节>}`,`"*"` | `InvalidArgs{TooLong}` |
| CTP-049.9 | `Annotate{kind "OpsNote", text "x", digest "abc"}`,`"*"` | `InvalidArgs{BadValue}` |

### CTP-050 ReplaceRefdata 的编码

- 规则:[请求与响应](README.md#请求与响应)、[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)
- 级别:table
- 里程碑:M2
- 前置:P0;blob `D` = hft-cfg 测试辅助产出的 refdata blob(30 000 B)
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_050_replace_refdata`

| 行 | 命令字段,`account` | 期望 |
|---|---|---|
| CTP-050.1 | `blob_b64 base64(D), version "8"`,`"acct-a"` | `cmd = 0x0009`、`payload_len = 48`;`BlobRef{kind = Refdata, len = 30 000, version = 8, sha256 = SHA-256(D)}`;`blobs` 拼接等于 `D` |
| CTP-050.2 | 无 `version`,`"acct-a"` | 模式层错误,`invalid_request` |
| CTP-050.3 | 786 433 字节,`"acct-a"` | `InvalidArgs{TooLong}` |
| CTP-050.4 | 同 050.1,`"*"` | `CmdHdr.account = 0` |

### CTP-051 StrategyCmd 的编码与载荷上限

- 规则:[请求与响应](README.md#请求与响应)
- 级别:table
- 里程碑:M2
- 前置:P0;`E0` + `{"type":"StrategyCmd","strategy":"s1-main","group":"16777317","reduce_only":false,"payload_b64":<列>}`,`account "acct-a"`;`STRATEGY_CMD_MAX_PAYLOAD = 256`
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_051_strategy_cmd`

| 行 | 输入 | 期望 |
|---|---|---|
| CTP-051.1 | 载荷 `01 02 03` | `cmd = 0x0018`、`payload_len = 264`;`{instance 1, reduce_only 0, len 3, group 0x0100_0065, payload = 01 02 03 后补 253 个 0}`;`CmdHdr.account = 1` |
| CTP-051.2 | 载荷 256 字节 | `len = 256` |
| CTP-051.3 | 载荷 255 字节 | `len = 255` |
| CTP-051.4 | 载荷 257 字节 | `InvalidArgs{TooLong}` |
| CTP-051.5 | 载荷为空 | `len = 0`、`payload` 全 0 |
| CTP-051.6 | `reduce_only true`,载荷 `01` | `reduce_only = 1` |
| CTP-051.7 | `account "acct-b"` | `InvalidArgs{AccountBound}` |
| CTP-051.8 | `group "33554533"`(属 `s2-main`) | `InvalidArgs{BadGroup}` |
| CTP-051.9 | `strategy "nope"` | `InvalidArgs{UnknownName}` |

### CTP-052 InjectWarmup 的编码与载荷上限

- 规则:[请求与响应](README.md#请求与响应)
- 级别:table
- 里程碑:M2
- 前置:P0;`E0` + `{"type":"InjectWarmup","strategy":"s2-main","local_kind":"3","chunk_no":"0","last":false,"payload_b64":<列>}`,无 `account`;`WARMUP_MAX_PAYLOAD = 4096`
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_052_inject_warmup`

| 行 | 输入 | 期望 |
|---|---|---|
| CTP-052.1 | 载荷 4 096 字节 | `cmd = 0x0019`、`payload_len = 4 112`(16 B 前缀 + 4 096);`{instance 2, last 0, local_kind 3, chunk_no 0, len 4 096}`;`CmdHdr.account = 0` |
| CTP-052.2 | 载荷 4 097 字节 | `InvalidArgs{TooLong}` |
| CTP-052.3 | 载荷 4 095 字节 | `len = 4 095`、`payload_len = 4 112`(按 8 B 补零) |
| CTP-052.4 | 载荷 1 字节 | `payload_len = 24` |
| CTP-052.5 | `last true` | `last = 1` |
| CTP-052.6 | `local_kind "65535"` | `local_kind = 65 535` |
| CTP-052.7 | `local_kind "65536"` | `InvalidArgs{BadValue}` |
| CTP-052.8 | `chunk_no "4294967295"` | `chunk_no = 4 294 967 295` |
| CTP-052.9 | `chunk_no "4294967296"` | `InvalidArgs{BadValue}` |
| CTP-052.10 | `strategy "nope"` | `InvalidArgs{UnknownName}` |

### CTP-053 ReconcileReport 整组规范化为连续记录

- 规则:[请求与响应](README.md#请求与响应)、[control-plane 非命令输入](../../docs/subsystems/control-plane.md#非命令输入reconcilereport)
- 级别:table
- 里程碑:M2
- 前置:P0;`issuer "reconciler"`;`E0` + `{"type":"ReconcileReport","report_id":"7","items":[…]}`,无 `account`
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_053_reconcile_report`

| 行 | `items` | 期望 |
|---|---|---|
| CTP-053.1 | 3 条:`{venue "binance-usdm", account "acct-a", item "Position", severity "Ignore", inst "binance-usdm:BTCUSDT", engine_qty "0", exch_qty "0", view_seq "100"}`、同样一条 `venue "bitget-spot", item "Balance", asset "BTC"`、一条 `item "Summary"` | 3 条 `CtlRecord{kind = ReconcileReport}`,依序 `idx = 0, 1, 2`、`count = 3`、`report_id = 7`;第 1 条 `item = 1, severity = 1, inst = 1, view_seq = 100`;不产生 `ControlCmd`,没有 `cmd_hash` |
| CTP-053.2 | 1 条 `item "Position"`,`sflags ["AUTO_EXTERNAL"]` | `sflags = 8` |
| CTP-053.3 | 1 条 `item "Summary"`,`sflags ["USERSTREAM_DOWN","TRUNCATED"]` | `sflags = 3` |
| CTP-053.4 | 1 条 `severity "Severe"` | 模式层错误,`invalid_request`,`detail = UnknownDiscriminant` |
| CTP-053.5 | 1 条 `account "nope"` | `InvalidArgs{UnknownName}` |

### CTP-054 CmdHdr 各字段的来源

- 规则:[实现说明](README.md#实现说明)、[hft-event 控制命令编码](../hft-event/README.md#控制命令编码)
- 级别:table
- 里程碑:M2
- 前置:P0;请求 = `E0` + `Resume{source "Kill"}`,`account "acct-a"`,另按行改动;断言 `Normalized.record` 的 `CmdHdr`
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_054_cmd_hdr_fields`

| 行 | 改动 | 期望 |
|---|---|---|
| CTP-054.1 | 无 | `cmd_id` = UUID 的 16 字节(RFC 4122 字节序:`00 00 00 00 00 00 40 00 80 00 00 00 00 00 00 01`);`issuer` = `"cli:alice"` 后补 15 个 0;`issued_at_ns`、`expires_at_ns` 原样;`expires_tsc = 0`、`peer_uid = 0`、`role = 0`;`cmd = 0x000A`、`cmd_ver = 1`、`account = 1`、`confirmed = 0`、`hflags = 0`、`payload_len = 24`、`base_version = 0`、`expect_state_version = 0` |
| CTP-054.2 | `preview = true` | `hflags = 1`、`confirmed = 0` |
| CTP-054.3 | `confirmed = true, expect_state_version = 4182` | `confirmed = 1`、`expect_state_version = 4182`、`hflags = 0` |
| CTP-054.4 | `confirmed = false, expect_state_version = 9` | `expect_state_version = 9`(原样透传) |
| CTP-054.5 | 换成 `Kill{level "Account"}` | `cmd_ver = 2` |
| CTP-054.6 | 换成 `AdoptExchange{…Position…}` | `cmd_ver = 2` |
| CTP-054.7 | 每个 core 命令各一次 | `payload_len` 等于 hft-event 给出的该命令载荷大小(`InjectWarmup` 为 16 + `len` 向上取 8) |

### CTP-055 大参数切成 BlobPart,拼接与 sha256 与原 blob 一致

- 规则:[实现说明](README.md#实现说明)
- 级别:prop
- 里程碑:M2
- 性质:`SetParams`/`SetRisk`/`ReplaceRefdata` 的 `blobs` 按序拼接等于解码后的 blob;`blobs.len() ≤ 64`;`BlobRef.len` 等于 blob 长度、`BlobRef.sha256` 等于 SHA-256(blob);命令载荷只含 `BlobRef`,不含 blob 内容
- 生成:三种命令之一;blob 长度 16..=786 432(SPB1 由测试辅助生成合法头部,其余为随机字节)
- 参考:直接对 blob 计算的 SHA-256 与长度
- 例数:PR 200
- 实现:`crates/hft-ctl-proto/tests/prop.rs::ctp_055_blob_parts_roundtrip`

### CTP-056 多处出错时按步骤顺序报第一个

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M2
- 前置:P0;每行同时含两处错误
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_056_first_error_by_step`

| 行 | 请求 | 期望 |
|---|---|---|
| CTP-056.1 | `HedgeFix{group "16777317"}` 缺 `account`(②),且 group 的高 8 位改为不存在的实例 4(③) | `InvalidArgs{AccountRequired}` |
| CTP-056.2 | `SimTransfer{asset "XYZ"(③), amount "0.000000001"(④)}`,`"sim-a"` | `InvalidArgs{UnknownName}` |
| CTP-056.3 | `SetParams{strategy "s1-main"}`,`account "*"`(②),blob 头 `account 2`(⑤) | `InvalidArgs{AccountRequired}` |
| CTP-056.4 | `SetParams`,`"acct-a"`,`blob_b64` 解码 786 433 字节且头 `strategy 2` | `InvalidArgs{TooLong}`(长度先于头部核对) |
| CTP-056.5 | `StrategyCmd{strategy "s1-main", payload 257 B}`,`"acct-b"` | `InvalidArgs{AccountBound}`(③ 先于 ⑤ 的长度) |

### CTP-057 normalize 对同一输入得到逐字节相同的结果

- 规则:[概要](README.md#概要)、[实现说明](README.md#实现说明)
- 级别:prop
- 里程碑:M2
- 性质:同一 `(CmdRequest, RefData)` 调用两次(中间穿插对其他请求的调用、改变进程环境变量与系统时间)得到逐字节相同的 `record`、`cmd_hash` 与 `blobs`;失败时得到相同的 `NormErr`
- 生成:P0 refdata;从全部 core 命令的合法与非法请求生成器中抽取(字段随机取合法名、`*`、未知名与越界数值)
- 参考:第一次调用的结果
- 例数:PR 1 000
- 实现:`crates/hft-ctl-proto/tests/prop.rs::ctp_057_normalize_deterministic`

### CTP-058 规范化后 CmdHdr.account 恒等于载荷中的账户

- 规则:[hft-event 控制命令编码](../hft-event/README.md#控制命令编码)、[control-plane 账户语义](../../docs/subsystems/control-plane.md#账户语义)
- 级别:prop
- 里程碑:M2
- 性质:对每个规范化成功的命令,带 `CmdScope` 的 `CmdScope.account == CmdHdr.account`;带 `account` 字段的载荷(`AdoptExchange`、`SimReset`、`SimTransfer`、`SetBalanceBaseline`、`RecordTransfer`、`SmokeOrder`、`WriteOffResidual`)该字段 `== CmdHdr.account`;`InjectWarmup` 的 `CmdHdr.account == 0`
- 生成:同 CTP-057,只取规范化成功的样本
- 参考:按命令种类读出的载荷账户
- 例数:PR 1 000
- 实现:`crates/hft-ctl-proto/tests/prop.rs::ctp_058_hdr_account_matches_payload`

### CTP-070 cmd_hash 是账户、命令种类、载荷版本与载荷的 SHA-256

- 规则:[请求与响应](README.md#请求与响应)
- 级别:unit
- 里程碑:M2
- 前置:P0;请求 = `E0` + `SetMode{strategy "s1-main", group "16777317", mode "Live"}`,`account "acct-a"`
- 步骤:
  1. `normalize`,取 `Normalized.cmd_hash` 与 `record` 的载荷字节 `P`(24 B)
  2. 测试自行计算 `SHA-256(01 ‖ 01 00 ‖ 01 00 ‖ P)`
- 期望:
  - 两者逐字节相等;`CmdHdr.cmd_hash` 字段(偏移 80)与 `Normalized.cmd_hash` 相同
- 实现:`crates/hft-ctl-proto/tests/hash.rs::ctp_070_cmd_hash_definition`

### CTP-071 信封字段不进 cmd_hash,预览与确认得到同一值

- 规则:[请求与响应](README.md#请求与响应)、[二次确认](README.md#二次确认)
- 级别:table
- 里程碑:M2
- 前置:P0;基准请求 `Q` = `E0` + `Kill{level "Account"}`,`account "acct-a"`,`issuer "tg:1001"`、`chat_type "private"`、`preview = true`;每行只改一处,期望都是"`cmd_hash` 与 `Q` 的逐字节相同"
- 实现:`crates/hft-ctl-proto/tests/hash.rs::ctp_071_envelope_not_hashed`

| 行 | 改动 | 期望 |
|---|---|---|
| CTP-071.1 | 新 `cmd_id` | 与 `Q` 相同 |
| CTP-071.2 | `issuer "tg:1002"` | 与 `Q` 相同 |
| CTP-071.3 | `issued_at_ns`、`expires_at_ns` 各加 20 s | 与 `Q` 相同 |
| CTP-071.4 | `preview = false, confirmed = true` | 与 `Q` 相同 |
| CTP-071.5 | `expect_state_version = 77` | 与 `Q` 相同 |
| CTP-071.6 | `view_account "acct-b"`、`chat_type "group"` | 与 `Q` 相同 |
| CTP-071.7 | 换成 `SetParams`(`Q` 的命令改为 CTP-041.1)并加 `base_version "41"` | 与无 `base_version` 时相同 |

### CTP-072 账户、作用域、命令种类或参数不同则 cmd_hash 不同

- 规则:[请求与响应](README.md#请求与响应)、[二次确认](README.md#二次确认)
- 级别:table
- 里程碑:M2
- 前置:P0;每行两条请求,其余字段相同
- 实现:`crates/hft-ctl-proto/tests/hash.rs::ctp_072_hash_distinguishes`

| 行 | 请求 A / 请求 B | 期望 |
|---|---|---|
| CTP-072.1 | `Kill{level Global}` / `Kill{level Account}` `acct-a` | 两个 `cmd_hash` 不同 |
| CTP-072.2 | `Kill{level Account}` `acct-a` / 同上 `acct-b` | 不同 |
| CTP-072.3 | `Kill{level Account}` `acct-a` / 同上加 `hard true` | 不同 |
| CTP-072.4 | `Kill{level Account}` `acct-a` / 同上加 `cancel_resting false` | 不同 |
| CTP-072.5 | 同一 16 B `CmdScope` 载荷与同一账户,`cmd = 0x000E`(`HedgeFix`)/ `cmd = 0x000F`(`CancelAll`),直接调 `cmd_hash` 的计算函数 | 不同(`cmd` 不同) |
| CTP-072.6 | `SetParams` blob `B` / blob `B` 的 body 末字节改 1 | 不同(`BlobRef.sha256` 不同) |
| CTP-072.7 | `InjectWarmup{chunk_no "0"}` / `{chunk_no "1"}`,载荷相同 | 不同 |

### CTP-080 check_confirm_fields 只放过带非 0 版本的确认与 PreviewBound

- 规则:[二次确认](README.md#二次确认)、[请求与响应](README.md#请求与响应)
- 级别:table
- 里程碑:M2
- 前置:P0;输入为 `(ConfirmRule, CmdRequest 的 preview / confirmed / expect_state_version)`
- 实现:`crates/hft-ctl-proto/tests/confirm.rs::ctp_080_check_confirm_fields`

| 行 | 规则 | `preview` / `confirmed` / `expect_state_version` | 期望 |
|---|---|---|---|
| CTP-080.1 | `Always` | false / true / `Some(5)` | `Ok(())` |
| CTP-080.2 | `Always` | false / true / `None` | `Err`,渲染为 `invalid_request` |
| CTP-080.3 | `Always` | false / true / `Some(0)` | `Err`,渲染为 `invalid_request`(0 在 `CmdHdr` 中表示不检查) |
| CTP-080.4 | `Always` | false / true / `Some(1)` | `Ok(())` |
| CTP-080.5 | `IfLoosening` | false / true / `None` | `Err` |
| CTP-080.6 | `Never` | false / true / `None` | `Err`(规则对 `PreviewBound` 以外的全部命令生效) |
| CTP-080.7 | `Never` | false / false / `None` | `Ok(())` |
| CTP-080.8 | `Always` | false / false / `None` | `Ok(())`(未确认由 sequencer 回 `NeedsConfirm`) |
| CTP-080.9 | `Always` | true / false / `None` | `Ok(())` |
| CTP-080.10 | `PreviewBound` | false / true / `None` | `Ok(())` |
| CTP-080.11 | `PreviewBound` | false / true / `Some(7)` | `Ok(())`(原样透传,sequencer 忽略) |
| CTP-080.12 | `PreviewBound` | false / true / `Some(0)` | `Ok(())` |

### CTP-081 needs_confirm_static 按命令与有效角色给出确认规则

- 规则:[二次确认](README.md#二次确认)、[权限表](README.md#权限表)
- 级别:table
- 里程碑:M2
- 前置:P0;`role` 为有效角色(进程主体为无角色),`principal` 同基线记号
- 实现:`crates/hft-ctl-proto/tests/confirm.rs::ctp_081_needs_confirm_static`

| 行 | `role` / `principal` / 命令 | 期望 |
|---|---|---|
| CTP-081.1 | Viewer / `tgV` / `Kill{Global}` 默认参数 | `PreviewBound` |
| CTP-081.2 | Viewer / `tgV` / `Kill{Account(acct-a)}` 默认参数 | `PreviewBound` |
| CTP-081.3 | Viewer / `cliV` / `Kill{Global}` 默认参数 | `PreviewBound` |
| CTP-081.4 | Trader / `tgT` / `Kill{Global}` 默认参数 | `Never` |
| CTP-081.5 | Admin / `cliA` / `Kill{Account(acct-a)}` 默认参数 | `Never` |
| CTP-081.6 | Trader / `tgT` / `Kill{Account(acct-a), flatten}` | `Always` |
| CTP-081.7 | Trader / `tgT` / `Kill{Account(acct-a), hard}` | `Never` |
| CTP-081.8 | Trader / `tgT` / `Kill{Venue(binance-spot)}` `cancel_resting = false` | `Never` |
| CTP-081.9 | 无 / `bot` / `Kill{Global}` 默认参数 | `Never` |
| CTP-081.10 | 无 / `sysd` / `Kill{Account(acct-a)}` 默认参数 | `Never` |
| CTP-081.11 | Trader / `tgT` / `SetMode→Live` | `Always` |
| CTP-081.12 | Trader / `tgT` / `SetMode→CloseOnly` | `Never` |
| CTP-081.13 | 无 / `pipe` / `SetMode→Shadow`(group `g1`) | `Never` |
| CTP-081.14 | Trader / `tgT` / `Resume{Kill}` | `Always` |
| CTP-081.15 | Trader / `cliT` / `SetParams` | `IfLoosening` |
| CTP-081.16 | 无 / `pipe` / `SetParams` | `IfLoosening` |
| CTP-081.17 | Admin / `cliA` / `SetRisk` | `IfLoosening` |
| CTP-081.18 | Admin / `cliA` / `ReplaceRefdata` | `Always` |
| CTP-081.19 | Trader / `tgT` / `CancelAll` | `Never` |
| CTP-081.20 | 无 / `sysd` / `Drain` | `Never` |
| CTP-081.21 | Trader / `tgT` / `HedgeFix` | `Never` |
| CTP-081.22 | Admin / `tgA` / `WriteOffResidual` | `Always` |
| CTP-081.23 | Admin / `tgA` / `SmokeOrder` | `Always` |
| CTP-081.24 | Trader / `tgT` / `StrategyCmd{reduce_only false}` | `Always` |
| CTP-081.25 | Trader / `tgT` / `StrategyCmd{reduce_only true}` | `Never` |
| CTP-081.26 | 无 / `pipe` / `InjectWarmup` | `Never` |
| CTP-081.27 | Trader / `tgT` / `AdoptExchange` | `Always` |
| CTP-081.28 | Trader / `tgT` / `SetBalanceBaseline` | `Always` |
| CTP-081.29 | Trader / `tgT` / `VenueNotice{Delisting}` | `Never` |
| CTP-081.30 | Trader / `tgT` / `VenueNotice{Maintenance}` | `Never` |
| CTP-081.31 | Trader / `tgT` / `SimReset` | `Always` |
| CTP-081.32 | Trader / `tgT` / `SimTransfer` | `Never` |
| CTP-081.33 | Trader / `tgT` / `RecordTransfer` | `Never` |
| CTP-081.34 | Viewer / `tgV` / `Annotate{OpsNote}` | `Never` |
| CTP-081.35 | Viewer / `tgV` / `GetSchema` | `Never` |

### CTP-082 只有 Viewer 的默认参数 Kill 得到 PreviewBound

- 规则:[二次确认](README.md#二次确认)、[权限表](README.md#权限表)
- 级别:prop
- 里程碑:M2
- 性质:`needs_confirm_static(role, principal, cmd) == PreviewBound` 当且仅当 `role = Viewer` 且 `cmd` 是非 HARD、不带 `flatten`、`cancel_resting = true` 的 `Kill{Global}` 或 `Kill{Account}`
- 生成:role ∈ {Viewer, Trader, Admin, 无};principal 取基线 13 种;cmd 取全部 core 命令变体与全部 `Kill` 参数组合(level × hard × flatten × cancel_resting)
- 参考:上述判定式
- 例数:PR 2 000
- 实现:`crates/hft-ctl-proto/tests/confirm.rs::ctp_082_preview_bound_only_viewer_kill`

### CTP-090 class_of 按命令与目标值静态分类

- 规则:[权限表](README.md#权限表)、[使用本组件](README.md#使用本组件)、[control-plane 类别](../../docs/subsystems/control-plane.md#类别)
- 级别:table
- 里程碑:M2
- 前置:P0
- 实现:`crates/hft-ctl-proto/tests/perms.rs::ctp_090_class_of`

| 行 | 命令 | 期望 |
|---|---|---|
| CTP-090.1 | `GetSchema` | `ReadOnly` |
| CTP-090.2 | `Kill{Global}` 默认参数 | `Downgrade` |
| CTP-090.3 | `Kill{Account, hard}` | `Downgrade` |
| CTP-090.4 | `Kill{Venue}` `cancel_resting = false` | `Downgrade` |
| CTP-090.5 | `Kill{Account, flatten}` | `Upgrade` |
| CTP-090.6 | `SetMode→Live` | `Upgrade` |
| CTP-090.7 | `SetMode→CloseOnly` | `Downgrade` |
| CTP-090.8 | `SetMode→Shadow` | `Downgrade` |
| CTP-090.9 | `Resume{Kill}` | `Upgrade` |
| CTP-090.10 | `SetParams` | `Neutral` |
| CTP-090.11 | `SetRisk` | `Neutral` |
| CTP-090.12 | `ReplaceRefdata` | `Neutral` |
| CTP-090.13 | `CancelAll` | `Downgrade` |
| CTP-090.14 | `Drain` | `Downgrade` |
| CTP-090.15 | `HedgeFix` | `Neutral` |
| CTP-090.16 | `WriteOffResidual` | `Upgrade` |
| CTP-090.17 | `SmokeOrder` | `Upgrade` |
| CTP-090.18 | `StrategyCmd{reduce_only false}` | `Upgrade` |
| CTP-090.19 | `StrategyCmd{reduce_only true}` | `Neutral` |
| CTP-090.20 | `InjectWarmup` | `Neutral` |
| CTP-090.21 | `AdoptExchange` | `Upgrade` |
| CTP-090.22 | `SetBalanceBaseline` | `Upgrade` |
| CTP-090.23 | `VenueNotice{Delisting}` | `Downgrade` |
| CTP-090.24 | `VenueNotice{FundingInterval}` | `Neutral` |
| CTP-090.25 | `VenueNotice{Maintenance}` | `Neutral` |
| CTP-090.26 | `SimReset` | `Neutral` |
| CTP-090.27 | `SimTransfer` | `Neutral` |
| CTP-090.28 | `RecordTransfer` | `Neutral` |
| CTP-090.29 | `Annotate{OpsNote}` / `{LocalWrite}` / `{Incident}` | 均为 `Neutral` |
| CTP-090.30 | `ReconcileReport` | `Neutral` |
| CTP-090.31 | `SimFault{RejectNew}` / `{Off}` | 均为 `Neutral` |
| CTP-090.31 | `RotateConn` / `Resubscribe` / `SnapshotNow` | 均为 `Neutral` |

### CTP-091 角色码与角色集合的位

- 规则:[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:无参数
- 实现:`crates/hft-ctl-proto/tests/perms.rs::ctp_091_role_codes`

| 行 | 输入 | 期望 |
|---|---|---|
| CTP-091.1 | `Role::Viewer as u8` | `1`;`RoleSet` 位 `0b0001` |
| CTP-091.2 | `Role::Trader as u8` | `2`;`RoleSet` 位 `0b0010` |
| CTP-091.3 | `Role::Admin as u8` | `4`;`RoleSet` 位 `0b1000` |
| CTP-091.4 | `Role::try_from(3)` | `Err`(退役码,永不复用) |
| CTP-091.5 | `Role::try_from(0)` | `Err`(0 只表示进程主体,不是角色) |
| CTP-091.6 | `Role::try_from(5)` | `Err` |
| CTP-091.7 | `RoleSet::from_bits(0b0100)` | `Err`(位 2 对应退役码 3) |

### CTP-092 主体表的 uid、issuer 形式与上限

- 规则:[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:读 `perms.rs` 导出的主体表常量
- 实现:`crates/hft-ctl-proto/tests/perms.rs::ctp_092_principal_table`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-092.1 | ops_bot 代表 Telegram 用户 | uid `hft-opsbot`,issuer 形式 `tg:<数字>`,上限 = Viewer + Trader + Admin 去掉"仅 `cli:<login>`"的命令 |
| CTP-092.2 | ops_bot 自身 | uid `hft-opsbot`,issuer `ops_bot`,允许集合 = {默认参数 `Kill{Global}`、默认参数 `Kill{Account}`、`Annotate{OpsNote}`} |
| CTP-092.3 | 运维 CLI | uid = 登录用户,issuer 形式 `cli:<login>`,上限 = 个人配置的角色 |
| CTP-092.4 | reconciler | uid `hft-recon`,issuer `reconciler`,允许集合 = {`ReconcileReport`} |
| CTP-092.5 | pipeline | uid `hft-pipeline`,issuer `pipeline`,允许集合 = {`SetParams`、group 级 `SetMode→Shadow`、group 级 `SetMode→CloseOnly`、`InjectWarmup`、`GetSchema`} |
| CTP-092.6 | hft-core 自身 | 无 uid(不经 socket),issuer `cli:systemd`,允许集合 = {`CancelAll`、`Drain`、默认参数 `Kill{Global}`、默认参数 `Kill{Account}`} |
| CTP-092.7 | watchtower、treasury | 主体表中没有它们的条目 |

### CTP-093 README 权限表与 perms.rs 由 doc-check 逐行比对

- 规则:[权限表](README.md#权限表)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M2
- 前置:`perms.rs` 的全部 `const PermRow`;README 的副本放在临时目录
- 步骤:
  1. 对未改动的副本运行 doc-check 的权限表比对
  2. 把副本中 `SmokeOrder` 行的 Trader 列由 `—` 改为 `✓` 后再运行
  3. 删去副本中 `HedgeFix` 一行后再运行
- 期望:
  - 步骤 1 通过,渲染出的 Markdown 行数等于 `PERM_ROWS` 与 `OPTIONAL_ROWS` 的行数之和(optional 行未实现时只在不参与 `allowed` 判定的 `OPTIONAL_ROWS` 中)
  - 步骤 2、3 各失败一次,错误信息分别含 `SmokeOrder` 与 `HedgeFix`
- 实现:`tools/xtask/tests/doc_check_perms.rs::ctp_093_perm_table_matches_code`

### CTP-094 class_of 与 allowed 不读任何运行状态

- 规则:[实现说明](README.md#实现说明)、[概要](README.md#概要)
- 级别:prop
- 里程碑:M2
- 性质:任意调用序列中,同一输入的 `class_of`、`allowed`、`needs_confirm_static` 结果相同,与之前调用过什么、调用次数、线程无关
- 生成:随机交错的 1–200 次调用,输入取自 CTP-082 的生成域
- 参考:对每个输入单独调用一次的结果
- 例数:PR 500
- 实现:`crates/hft-ctl-proto/tests/prop.rs::ctp_094_perms_stateless`

### CTP-095 模拟账户上的命令与实盘账户权限相同

- 规则:[权限表](README.md#权限表)
- 级别:prop
- 里程碑:M2
- 性质:把命令中的账户 `acct-a` 换成 `sim-a`(其余相同),`allowed`、`class_of`、`needs_confirm_static` 的结果不变
- 生成:principal 取基线 13 种;cmd 取带账户的全部 core 命令变体
- 参考:`acct-a` 版本的结果
- 例数:PR 500
- 实现:`crates/hft-ctl-proto/tests/prop.rs::ctp_095_sim_account_same_perms`

### CTP-100 GetSchema 对全部角色与 pipeline 放行,群聊也放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `GetSchema{strategy_kind "probe-taker"}`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_100_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-100.1 | `tgV`(Viewer,私聊) | 放行 |
| CTP-100.2 | `tgVg`(Viewer,群聊) | 放行 |
| CTP-100.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-100.4 | `tgTg`(Trader,群聊) | 放行 |
| CTP-100.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-100.6 | `tgAg`(Admin,群聊) | 放行 |
| CTP-100.7 | `cliV`(Viewer) | 放行 |
| CTP-100.8 | `cliT`(Trader) | 放行 |
| CTP-100.9 | `cliA`(Admin) | 放行 |
| CTP-100.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-100.11 | `pipe`(`pipeline`) | 放行 |
| CTP-100.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-100.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-101 默认参数的 Kill{Global}:三种角色、群聊、ops_bot 自身与 cli:systemd 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = 默认参数的 `Kill{Global}`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_101_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-101.1 | `tgV`(Viewer,私聊) | 放行 |
| CTP-101.2 | `tgVg`(Viewer,群聊) | 放行 |
| CTP-101.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-101.4 | `tgTg`(Trader,群聊) | 放行 |
| CTP-101.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-101.6 | `tgAg`(Admin,群聊) | 放行 |
| CTP-101.7 | `cliV`(Viewer) | 放行 |
| CTP-101.8 | `cliT`(Trader) | 放行 |
| CTP-101.9 | `cliA`(Admin) | 放行 |
| CTP-101.10 | `bot`(`ops_bot` 自身) | 放行 |
| CTP-101.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-101.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-101.13 | `sysd`(`cli:systemd`) | 放行 |

### CTP-102 默认参数的 Kill{Account}:三种角色、群聊、ops_bot 自身与 cli:systemd 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = 默认参数的 `Kill{Account(acct-a)}`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_102_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-102.1 | `tgV`(Viewer,私聊) | 放行 |
| CTP-102.2 | `tgVg`(Viewer,群聊) | 放行 |
| CTP-102.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-102.4 | `tgTg`(Trader,群聊) | 放行 |
| CTP-102.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-102.6 | `tgAg`(Admin,群聊) | 放行 |
| CTP-102.7 | `cliV`(Viewer) | 放行 |
| CTP-102.8 | `cliT`(Trader) | 放行 |
| CTP-102.9 | `cliA`(Admin) | 放行 |
| CTP-102.10 | `bot`(`ops_bot` 自身) | 放行 |
| CTP-102.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-102.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-102.13 | `sysd`(`cli:systemd`) | 放行 |

### CTP-103 默认参数的 Kill{Venue}:只对 Trader、Admin 放行,Viewer 与进程主体被拒

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = 默认参数的 `Kill{Venue(bitget-spot)}`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_103_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-103.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-103.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-103.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-103.4 | `tgTg`(Trader,群聊) | 放行 |
| CTP-103.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-103.6 | `tgAg`(Admin,群聊) | 放行 |
| CTP-103.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-103.8 | `cliT`(Trader) | 放行 |
| CTP-103.9 | `cliA`(Admin) | 放行 |
| CTP-103.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-103.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-103.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-103.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-104 Kill{Global} 改 cancel_resting = false 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `Kill{Global}` 且 `cancel_resting = false`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_104_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-104.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-104.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-104.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-104.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-104.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-104.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-104.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-104.8 | `cliT`(Trader) | 放行 |
| CTP-104.9 | `cliA`(Admin) | 放行 |
| CTP-104.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-104.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-104.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-104.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-105 Kill{Account} 改 cancel_resting = false 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `Kill{Account(acct-a)}` 且 `cancel_resting = false`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_105_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-105.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-105.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-105.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-105.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-105.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-105.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-105.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-105.8 | `cliT`(Trader) | 放行 |
| CTP-105.9 | `cliA`(Admin) | 放行 |
| CTP-105.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-105.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-105.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-105.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-106 Kill{Venue} 改 cancel_resting = false 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `Kill{Venue(bitget-spot)}` 且 `cancel_resting = false`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_106_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-106.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-106.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-106.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-106.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-106.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-106.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-106.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-106.8 | `cliT`(Trader) | 放行 |
| CTP-106.9 | `cliA`(Admin) | 放行 |
| CTP-106.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-106.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-106.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-106.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-107 Kill{Global, hard} 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `Kill{Global, hard}`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_107_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-107.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-107.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-107.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-107.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-107.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-107.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-107.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-107.8 | `cliT`(Trader) | 放行 |
| CTP-107.9 | `cliA`(Admin) | 放行 |
| CTP-107.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-107.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-107.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-107.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-108 Kill{Account, hard} 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `Kill{Account(acct-a), hard}`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_108_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-108.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-108.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-108.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-108.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-108.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-108.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-108.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-108.8 | `cliT`(Trader) | 放行 |
| CTP-108.9 | `cliA`(Admin) | 放行 |
| CTP-108.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-108.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-108.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-108.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-109 Kill{Account, flatten} 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `Kill{Account(acct-a), flatten}`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_109_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-109.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-109.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-109.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-109.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-109.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-109.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-109.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-109.8 | `cliT`(Trader) | 放行 |
| CTP-109.9 | `cliA`(Admin) | 放行 |
| CTP-109.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-109.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-109.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-109.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-110 group 级 SetMode→CloseOnly 对 Trader、Admin(含群聊)与 pipeline 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `SetMode→CloseOnly`,group `g1`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_110_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-110.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-110.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-110.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-110.4 | `tgTg`(Trader,群聊) | 放行 |
| CTP-110.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-110.6 | `tgAg`(Admin,群聊) | 放行 |
| CTP-110.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-110.8 | `cliT`(Trader) | 放行 |
| CTP-110.9 | `cliA`(Admin) | 放行 |
| CTP-110.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-110.11 | `pipe`(`pipeline`) | 放行 |
| CTP-110.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-110.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-111 group 为 * 的 SetMode→CloseOnly 对 pipeline 不放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `SetMode→CloseOnly`,group `"*"`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`;pipeline 只允许具体 group
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_111_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-111.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-111.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-111.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-111.4 | `tgTg`(Trader,群聊) | 放行 |
| CTP-111.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-111.6 | `tgAg`(Admin,群聊) | 放行 |
| CTP-111.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-111.8 | `cliT`(Trader) | 放行 |
| CTP-111.9 | `cliA`(Admin) | 放行 |
| CTP-111.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-111.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-111.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-111.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-112 group 级 SetMode→Shadow 对私聊与 CLI 的 Trader、Admin 与 pipeline 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `SetMode→Shadow`,group `g1`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_112_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-112.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-112.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-112.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-112.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-112.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-112.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-112.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-112.8 | `cliT`(Trader) | 放行 |
| CTP-112.9 | `cliA`(Admin) | 放行 |
| CTP-112.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-112.11 | `pipe`(`pipeline`) | 放行 |
| CTP-112.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-112.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-113 group 为 * 的 SetMode→Shadow 对 pipeline 不放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `SetMode→Shadow`,group `"*"`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`;pipeline 只允许具体 group
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_113_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-113.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-113.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-113.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-113.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-113.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-113.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-113.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-113.8 | `cliT`(Trader) | 放行 |
| CTP-113.9 | `cliA`(Admin) | 放行 |
| CTP-113.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-113.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-113.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-113.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-114 SetMode→Live 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `SetMode→Live`,group `g1`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_114_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-114.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-114.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-114.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-114.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-114.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-114.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-114.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-114.8 | `cliT`(Trader) | 放行 |
| CTP-114.9 | `cliA`(Admin) | 放行 |
| CTP-114.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-114.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-114.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-114.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-115 Resume 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `Resume{Kill}`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_115_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-115.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-115.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-115.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-115.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-115.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-115.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-115.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-115.8 | `cliT`(Trader) | 放行 |
| CTP-115.9 | `cliA`(Admin) | 放行 |
| CTP-115.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-115.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-115.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-115.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-116 SetParams 只对 CLI 的 Trader、Admin 与 pipeline 放行,Telegram 一律被拒

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `SetParams`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_116_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-116.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-116.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-116.3 | `tgT`(Trader,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-116.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-116.5 | `tgA`(Admin,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-116.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-116.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-116.8 | `cliT`(Trader) | 放行 |
| CTP-116.9 | `cliA`(Admin) | 放行 |
| CTP-116.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-116.11 | `pipe`(`pipeline`) | 放行 |
| CTP-116.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-116.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-117 SetRisk 只对 CLI 的 Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `SetRisk`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_117_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-117.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-117.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-117.3 | `tgT`(Trader,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-117.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-117.5 | `tgA`(Admin,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-117.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-117.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-117.8 | `cliT`(Trader) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-117.9 | `cliA`(Admin) | 放行 |
| CTP-117.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-117.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-117.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-117.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-118 ReplaceRefdata 只对 CLI 的 Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `ReplaceRefdata`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_118_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-118.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-118.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-118.3 | `tgT`(Trader,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-118.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-118.5 | `tgA`(Admin,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-118.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-118.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-118.8 | `cliT`(Trader) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-118.9 | `cliA`(Admin) | 放行 |
| CTP-118.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-118.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-118.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-118.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-119 CancelAll 对私聊与 CLI 的 Trader、Admin 与 cli:systemd 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `CancelAll`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_119_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-119.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-119.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-119.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-119.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-119.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-119.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-119.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-119.8 | `cliT`(Trader) | 放行 |
| CTP-119.9 | `cliA`(Admin) | 放行 |
| CTP-119.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-119.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-119.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-119.13 | `sysd`(`cli:systemd`) | 放行 |

### CTP-120 Drain 对私聊与 CLI 的 Trader、Admin 与 cli:systemd 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `Drain`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_120_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-120.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-120.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-120.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-120.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-120.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-120.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-120.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-120.8 | `cliT`(Trader) | 放行 |
| CTP-120.9 | `cliA`(Admin) | 放行 |
| CTP-120.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-120.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-120.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-120.13 | `sysd`(`cli:systemd`) | 放行 |

### CTP-121 HedgeFix 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `HedgeFix`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_121_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-121.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-121.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-121.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-121.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-121.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-121.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-121.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-121.8 | `cliT`(Trader) | 放行 |
| CTP-121.9 | `cliA`(Admin) | 放行 |
| CTP-121.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-121.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-121.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-121.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-122 WriteOffResidual 只对私聊与 CLI 的 Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `WriteOffResidual`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_122_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-122.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-122.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-122.3 | `tgT`(Trader,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-122.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-122.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-122.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-122.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-122.8 | `cliT`(Trader) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-122.9 | `cliA`(Admin) | 放行 |
| CTP-122.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-122.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-122.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-122.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-123 SmokeOrder 只对私聊与 CLI 的 Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `SmokeOrder`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_123_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-123.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-123.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-123.3 | `tgT`(Trader,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-123.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-123.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-123.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-123.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-123.8 | `cliT`(Trader) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-123.9 | `cliA`(Admin) | 放行 |
| CTP-123.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-123.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-123.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-123.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-124 未声明 reduce_only 的 StrategyCmd 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `StrategyCmd{reduce_only false}`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_124_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-124.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-124.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-124.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-124.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-124.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-124.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-124.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-124.8 | `cliT`(Trader) | 放行 |
| CTP-124.9 | `cliA`(Admin) | 放行 |
| CTP-124.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-124.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-124.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-124.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-125 StrategyCmd{reduce_only} 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `StrategyCmd{reduce_only true}`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_125_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-125.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-125.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-125.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-125.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-125.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-125.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-125.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-125.8 | `cliT`(Trader) | 放行 |
| CTP-125.9 | `cliA`(Admin) | 放行 |
| CTP-125.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-125.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-125.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-125.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-126 InjectWarmup 对私聊与 CLI 的 Trader、Admin 与 pipeline 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `InjectWarmup`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_126_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-126.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-126.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-126.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-126.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-126.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-126.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-126.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-126.8 | `cliT`(Trader) | 放行 |
| CTP-126.9 | `cliA`(Admin) | 放行 |
| CTP-126.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-126.11 | `pipe`(`pipeline`) | 放行 |
| CTP-126.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-126.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-127 AdoptExchange 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `AdoptExchange`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_127_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-127.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-127.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-127.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-127.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-127.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-127.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-127.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-127.8 | `cliT`(Trader) | 放行 |
| CTP-127.9 | `cliA`(Admin) | 放行 |
| CTP-127.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-127.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-127.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-127.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-128 SetBalanceBaseline 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `SetBalanceBaseline`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_128_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-128.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-128.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-128.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-128.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-128.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-128.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-128.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-128.8 | `cliT`(Trader) | 放行 |
| CTP-128.9 | `cliA`(Admin) | 放行 |
| CTP-128.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-128.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-128.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-128.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-129 VenueNotice{Delisting} 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `VenueNotice{Delisting}`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_129_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-129.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-129.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-129.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-129.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-129.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-129.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-129.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-129.8 | `cliT`(Trader) | 放行 |
| CTP-129.9 | `cliA`(Admin) | 放行 |
| CTP-129.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-129.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-129.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-129.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-130 VenueNotice{FundingInterval} 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `VenueNotice{FundingInterval}`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_130_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-130.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-130.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-130.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-130.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-130.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-130.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-130.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-130.8 | `cliT`(Trader) | 放行 |
| CTP-130.9 | `cliA`(Admin) | 放行 |
| CTP-130.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-130.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-130.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-130.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-131 VenueNotice{Maintenance} 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `VenueNotice{Maintenance}`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_131_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-131.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-131.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-131.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-131.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-131.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-131.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-131.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-131.8 | `cliT`(Trader) | 放行 |
| CTP-131.9 | `cliA`(Admin) | 放行 |
| CTP-131.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-131.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-131.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-131.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-132 SimReset 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `SimReset`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_132_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-132.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-132.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-132.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-132.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-132.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-132.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-132.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-132.8 | `cliT`(Trader) | 放行 |
| CTP-132.9 | `cliA`(Admin) | 放行 |
| CTP-132.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-132.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-132.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-132.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-133 SimTransfer 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `SimTransfer`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_133_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-133.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-133.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-133.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-133.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-133.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-133.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-133.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-133.8 | `cliT`(Trader) | 放行 |
| CTP-133.9 | `cliA`(Admin) | 放行 |
| CTP-133.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-133.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-133.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-133.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-134 RecordTransfer 只对私聊与 CLI 的 Trader、Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `RecordTransfer`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_134_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-134.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-134.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-134.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-134.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-134.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-134.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-134.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-134.8 | `cliT`(Trader) | 放行 |
| CTP-134.9 | `cliA`(Admin) | 放行 |
| CTP-134.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-134.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-134.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-134.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-135 Annotate{OpsNote} 对私聊与 CLI 的三种角色与 ops_bot 自身放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `Annotate{OpsNote}`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_135_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-135.1 | `tgV`(Viewer,私聊) | 放行 |
| CTP-135.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-135.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-135.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-135.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-135.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-135.7 | `cliV`(Viewer) | 放行 |
| CTP-135.8 | `cliT`(Trader) | 放行 |
| CTP-135.9 | `cliA`(Admin) | 放行 |
| CTP-135.10 | `bot`(`ops_bot` 自身) | 放行 |
| CTP-135.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-135.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-135.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-136 Annotate{LocalWrite} 对私聊与 CLI 的三种角色放行,ops_bot 自身被拒

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `Annotate{LocalWrite}`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_136_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-136.1 | `tgV`(Viewer,私聊) | 放行 |
| CTP-136.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-136.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-136.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-136.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-136.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-136.7 | `cliV`(Viewer) | 放行 |
| CTP-136.8 | `cliT`(Trader) | 放行 |
| CTP-136.9 | `cliA`(Admin) | 放行 |
| CTP-136.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-136.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-136.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-136.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-137 Annotate{Incident} 只对私聊与 CLI 的 Trader、Admin 放行,Viewer 被拒

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `Annotate{Incident}`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_137_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-137.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-137.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-137.3 | `tgT`(Trader,私聊) | 放行 |
| CTP-137.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-137.5 | `tgA`(Admin,私聊) | 放行 |
| CTP-137.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-137.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-137.8 | `cliT`(Trader) | 放行 |
| CTP-137.9 | `cliA`(Admin) | 放行 |
| CTP-137.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-137.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-137.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-137.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-138 ReconcileReport 只对 reconciler 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `ReconcileReport`(其余字段合法);调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_138_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-138.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-138.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-138.3 | `tgT`(Trader,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-138.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-138.5 | `tgA`(Admin,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-138.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-138.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-138.8 | `cliT`(Trader) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-138.9 | `cliA`(Admin) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-138.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-138.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-138.12 | `recon`(`reconciler`) | 放行 |
| CTP-138.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-139 optional 命令在实现前对任何主体都不放行

- 规则:[权限表](README.md#权限表)、[control-plane 命令集](../../docs/subsystems/control-plane.md#命令集)
- 级别:table
- 里程碑:M2
- 前置:P0;`perms.rs` 不含 optional 行(未实现);每行对列出的命令遍历基线 13 种主体
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_139_optional_forbidden`

| 行 | 命令 | 期望 |
|---|---|---|
| CTP-139.1 | `RotateConn{venue bitget-spot}`,`acct-a` | 13 种主体全部 `Forbidden`(含 `cliA`) |
| CTP-139.2 | `Resubscribe{venue bitget-spot}` | 13 种主体全部 `Forbidden` |
| CTP-139.3 | `SnapshotNow` | 13 种主体全部 `Forbidden` |

### CTP-140 SetMode 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0;各 `.cmd_hash.hex` 为一行 `cmd_hash <64 位小写十六进制>`(行内写明 hash,fixtures 秘密扫描据此放行)
- 输入:`fixtures/ctl/set_mode_live_group.request.json`、`fixtures/ctl/set_mode_closeonly_all.request.json`、`fixtures/ctl/set_mode_shadow_sim.request.json`
- 期望文件:`fixtures/ctl/set_mode_live_group.record.hex`、`fixtures/ctl/set_mode_closeonly_all.record.hex`、`fixtures/ctl/set_mode_shadow_sim.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_140_golden_set_mode`

### CTP-141 SetParams 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/set_params_base.request.json`、`fixtures/ctl/set_params_base_version.request.json`
- 期望文件:`fixtures/ctl/set_params_base.record.hex`、`fixtures/ctl/set_params_base_version.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_141_golden_set_params`

### CTP-142 SetRisk 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/set_risk_account.request.json`、`fixtures/ctl/set_risk_all.request.json`
- 期望文件:`fixtures/ctl/set_risk_account.record.hex`、`fixtures/ctl/set_risk_all.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_142_golden_set_risk`

### CTP-143 Kill 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/kill_global_default.request.json`、`fixtures/ctl/kill_account_default.request.json`、`fixtures/ctl/kill_venue_account.request.json`、`fixtures/ctl/kill_account_hard.request.json`、`fixtures/ctl/kill_global_hard.request.json`、`fixtures/ctl/kill_account_flatten.request.json`、`fixtures/ctl/kill_account_keep_resting.request.json`
- 期望文件:`fixtures/ctl/kill_global_default.record.hex`、`fixtures/ctl/kill_account_default.record.hex`、`fixtures/ctl/kill_venue_account.record.hex`、`fixtures/ctl/kill_account_hard.record.hex`、`fixtures/ctl/kill_global_hard.record.hex`、`fixtures/ctl/kill_account_flatten.record.hex`、`fixtures/ctl/kill_account_keep_resting.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_143_golden_kill`

### CTP-144 Resume 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/resume_kill_account.request.json`、`fixtures/ctl/resume_kill_all.request.json`、`fixtures/ctl/resume_recon_venue.request.json`、`fixtures/ctl/resume_halted_group.request.json`、`fixtures/ctl/resume_broken_group.request.json`、`fixtures/ctl/resume_delisting_inst.request.json`
- 期望文件:`fixtures/ctl/resume_kill_account.record.hex`、`fixtures/ctl/resume_kill_all.record.hex`、`fixtures/ctl/resume_recon_venue.record.hex`、`fixtures/ctl/resume_halted_group.record.hex`、`fixtures/ctl/resume_broken_group.record.hex`、`fixtures/ctl/resume_delisting_inst.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_144_golden_resume`

### CTP-145 CancelAll 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/cancel_all_account_venue_inst.request.json`、`fixtures/ctl/cancel_all_all.request.json`
- 期望文件:`fixtures/ctl/cancel_all_account_venue_inst.record.hex`、`fixtures/ctl/cancel_all_all.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_145_golden_cancel_all`

### CTP-146 Drain 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/drain_default.request.json`、`fixtures/ctl/drain_keep_resting.request.json`
- 期望文件:`fixtures/ctl/drain_default.record.hex`、`fixtures/ctl/drain_keep_resting.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_146_golden_drain`

### CTP-147 HedgeFix 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/hedge_fix_group.request.json`
- 期望文件:`fixtures/ctl/hedge_fix_group.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_147_golden_hedge_fix`

### CTP-148 WriteOffResidual 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/write_off_residual_group.request.json`
- 期望文件:`fixtures/ctl/write_off_residual_group.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_148_golden_write_off_residual`

### CTP-149 SmokeOrder 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/smoke_order_ttl_default.request.json`、`fixtures/ctl/smoke_order_ttl_max.request.json`、`fixtures/ctl/smoke_order_sell.request.json`
- 期望文件:`fixtures/ctl/smoke_order_ttl_default.record.hex`、`fixtures/ctl/smoke_order_ttl_max.record.hex`、`fixtures/ctl/smoke_order_sell.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_149_golden_smoke_order`

### CTP-150 AdoptExchange 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/adopt_exchange_position.request.json`、`fixtures/ctl/adopt_exchange_balance_group.request.json`、`fixtures/ctl/adopt_exchange_balance_unassigned.request.json`
- 期望文件:`fixtures/ctl/adopt_exchange_position.record.hex`、`fixtures/ctl/adopt_exchange_balance_group.record.hex`、`fixtures/ctl/adopt_exchange_balance_unassigned.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_150_golden_adopt_exchange`

### CTP-151 SetBalanceBaseline 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/set_balance_baseline_venue.request.json`
- 期望文件:`fixtures/ctl/set_balance_baseline_venue.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_151_golden_set_balance_baseline`

### CTP-152 VenueNotice 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/venue_notice_delisting_inst.request.json`、`fixtures/ctl/venue_notice_delisting_venue.request.json`、`fixtures/ctl/venue_notice_funding_interval.request.json`、`fixtures/ctl/venue_notice_maintenance.request.json`
- 期望文件:`fixtures/ctl/venue_notice_delisting_inst.record.hex`、`fixtures/ctl/venue_notice_delisting_venue.record.hex`、`fixtures/ctl/venue_notice_funding_interval.record.hex`、`fixtures/ctl/venue_notice_maintenance.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_152_golden_venue_notice`

### CTP-153 SimReset 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/sim_reset_sim_account.request.json`
- 期望文件:`fixtures/ctl/sim_reset_sim_account.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_153_golden_sim_reset`

### CTP-154 SimTransfer 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/sim_transfer_btc.request.json`
- 期望文件:`fixtures/ctl/sim_transfer_btc.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_154_golden_sim_transfer`

### CTP-155 RecordTransfer 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/record_transfer_usdt.request.json`
- 期望文件:`fixtures/ctl/record_transfer_usdt.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_155_golden_record_transfer`

### CTP-156 Annotate 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/annotate_ops_note.request.json`、`fixtures/ctl/annotate_local_write_digest.request.json`、`fixtures/ctl/annotate_incident.request.json`
- 期望文件:`fixtures/ctl/annotate_ops_note.record.hex`、`fixtures/ctl/annotate_local_write_digest.record.hex`、`fixtures/ctl/annotate_incident.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_156_golden_annotate`

### CTP-157 ReplaceRefdata 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/replace_refdata_v8.request.json`
- 期望文件:`fixtures/ctl/replace_refdata_v8.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_157_golden_replace_refdata`

### CTP-158 StrategyCmd 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/strategy_cmd_open_group.request.json`、`fixtures/ctl/strategy_cmd_reduce_only_none.request.json`
- 期望文件:`fixtures/ctl/strategy_cmd_open_group.record.hex`、`fixtures/ctl/strategy_cmd_reduce_only_none.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_158_golden_strategy_cmd`

### CTP-159 InjectWarmup 的请求规范化为固定字节与 cmd_hash

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/inject_warmup_chunk0.request.json`、`fixtures/ctl/inject_warmup_last_chunk.request.json`
- 期望文件:`fixtures/ctl/inject_warmup_chunk0.record.hex`、`fixtures/ctl/inject_warmup_last_chunk.record.hex`与 `.cmd_hash.hex`(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_159_golden_inject_warmup`

### CTP-160 ReconcileReport 的请求规范化为固定字节

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:golden
- 里程碑:M2
- 前置:P0 的 refdata 以 `fixtures/refdata/ctl-p0.blob` 固定;记录中 `expires_tsc`、`peer_uid`、`role` 为 0
- 输入:`fixtures/ctl/reconcile_report_three_items.request.json`
- 期望文件:`fixtures/ctl/reconcile_report_three_items.record.hex`(每条记录一行;没有 cmd_hash)(同名)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_160_golden_reconcile_report`

### CTP-161 响应的 JSON 渲染与金样逐字节相同

- 规则:[请求与响应](README.md#请求与响应)、[开发备忘](README.md#开发备忘)
- 级别:golden
- 里程碑:M2
- 前置:P0;每个输入是一个 `CmdResponse` 的构造参数(`status`、`CmdResult` 字段或 gateway 码、`cmd_hash`、`engine`、`body`)
- 输入:`fixtures/ctl/responses/applied_preview_ttl.in.toml`、`accepted.in.toml`、`invalid_request_unknown_type.in.toml`、`forbidden.in.toml`、`param_invalid_strategy_field.in.toml`、`refdata_assembly.in.toml`、`get_schema.in.toml`
- 期望文件:`fixtures/ctl/responses/<同名>.json`(紧凑 JSON,键序固定)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_161_golden_responses`

### CTP-162 请求帧的字节与金样逐字节相同

- 规则:[使用本组件](README.md#使用本组件)
- 级别:golden
- 里程碑:M2
- 前置:无参数
- 输入:CTP-140–CTP-160 的全部 `fixtures/ctl/*.request.json`(紧凑 UTF-8)
- 期望文件:`fixtures/ctl/frames/<同名>.frame.bin`(4 字节大端长度 + JSON 原字节)
- 实现:`crates/hft-ctl-proto/tests/golden.rs::ctp_162_golden_frames`

### CTP-170 fuzz 目标 ctl_frame 零 panic

- 规则:[使用本组件](README.md#使用本组件)、[请求与响应](README.md#请求与响应)
- 级别:fuzz
- 里程碑:M2
- 目标:`ctl_frame`:任意字节流按帧切分(`frame_len` + 取体),每帧走模式解析与错误渲染
- 语料:`fixtures/ctl/frames/*.frame.bin` 与 `fixtures/ctl/regressions/frame/`
- 判定:零 panic、零超时(崩溃与超时口径见 [testing §3.3](../../docs/testing.md#33-fuzz));进入服务器上 `hft-verify` 的轮转与 CI 每夜冒烟;不计入上线门槛 ③
- 实现:`crates/hft-ctl-proto/fuzz/fuzz_targets/ctl_frame.rs::ctp_170_ctl_frame`

### CTP-171 fuzz 目标 ctl_normalize 零 panic

- 规则:[请求与响应](README.md#请求与响应)、[实现说明](README.md#实现说明)
- 级别:fuzz
- 目标:`ctl_normalize`:任意字节作为请求 JSON,解析成功的再以 P0 refdata 调 `normalize`、`class_of`、`allowed`、`needs_confirm_static` 与 `check_confirm_fields`
- 里程碑:M2
- 语料:`fixtures/ctl/*.request.json` 与 `fixtures/ctl/regressions/normalize/`
- 判定:零 panic、零超时(口径见 [testing §3.3](../../docs/testing.md#33-fuzz));进入 `hft-verify` 轮转与 CI 每夜冒烟;不计入上线门槛 ③
- 实现:`crates/hft-ctl-proto/fuzz/fuzz_targets/ctl_normalize.rs::ctp_171_ctl_normalize`

### CTP-172 一帧上限大小的请求的规范化耗时

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`ctlproto/normalize_1m`
- 前置:`SetParams`,`blob_b64` 解码后 786 432 字节(`MAX_BLOB`),P0 refdata
- 实现:`crates/hft-ctl-proto/benches/normalize.rs::ctp_172_normalize_1m`

### CTP-173 依赖闭包只含 L0 crate 与列出的外部库

- 规则:[依赖](README.md#依赖)、[概要](README.md#概要)
- 级别:unit
- 里程碑:M2
- 前置:`tools/xtask/deps.toml`
- 步骤:
  1. `cargo xtask deps` 检查 `hft-ctl-proto` 的依赖闭包(含 dev 依赖)
- 期望:
  - 内部依赖恰为 {`hft-types`、`hft-refdata`、`hft-event`};闭包中没有 `hft-strategy-sdk`、`hft-strategy-registry`、任何 `strategies/*`、L1 引擎 crate(`hft-oms`、`hft-risk`、`hft-ledger`、`hft-exec`、`hft-book`)与 `tokio`
  - 外部直接依赖是 {`serde`、`serde_json`、`uuid`、`sha2`、`base64`、`zerocopy`} 的子集
- 实现:`tools/xtask/tests/deps_ctl.rs::ctp_173_ctl_proto_dep_closure`

### CTP-174 源码不做 I/O、不建线程、不读时钟

- 规则:[依赖](README.md#依赖)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M2
- 前置:本 crate 的 `clippy.toml`
- 步骤:
  1. 在 `src/` 下以 clippy 的 `disallowed_methods`/`disallowed_types` 检查
  2. 往测试副本的 `src/normalize.rs` 加一行 `std::time::SystemTime::now()` 后再检查
- 期望:
  - 步骤 1 通过,`std::fs`、`std::net`、`std::time`、`std::thread`、`std::env` 都在禁用表里
  - 步骤 2 报 `disallowed_methods`,检查失败
- 实现:`tools/xtask/tests/purity.rs::ctp_174_ctl_proto_pure`

### CTP-175 GetSchema 的 schema JSON 原样作为响应 body

- 规则:[策略参数 schema JSON](README.md#策略参数-schema-json)
- 级别:prop
- 里程碑:M2
- 性质:把任意 JSON 值作为 `CmdResponse.body` 序列化后再解析,得到与输入相等的值;序列化后的 `body` 片段与 `serde_json::to_vec(&输入)` 逐字节相同(本 crate 不改键序、不改数值表示)
- 生成:深度 ≤ 6、每层 ≤ 16 个键或元素的 JSON 值,含大整数、长字符串与 Unicode
- 参考:输入值
- 例数:PR 500
- 实现:`crates/hft-ctl-proto/tests/prop.rs::ctp_175_schema_body_passthrough`

### CTP-176 SimFault 只对 CLI 的 Admin 放行

- 规则:[权限表](README.md#权限表)、[主体与角色码](README.md#主体与角色码)
- 级别:table
- 里程碑:M2
- 前置:P0;命令 = `SimFault{account = sim-a, venue = binance-usdm, fault = RejectNew, delay_ms = 0}`;调用 `allowed(roles, principal, chat, &cmd)`,主体记号见基线;"放行" = `Ok(())`
- 实现:`crates/hft-ctl-proto/tests/perm_matrix.rs::ctp_176_perm_matrix`

| 行 | 主体 | 期望 |
|---|---|---|
| CTP-176.1 | `tgV`(Viewer,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-176.2 | `tgVg`(Viewer,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-176.3 | `tgT`(Trader,私聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-176.4 | `tgTg`(Trader,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-176.5 | `tgA`(Admin,私聊) | `Err(PermErr)`,渲染为 `Forbidden`(仅 `cli:<login>`) |
| CTP-176.6 | `tgAg`(Admin,群聊) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-176.7 | `cliV`(Viewer) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-176.8 | `cliT`(Trader) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-176.9 | `cliA`(Admin) | 放行 |
| CTP-176.10 | `bot`(`ops_bot` 自身) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-176.11 | `pipe`(`pipeline`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-176.12 | `recon`(`reconciler`) | `Err(PermErr)`,渲染为 `Forbidden` |
| CTP-176.13 | `sysd`(`cli:systemd`) | `Err(PermErr)`,渲染为 `Forbidden` |

### CTP-177 SimFault 的 fault 与 delay_ms 规范化

- 规则:[请求与响应](README.md#请求与响应)
- 级别:table
- 里程碑:M2
- 前置:P0;`E0` + `{"type":"SimFault","account":"sim-a","venue":"binance-usdm",…}`,各行给出 `fault` 与 `delay_ms`
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_177_simfault_args`

| 行 | `fault` / `delay_ms` | 期望 |
|---|---|---|
| CTP-177.1 | `"RejectNew"` / `"0"` | `Ok`;记录为 `SimFault`,`delay_ms = 0` |
| CTP-177.2 | `"DelayReports"` / `"1"` | `Ok`,`delay_ms = 1` |
| CTP-177.3 | `"DelayReports"` / `"0"` | `InvalidArgs{BadValue}` |
| CTP-177.4 | `"DelayReports"` / `"60000"` | `Ok` |
| CTP-177.5 | `"DelayReports"` / `"60001"` | `InvalidArgs{BadValue}` |
| CTP-177.6 | `"Disconnect"` / `"5"` | `InvalidArgs{BadValue}` |
| CTP-177.7 | `"Off"` / `"0"` | `Ok` |
| CTP-177.8 | `"Pause"` / `"0"` | `invalid_request`,`detail = UnknownDiscriminant` |

### CTP-178 带原始凭据的报告规范化

- 规则:[实现规格](README.md#请求与响应)
- 级别:unit
- 里程碑:M2
- 前置:两项同账户报告与完整原始成交,refdata 开启严格永续结算
- 步骤:解析 JSON、重新序列化并规范化;检查环记录;分别提交空、截断、超量、零字节凭据
- 期望:JSON 往返保留凭据,合法组写一条 ReconcileBatch 且原始成交字节一致;坏体拒绝
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_178_reconciliation_receipts_use_one_atomic_record`

### CTP-179 成交语义不成立的原始凭据在规范化时拒绝

- 规则:[实现规格](README.md#请求与响应)
- 级别:table
- 里程碑:M2
- 前置:与 CTP-178 相同的 refdata(严格永续结算)与两项报告;合法凭据数量 0.001 BTC、价格 60000、trade_id = 7、fill_seq = 0、现金覆盖量等于数量
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_179_reconciliation_receipts_reject_bad_fill_semantics`

| 行 | 凭据改动 | 期望 |
|---|---|---|
| CTP-179.1 | 数量与现金覆盖量为 0 | `InvalidArgs{BadValue}` |
| CTP-179.2 | 价格为 0 | `InvalidArgs{BadValue}` |
| CTP-179.3 | 现金覆盖量比数量少 1 raw | `InvalidArgs{BadValue}` |
| CTP-179.4 | 成因为 Manual | `InvalidArgs{BadValue}` |
| CTP-179.5 | 有 trade_id 且 fill_seq = 1 | `InvalidArgs{BadValue}` |
| CTP-179.6 | trade_id = 0 且 fill_seq = 0 | `InvalidArgs{BadValue}` |
| CTP-179.7 | account 与分组不同 | `InvalidArgs{BadValue}` |

未改动的合法凭据规范化成功。

### CTP-180 BindSlot 的名字与账户

- 规则:[请求与响应](README.md#请求与响应)
- 级别:table
- 里程碑:M3
- 前置:P0 加 venue `polymarket-clob`(VenueId 15)与其现货 inst 5(符号 `btc-up-0`);issuer `pipeline`;`start_s` 1792425300、`end_s` 1792425600、`wire_id` 为 78 位十进制代币 id(大端首字节 0xCE)
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_180_bind_slot_names_and_account`

| 行 | venue / inst / account | 期望 |
|---|---|---|
| CTP-180.1 | `polymarket-clob` / `polymarket-clob:btc-up-0` / `*` | `ControlCmd::BindSlot{venue 15, inst 5, start_s, end_s}`,`wire_id[0] = 0xCE` |
| CTP-180.2 | `15` / `5` / `*` | 同样规范化为 `BindSlot` |
| CTP-180.3 | `POLYMARKET_CLOB` / … / `*` | `InvalidArgs{UnknownName}` |
| CTP-180.4 | … / `btc-up-0`(无 venue 前缀)/ `*` | `InvalidArgs{UnknownName}` |
| CTP-180.5 | … / … / 缺省 | `InvalidArgs{AccountRequired}` |

### CTP-181 Settle 的名字、兑付价与账户

- 规则:[请求与响应](README.md#请求与响应)
- 级别:table
- 里程碑:M3
- 前置:同 CTP-180;issuer `pipeline`;`venue = polymarket-clob`、`end_s` 1792425600
- 实现:`crates/hft-ctl-proto/tests/normalize.rs::ctp_181_settle_names_payout_and_account`

| 行 | inst / payout / account | 期望 |
|---|---|---|
| CTP-181.1 | `polymarket-clob:btc-up-0` / `"1"` / `*` | `ControlCmd::Settle{venue 15, inst 5, end_s, payout}`,`payout` = 该 inst 价格单位的 1 |
| CTP-181.2 | 同上 / `"0"` / `*` | `payout = 0` |
| CTP-181.3 | `btc-up-0`(无 venue 前缀)/ `"1"` / `*` | `InvalidArgs{UnknownName}` |
| CTP-181.4 | … / `"1"` / 缺省 | `InvalidArgs{AccountRequired}` |
| CTP-181.5 | … / `"0.00000000001"`(小数位多于价格指数)/ `*` | `InvalidArgs{BadValue}` |

## 验收

- 本 crate 在 M2 完成的标准:本文件里程碑为 M2 的用例全部有同编号的实现且通过([docs/tests §1](../../docs/tests/README.md#1-地位))。
- 属性测试(CTP-004、CTP-055、CTP-057、CTP-058、CTP-082、CTP-094、CTP-095、CTP-175):PR 按各用例的例数;CI 每夜以 100 倍例数运行一次。
- 金样(CTP-140–CTP-162):Rust 侧逐字节一致;Python 客户端对同一组金样的一致性由 [hft-ctl 的 Python 客户端用例](../hft-ctl/TESTS.md) 验证。
- fuzz(CTP-170、CTP-171):CI 每夜冒烟与服务器上 `hft-verify` 轮转零 panic;它们不是 core 解码器目标,不计入上线门槛 ③(计入范围与阈值归 [testing §8](../../docs/testing.md#8-上线门槛))。
- 基准 `ctlproto/normalize_1m` 的上限只在 [延迟与资源契约](README.md#延迟与资源契约) 登记,参考机判定。
