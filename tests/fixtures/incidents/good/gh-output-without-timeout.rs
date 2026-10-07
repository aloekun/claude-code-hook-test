//! WP-08 incident-eval fixture (synthetic test data). Clean counterpart for gh-output-without-timeout (must NOT fire).
let cap = lib_subprocess::run_cmd_direct_capture("gh", &["pr", "view", &pr_str, "--json", "commits,mergedAt"], GH_CMD_TIMEOUT_SECS);
