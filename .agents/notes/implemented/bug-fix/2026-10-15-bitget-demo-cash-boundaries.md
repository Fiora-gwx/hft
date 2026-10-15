# Agent Note: Bitget demo 现金边界的真实报文

Status: implemented

## Problem

UTA 现货挂单冻结 11.7485 USDT 后,WS `balance=9988.11962704`、`locked=11.7485`,两者之和等于挂单前现金 9999.86812704。独立 REST 挂单采样也满足同样关系。把 balance 当含冻结总额再减 locked 会重复扣款。

现货卖出订单频道给出手续费 `0.011571399` USDT,超过测试 refdata 的数量指数 -8,精确解析使整条成交失败。同笔 fill 频道给出截断到 8 位的小数,不能用另一个频道替换订单累计费用。

## Decision

按真实 UTA 数据,REST/WS 的现货 free 直接取 balance,locked 单列。永续仍不将 available 当现金。此决定取代“现货 balance 减 locked”的推断与合成测试期望;真实 fixture 不作数值修改。

手续费在资产数量指数处一次 HalfEven 舍入,再按已有累计差分计费;均价、成交量与币种检查不变。金额保留不足一个最小数量单位的表示误差,不声称原始十进制逐位无损。

## Alternatives considered

拒绝多余手续费精度会丢失真实成交;截断会引入单向误差;用 available 代替 balance 会混淆保证金购买力。修改真实报文以迁就原解码不可接受。

## Validation

BGT-318/324 的真实金样先复现 Number 与重复扣冻失败,覆盖买卖方向、同累计量 partial/filled 重复、REST/WS 锁定余额及 9 位小数费用。BGT-321 保证旁路 fill 推送不重复记账。

## Risks

demo 观察不能替代正式账户验收。资金语义在 docs/changes.md 标待评审;上线前需重复核验正式环境的同字段边界与多笔费用舍入差额。
