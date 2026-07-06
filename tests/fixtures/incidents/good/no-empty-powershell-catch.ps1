# WP-08 incident-eval fixture (synthetic test data). Clean counterpart for no-empty-powershell-catch (must NOT fire).
try { Get-Item $path } catch { Write-Verbose "expected miss: $_" }
