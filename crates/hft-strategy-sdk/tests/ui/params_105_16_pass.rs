// SDK-105.16: Bool field with minor = Any
hft_strategy_sdk::define_params! {
    pub struct P(schema_id = 0xFF01, version = 1, fields = PF) {
        x: Bool [default = 1, minor = Any],
    }
}

fn main() {
    let _ = P::SCHEMA;
}
