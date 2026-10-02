//! WP-08 incident-eval fixture (synthetic test data). Reproduces PR #454 (read-dir-entry-error-dropped).
let Ok(entries) = std::fs::read_dir(dir) else { return; }; for entry in entries.flatten() { check(entry.path()); }
