//! proptest strategies (README "2. proptest 生成器"): `market_stream` (core market data with
//! connection health), `perturb`, `order_lifecycle`, `control_cmds`, `fills_with_fee_asset`,
//! `funding_flows`, `margin_path`, `one_sided_move`, `external_fills` and `account_config`.

pub mod ctl;
pub mod external;
pub mod fees;
pub mod margin;
pub mod market;
pub mod orders;
pub mod perturb;
