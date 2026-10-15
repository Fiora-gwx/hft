//! Cold schema inspection: collect missing keys without echoing configuration values.
use crate::Error;
use toml::Value;

pub fn check(text: &str) -> Result<(), Vec<Error>> {
    let value: Value =
        toml::from_str(text).map_err(|_| vec![Error::config("ConfigParse", "invalid TOML")])?;
    let mut missing = Vec::new();
    inspect(&value, "", "config", &mut missing).map_err(|e| vec![e])?;
    if missing.is_empty() { Ok(()) } else { Err(missing) }
}

fn inspect(value: &Value, path: &str, shape: &str, missing: &mut Vec<Error>) -> Result<(), Error> {
    let (required, optional): (&[&str], &[&str]) = match shape {
        "config" => (
            &[
                "schema", "instance", "paths", "blobs", "shutdown", "journal", "clock", "watchdog",
                "threads", "venues", "rings",
            ],
            &["sim", "startup"],
        ),
        "instance" => (&["name", "config_source"], &["order_env", "secrets_mode"]),
        "paths" => (
            &["journal_ram", "journal_cold", "ctl_socket", "ctl_principals"],
            &[],
        ),
        "blobs" => (&["refdata", "engine", "risk_params", "strategy_params"], &[]),
        "blob" => (&["path", "sha256"], &[]),
        "shutdown" => (&["cancel_open_orders", "drain_timeout_ms"], &[]),
        "journal" => (
            &["segment_bytes", "hot_keep"],
            &["raw_feed", "raw_gw", "latency_trace_capacity"],
        ),
        "journal_size" => (&["segment_bytes", "hot_keep"], &[]),
        "clock" => (&["step_threshold_ns", "interval_ms"], &[]),
        "watchdog" => (&["hb_stale_ms"], &[]),
        "startup" => (&["budget_s", "gate_timeout_s"], &[]),
        "thread" => (&["name", "task", "park_us", "nice"], &["core"]),
        "rings" => (&["elements", "bytes"], &[]),
        "venue" => (
            &["protocol", "id", "max_sub_insts", "stale_guard_ms"],
            &["src_ip", "bind_device", "account_io", "accounts"],
        ),
        "account_io" => (
            &[
                "timeout_ms",
                "interval_ms",
                "request_gap_ms",
                "max_cancel_rounds",
                "listen_keepalive_ms",
            ],
            &[],
        ),
        "keys" => (
            &["account", "api_key_path", "rest_api_key_path", "rest_secret_path"],
            &[
                "session_seed_path",
                "session_secret_path",
                "rest_sign_alg",
                "passphrase_path",
            ],
        ),
        "sim" => (
            &[
                "seed",
                "entry_us",
                "resp_us",
                "leverage",
                "mmr_raw",
                "liq_fee_raw",
                "liq_slippage_bp",
                "quote_interval_ms",
                "quotes",
            ],
            &[],
        ),
        "quote" => (&["inst", "bid_raw", "ask_raw", "qty_raw"], &[]),
        _ => return Err(Error::config("ConfigParse", "unknown configuration section")),
    };
    let table = value
        .as_table()
        .ok_or_else(|| Error::config("ConfigParse", "expected table"))?;
    if table
        .keys()
        .any(|key| !required.contains(&key.as_str()) && !optional.contains(&key.as_str()))
    {
        // Do not print an untrusted key or TOML source (it may contain an inline secret).
        return Err(Error::config("ConfigParse", "unknown configuration field"));
    }
    let join = |key: &str| {
        if path.is_empty() {
            key.to_owned()
        } else {
            format!("{path}.{key}")
        }
    };
    for key in required {
        if !table.contains_key(*key) {
            let code = if shape == "keys" && *key != "account" {
                "MissingCredential"
            } else {
                "MissingField"
            };
            missing.push(Error::config(code, join(key)));
        }
    }
    if shape == "keys"
        && !table.contains_key("session_seed_path")
        && !table.contains_key("session_secret_path")
    {
        missing.push(Error::config("MissingCredential", join("session_seed_path")));
    }
    for (key, child) in table {
        let child_shape = match (shape, key.as_str()) {
            (
                "config",
                "instance" | "paths" | "blobs" | "shutdown" | "journal" | "clock" | "watchdog" | "rings"
                | "sim" | "startup",
            ) => Some(key.as_str()),
            ("blobs", "refdata" | "engine" | "risk_params") => Some("blob"),
            ("venue", "account_io") => Some("account_io"),
            ("journal", "raw_feed" | "raw_gw") => Some("journal_size"),
            _ => None,
        };
        if let Some(child_shape) = child_shape {
            inspect(child, &join(key), child_shape, missing)?;
        }
        let array_shape = match (shape, key.as_str()) {
            ("config", "threads") => Some("thread"),
            ("blobs", "strategy_params") => Some("blob"),
            ("venue", "accounts") => Some("keys"),
            ("sim", "quotes") => Some("quote"),
            _ => None,
        };
        if let Some(array_shape) = array_shape {
            let items = child
                .as_array()
                .ok_or_else(|| Error::config("ConfigParse", "expected array"))?;
            for (index, item) in items.iter().enumerate() {
                inspect(item, &format!("{}[{index}]", join(key)), array_shape, missing)?;
            }
        }
        if shape == "config" && key == "venues" {
            let venues = child
                .as_table()
                .ok_or_else(|| Error::config("ConfigParse", "expected venues table"))?;
            for (index, item) in venues.values().enumerate() {
                inspect(item, &format!("venues[{index}]"), "venue", missing)?;
            }
        }
    }
    Ok(())
}
