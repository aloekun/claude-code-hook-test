//! WP-08 incident-eval fixture (synthetic test data). Clean counterpart for jj-workspace-list-without-ignore-working-copy (must NOT fire).
let output = Command::new("jj").args(["workspace", "list", "--ignore-working-copy", "-T", "self.root()"]).output();
