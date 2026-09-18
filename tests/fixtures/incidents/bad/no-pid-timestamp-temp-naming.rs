//! WP-08 incident-eval fixture (synthetic test data). Reproduces PR #229 (temp_dir() PID+timestamp manual naming, distinct from no-weak-temp-uniqueness's fixed-literal scope).
let path = std::env::temp_dir().join(format!("nightly-outcome-e2e-{}-{}.json", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
