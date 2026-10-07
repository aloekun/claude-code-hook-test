//! WP-08 incident-eval fixture (synthetic test data). Clean counterpart for gh-json-files-truncated (must NOT fire).
let cap = run_gh(&["api", "--paginate", &files_path, "--jq", ".[].filename"]);
