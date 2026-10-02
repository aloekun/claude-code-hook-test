// WP-08 incident-eval fixture (synthetic test data). Reproduces PR #470 (gh-without-repo-in-node-script).
const result = spawnSync("gh", ["pr", "list", "--state", "merged", "--json", "number"], { timeout: GH_TIMEOUT_MS });
