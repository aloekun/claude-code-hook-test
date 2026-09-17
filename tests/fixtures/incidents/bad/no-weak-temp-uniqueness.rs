//! WP-08 incident-eval fixture (synthetic test data). Reproduces PR #405 (fixed temp_dir name).
let snapshot_path = std::env::temp_dir().join("push-runner-snapshot.json");
