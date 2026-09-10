//! WP-08 incident-eval fixture (synthetic test data). Reproduces PR #254 (no-unbounded-child-wait).
let output = child.wait_with_output().expect("wait");
