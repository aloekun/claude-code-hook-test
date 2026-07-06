# WP-08 incident-eval fixture (synthetic test data). Reproduces PR #85 (no-empty-powershell-catch).
try { Get-Item $path } catch {}
