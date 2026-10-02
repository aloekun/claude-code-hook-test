// WP-08 incident-eval fixture (synthetic test data). Reproduces PR #470 (network-spawn-without-timeout).
const result = spawnSync("gh", ["pr", "list", "--repo", REPO, "--json", "number"], { cwd: REPO_ROOT, encoding: "utf8" });
