// SDK-105.10: version = 3 with largest since = 2
hft_strategy_sdk::define_params! {
    pub struct P(schema_id = 0xFF01, version = 3, fields = PF) {
        a: Count [max = 1],
        b: Count [max = 1, since = 2],
    }
}

fn main() {
    let _ = P::SCHEMA;
}
