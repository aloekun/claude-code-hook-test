//! WP-08 incident-eval fixture (synthetic test data). Clean counterpart for no-weak-temp-uniqueness (must NOT fire).
let snapshot_path = std::env::temp_dir().join(format!("push-runner-snapshot-{}.json", std::process::id()));
