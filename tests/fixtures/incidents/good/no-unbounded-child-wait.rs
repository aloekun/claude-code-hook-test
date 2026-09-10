//! WP-08 incident-eval fixture (synthetic test data). Clean counterpart for no-unbounded-child-wait (must NOT fire).
let status = wait_with_timeout_safe("my-exe", &mut child, 30).expect("wait");
