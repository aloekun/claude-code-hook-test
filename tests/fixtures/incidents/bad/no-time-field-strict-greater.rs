//! WP-08 incident-eval fixture (synthetic test data). Reproduces PR #101 (no-time-field-strict-greater).
let recent = comments.iter().filter(|c| c.created_at > push_time);
