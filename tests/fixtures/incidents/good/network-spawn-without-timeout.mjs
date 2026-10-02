// WP-08 incident-eval fixture (synthetic test data). Clean counterpart for network-spawn-without-timeout (must NOT fire).
const result = spawnSync("gh", ["pr", "list", "--repo", REPO, "--json", "number"], { cwd: REPO_ROOT, encoding: "utf8", timeout: GH_TIMEOUT_MS });
