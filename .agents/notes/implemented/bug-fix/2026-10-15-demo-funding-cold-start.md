# Agent Note: demo 权限枚举与 funding 周期冷启动

Status: implemented

## Problem

真实 Bitget demo API-info 在认证读取与下单均成功时返回 unknown/空权限。生产严格解析拒绝此响应，无法启动 demo。两个产品的 WS 周期推断需等待 nextFundingTime 变化，使冷启动不能在首个结算前执行依赖实际周期的策略。

## Decision

负责人明确批准 demo-only 权限豁免与 REST 周期引导。环境通过新 warm 扩展 account_decode_env 传入，旧入口保持生产严格；仅 Testnet + is_prod=false + 精确 unknown/空列表允许继续。身份矛盾、withdraw、read-only、业务/传输错误继续拒绝。权限枚举不被标记为已验证，运行验收必须分别记录认证读取与首单 ack。

启动最后读取公共 fundingInfo/premiumIndex 或 current-fund-rate。完整成功结果只 seed feed 上下文的 inst/next/interval；费率与实际行情时间仍从第一条 WS 获取，完整 Funding 事件写 seq journal 后影响决策，因此回放不依赖 REST 或时钟。任何可选查询失败清空整批 seed，保留原未知周期等待；不豁免账户查询，不从费率估造资金费现金。

## Alternatives considered

从 refdata 硬编码实际周期不能反映交易所调整；只发布一次 REST Funding 会被随后未知周期 WS 覆盖；修改 book/策略对 interval=0 的解释会改变资金语义。用 REST seed 同一协议上下文，后续仍由真实 next 变化更新周期。

## Validation

BIN-033/BGT-034 覆盖完整周期、首帧保持、后续 8h→4h 变化、坏字段/时间/状态；CORE-120 脚本传输覆盖成功、超时、429、坏正文与部分结果丢弃。BGT-328 对真实脱敏权限帧验证 demo 成功、生产账户与生产入口失败、UID 矛盾和可见危险权限失败。真实主机两 API 已确认 ETHUSDT 下一次 16:00 UTC、8 小时；这不证明引擎成交或结算已通过。
