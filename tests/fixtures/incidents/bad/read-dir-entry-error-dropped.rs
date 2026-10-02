//! WP-08 incident-eval fixture (synthetic test data). Reproduces PR #454 (read-dir-entry-error-dropped).
for entry in entries.flatten() { check(entry.path()); }
