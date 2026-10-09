// SDK-105.7: Px field without per_group
hft_strategy_sdk::define_params! {
    pub struct P(schema_id = 0xFF01, version = 1, fields = PF) {
        x: Px [min = 0, max = 10, default = 0],
    }
}

fn main() {
    let _ = P::SCHEMA;
}
