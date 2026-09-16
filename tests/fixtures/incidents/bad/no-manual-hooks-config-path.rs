//! WP-08 incident-eval fixture (synthetic test data). Reproduces PR #267 (no-manual-hooks-config-path).
fn config_path() -> std::path::PathBuf { std::env::current_dir().unwrap_or_default().join(".claude").join("hooks-config.toml") }
