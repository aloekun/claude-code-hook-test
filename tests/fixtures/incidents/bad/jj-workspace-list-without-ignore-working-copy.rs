//! WP-08 incident-eval fixture (synthetic test data). Reproduces PR #421 (jj-workspace-list-without-ignore-working-copy).
let output = Command::new("jj").args(["workspace", "list", "-T", "self.root()"]).output();
