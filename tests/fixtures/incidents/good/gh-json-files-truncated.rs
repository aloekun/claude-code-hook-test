//! WP-08 incident-eval fixture (synthetic test data). Clean counterpart for gh-json-files-truncated (must NOT fire).
let cap = run_gh(&["api", "--paginate", &files_path, "--jq", ".[].filename"]);
// NOTE: コメントアウトした違反は発火しない: let out = Command::new("gh").args(["pr", "view", &n, "--json", "files,commits"]).output();
