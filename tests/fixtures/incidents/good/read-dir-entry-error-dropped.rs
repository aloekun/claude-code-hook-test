//! WP-08 incident-eval fixture (synthetic test data). Clean counterpart for read-dir-entry-error-dropped (must NOT fire).
// NOTE: コメントアウトした違反は発火しない: let Ok(entries) = std::fs::read_dir(dir) else { return; }; for entry in entries.flatten() { check(entry.path()); }
let Ok(entries) = std::fs::read_dir(dir) else { return; }; for entry in entries { let path = entry.map_err(|e| format!("entry を読めません: {e}"))?.path(); check(path); }
