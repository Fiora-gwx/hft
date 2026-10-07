//! TEL-001: `TelemetryConfig` defaults.

use hft_telemetry::TelemetryConfig;

#[test]
fn tel_001_defaults() {
    let c = TelemetryConfig::default();
    assert_eq!(c.window_s, 10);
    assert_eq!(c.sig_digits, 3);
    assert_eq!(c.min_ns, 1);
    assert_eq!(c.max_ns, 10_000_000_000);
    assert_eq!(c.clock_untrusted_offset_ns, 200_000);
}
