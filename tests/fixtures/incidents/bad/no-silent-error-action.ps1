# WP-08 incident-eval fixture (synthetic test data). Reproduces PR #85 (no-silent-error-action).
$data = Get-Item $path -ErrorAction SilentlyContinue
