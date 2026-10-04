# Agent Note: 读不到 UID 的测试网账户用 uid_policy = unverifiable

Status: proposed

> **第八轮第七次修订(M0-g,2026-10-04)**:事实同步——TOML 取值按 hft-cfg 惯例小写 `"verified" | "unverifiable"`(Rust 枚举 `UidPolicy::{Verified, Unverifiable}`,签字字段 `uid_policy_signoff`);`DuplicateVenueUid` 只对 `is_prod = true` 的账户是加载错误,非正式账户在同一交易所不同 `VenueId` 上 UID 相同(Binance 演示环境现货与 USDⓈ-M 共用 UID)只写 P1 告警。依据 [round8g 简报](../../../../design/_work/round8g-tests-brief.md) §4 RF4、RF6 与 [round8g 裁决](../../../../design/_work/round8g-rulings.md) RG8;规则归 [hft-refdata](../../../../crates/hft-refdata/README.md)。

## Problem

账户读不到 UID(`UNREADABLE`)时 `IDENTITY` 闸门保持关闭,没有任何覆盖;launch 只写"由 Admin 决定",却没有可用的机制。结果这个 venue 在测试网上根本不能下单:门槛 ⑥ 的下单、成交与对冲做不了,M7 也没有该 venue 的订单类延迟样本。Binance 正把测试网迁到 Demo Trading,旧测试网鉴权据第三方报告已不可用(待核实)([X4 评审](../../../../design/_work/review-2026-10-03-M0e-X4运行与交付.md) N-P1-7、§5-3)。

## Proposal

用户决定按推荐执行([round8-decisions](../../../../design/_work/round8-decisions.md) R8-12;规范见 [round8f 简报](../../../../design/_work/round8f-spec-brief.md) F6)。字段归 [hft-refdata](../../../../crates/hft-refdata/README.md)(gen-schema 导出),放行规则归 [hft-risk](../../../../crates/hft-risk/README.md),操作步骤归 [launch A](../../../../docs/runbooks/launch.md#a-联调dev2测试网模拟盘)。

1. 账户配置增加 `uid_policy = "verified" | "unverifiable"`(默认 `"verified"`)。
2. `"unverifiable"` 只允许 `is_prod = false` 的账户,装配时需要 Admin 签字记录;此时 `IDENTITY` 由"kill 自检通过 + 一次成功签名的只读请求"放行,并写 P1 Note 标明未核验。
3. venue-binance 按 Binance Demo Trading 端点核实(现货与 USDⓈ-M),待核实写法照旧。

## Alternatives considered

**测试网账户一律跳过 `IDENTITY`。** 测试网 key 放错账户的情形同样会让联调结论失真;只对读不到 UID 的账户开例外,且要签字。

**为测试网另写一套身份闸门。** 同一代码路径分叉,正式账户的闸门会少了测试网上的演练。

## Acceptance criteria

- refdata 加载:`is_prod = true` 的账户设 `"unverifiable"` → 拒绝;没有 Admin 签字记录 → 装配失败。
- 测试网联调:`"unverifiable"` 账户在 kill 自检与一次签名只读请求成功后 `IDENTITY` 置位,journal 有一条 P1 Note。

## Risks

- `"unverifiable"` 账户的 key 放错账户时无法由闸门发现;只用于测试网,上线记录写明未核验的账户。
