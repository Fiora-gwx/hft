// SDK-105.3: min = 10, default = 9
hft_strategy_sdk::define_params! {
    pub struct P(schema_id = 0xFF01, version = 1, fields = PF) {
        x: Count [min = 10, max = 20, default = 9],
    }
}

fn main() {
    let _ = P::SCHEMA;
}
