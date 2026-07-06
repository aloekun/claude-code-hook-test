//! WP-08 incident-eval fixture (synthetic test data). Clean counterpart for no-write-result-discard (must NOT fire).
if let Err(e) = write_state(&state) { log_warn(&format!("state write failed: {e}")); }
