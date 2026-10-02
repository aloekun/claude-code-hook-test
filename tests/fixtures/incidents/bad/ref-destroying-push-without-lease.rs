//! WP-08 incident-eval fixture (synthetic test data). Reproduces PR #437 (ref-destroying-push-without-lease).
let out = Command::new("git").args(["push", url, "--delete", &refspec]).output();
