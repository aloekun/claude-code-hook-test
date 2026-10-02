//! WP-08 incident-eval fixture (synthetic test data). Clean counterpart for gh-json-files-truncated (must NOT fire).
let out = Command::new("gh").args(["api", "--paginate", &files_path, "--jq", ".[].filename"]).output();
