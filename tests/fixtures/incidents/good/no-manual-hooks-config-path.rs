//! WP-08 incident-eval fixture (synthetic test data). Clean counterpart for no-manual-hooks-config-path (must NOT fire).
fn config_path() -> std::path::PathBuf { lib_config_path::resolve_config("hooks-config.toml") }
