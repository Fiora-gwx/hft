// SDK-105.19: minor = RelPct(1) and RelPct(100)
hft_strategy_sdk::define_params! {
    pub struct P(schema_id = 0xFF01, version = 1, fields = PF) {
        a: Count [max = 10, minor = RelPct(1)],
        b: Count [max = 10, minor = RelPct(100)],
    }
}

fn main() {
    let _ = P::SCHEMA;
}
