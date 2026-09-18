//! WP-08 incident-eval fixture (synthetic test data). Clean counterpart for no-pid-timestamp-temp-naming — PID alone (rule 15's recommended fix) must not fire.
let path = std::env::temp_dir().join(format!("push-runner-snapshot-{}.json", std::process::id()));
