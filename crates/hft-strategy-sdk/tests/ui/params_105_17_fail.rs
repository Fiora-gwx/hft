// SDK-105.17: minor = RelPct(0)
hft_strategy_sdk::define_params! {
    pub struct P(schema_id = 0xFF01, version = 1, fields = PF) {
        x: Count [max = 10, minor = RelPct(0)],
    }
}

fn main() {
    let _ = P::SCHEMA;
}
