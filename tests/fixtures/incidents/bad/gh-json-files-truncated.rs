//! WP-08 incident-eval fixture (synthetic test data). Reproduces PR #435 (gh-json-files-truncated).
let out = Command::new("gh").args(["pr", "view", &n, "--json", "files,commits"]).output();
