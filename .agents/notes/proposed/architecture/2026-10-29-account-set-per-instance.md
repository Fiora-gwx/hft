# Agent Note: 一个账户只被一个实例交易;一个实例可交易多个账户

Status: proposed

> **重构修订(2026-10-29,负责人决定)**:本 Note 放宽 [one-account-per-strategy-instance](2026-10-02-one-account-per-strategy-instance.md) 第 1 条的"一实例一账户";该 Note 第 3–6 条(身份核验、按账户的爆炸半径、模拟账户同样是账户、一账户一系统)保留。落地在重构 M4;落地前代码仍按一实例一账户校验。设计依据:重构设计 2026-10-29 §1.2 D6、§6、§9 第 2–3 条(仓外 `E:\trade\handoff\hft-refactor-design-2026-10-29.md`)。

## Problem

同一策略需要同时驱动多个钱包:Polymarket 的下单/撤单限额按签名者计,挂单与吃单分账户可以各用一份预算;两条腿分属两个钱包便于各自赎回;拆分库存降低单账户风险。今天 `AccountMeta.bound_instance` 与 sequencer 的实例装配是 1:1,hft-core 每个 venue 只允许一个实盘账户(`AccountCapacity`),gw 的槽镜像按槽号索引、各账户共用,REST 池只装第一个账户。

## Proposal

1. **绑定方向**:`AccountMeta.bound_instance` 仍表示"这个账户只被这一个实例交易"(防串单的目的不变);一个实例可以绑定一个账户集合(`AccountSet`,≤ 8 个,带角色 `Any | MakerOnly | TakerOnly | Side | Inst | Standby`)。去掉 refdata 的 `InstanceMultiBound`,改为校验"每个账户只出现在一个实例的集合里"。
2. **下单归属**:`OrderIntent.account` 由策略填写,或交给路由器按 `RoutePolicy` 填写;cell 键仍按 (策略, 账户, group),所以模式、cap、Kill 仍可按账户。
3. **执行层按账户**:gw 槽镜像键改为 (账户, 槽);每个账户一个 REST 池、独立限速桶与延迟统计;去掉 `AccountCapacity`(上限 `KillBits::MAX_ACCOUNTS_PER_GW`)。
4. **聚合风控与自成交**:新增按实例聚合的名义、每窗净份数与日亏限额;风控链新增 `SELF_TRADE` 检查(同 token 对手价交叉、互补 token 同向价格和 ≥ 1),默认拒绝;模拟器执行同一规则。
5. **钱包形态与出口 IP**:每个账户一个独立 Deposit Wallet + 独立 API key + 独立 Builder key;所有账户共用实盘箱同一个出口 IP(`SHARED_EGRESS_IP`),Cloudflare 的 IP 级限额由全部账户共享。

## Alternatives considered

**维持 1:1,一个钱包一个实例。** 策略逻辑要在实例之间协调(配对、库存、每窗上限),违背单写者内完成决策的结构。

**多个实例共用一个账户。** 原 Note 否决的理由(单向持仓、保证金与 PnL 不可拆、爆炸半径)仍成立。

**每账户独立出口 IP。** 负责人决定不采用:PM 的 IP 级限额远高于我们的峰值(8 个账户合计 < 2 %),多 EIP 增加运维面。

## Acceptance criteria

- DST:两账户互补买单价格和 ≥ 1 → 后到者 `SELF_TRADE` 拒绝(或按 `stp_mode` 先撤后放)。
- `Kill{Account(a)}` 不影响同实例的另一账户;实例级日亏触发封全部账户。
- gw 槽镜像在两账户同槽号下无串位(属性测试)。
- 含两个账户的 prod journal `replayer verify` 逐字节一致。

## Risks

- `bound_instance` 在热路径上约 94 处引用,回归面大:cell 键不变,先改 refdata 校验与装配,再改 gw;DST 覆盖。
- 多钱包的链上与 API 运维成本:赎回与钱包脚本按账户参数化,先 2 个账户。
