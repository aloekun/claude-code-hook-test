//! WP-08 incident-eval fixture (synthetic test data). Clean counterpart for ref-destroying-push-without-lease (must NOT fire).
let out = Command::new("git").args(["push", &format!("--force-with-lease=refs/heads/{b}:{sha}"), url, "--delete", &refspec]).output();
