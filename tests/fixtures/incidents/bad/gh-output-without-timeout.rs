//! WP-08 incident-eval fixture (synthetic test data). Reproduces PR #230 (gh-output-without-timeout).
let output = Command::new("gh")
    .args(["pr", "view", &pr_str, "--json", "commits,mergedAt"])
    .stdout(Stdio::piped())
    .output()
    .map_err(|e| format!("gh コマンド起動失敗: {}", e))?;
