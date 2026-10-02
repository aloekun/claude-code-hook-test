// WP-08 incident-eval fixture (synthetic test data). Clean counterpart for gh-without-repo-in-node-script (must NOT fire).
const result = spawnSync("gh", ["pr", "list", "--repo", REPO, "--state", "merged", "--json", "number"], { timeout: GH_TIMEOUT_MS });
