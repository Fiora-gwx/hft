# Agent Note: demo 资金费查询与金额边界

Status: implemented

## Problem

Bitget demo 的 ETHUSDT 空头持有跨过 2026-10-15 08:00 UTC,认证财务流水实际收入 0.00238005 USDT,账单时间为整点后 1550 ms。按 Notional -6 精确解析使整页返回 Number。Binance 的同类金额入口也要求精确六位,单元输入复现相同失败。

Binance demo 使用 Ed25519 凭证,账户 REST 已支持该算法,独立资金费查询却仅接受 HMAC。BIN-332 回归在生成请求时复现 Cfg。

## Decision

两个资金费历史入口在 Notional -6 边界作一次 HalfEven 舍入,保留现金方向与账单身份;格式、溢出、时间窗、币种与合约校验不变。原始金额留在脱敏 fixture,不改报文迁就解析器。真实样本的表示差额为 0.00000005 USDT,每笔误差界为 0.5 微 USDT。

Binance 资金费查询复用账户 REST 签名入口,支持 HMAC 与 URL 编码的 Ed25519 Base64,带配置 recvWindow,保持敏感标记。

## Alternatives considered

要求精确六位会持续拒绝真实账单;截断产生方向性误差。扩大 FundingBill/Notional 的持久表示涉及事件与账本迁移,不属于本轮协议修复。不能把钱包余额差额当作逐合约资金费。

## Validation

BGT-336 的真实金样先失败后通过,覆盖实际信用流水与忽略交易费。两协议覆盖正负/IN-OUT 与 HalfEven 半值取偶。BIN-332 用公开 RFC 8032 测试密钥对完整请求独立验签。BIN-303/BGT-307 的真实行情前后帧证明同一上下文从未知周期变为 28800 秒;不是八小时变四小时。

## Risks

这不是 hft-core 或策略实跑验收。Binance 合约 demo 余额为零,没有实际资金费样本。Bitget 只取得一笔收入,扣款、跨多品种、较长延迟与更高精度仍需采样。资金表示变更在 docs/changes.md 待评审。
