// SDK-105.14: Bool field with minor = RelPct(10)
hft_strategy_sdk::define_params! {
    pub struct P(schema_id = 0xFF01, version = 1, fields = PF) {
        x: Bool [default = 1, minor = RelPct(10)],
    }
}

fn main() {
    let _ = P::SCHEMA;
}
