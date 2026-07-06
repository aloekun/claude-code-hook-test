//! WP-08 incident-eval fixture (synthetic test data). Clean counterpart for no-time-field-strict-greater (must NOT fire).
let recent = comments.iter().filter(|c| c.created_at >= push_time);
