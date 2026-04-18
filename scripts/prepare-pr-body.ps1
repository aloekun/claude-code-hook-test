# prepare-pr-body.ps1
# stdin から PR body を受け取り `.tmp-pr-body.md` に書き出し、そのパスを stdout に返す。
# -Cleanup 指定時は `.tmp-pr-body.md` を削除する。
#
# Usage:
#   "PR body content" | pnpm prepare-pr-body       -> writes .tmp-pr-body.md and prints path
#   pnpm prepare-pr-body:cleanup                   -> removes .tmp-pr-body.md if present

param(
    [switch]$Cleanup
)

$ErrorActionPreference = 'Stop'
# `.tmp-pr-body.md` はリポジトリルートに置く。
# CWD ではなくスクリプト自身の位置 (scripts/) の親ディレクトリを基点にすることで、
# 呼び出しディレクトリに関わらずノーマル/Cleanup 両モードが同じファイルを対象にできる。
$repoRoot = Split-Path -Parent $PSScriptRoot
$bodyPath = Join-Path $repoRoot '.tmp-pr-body.md'

if ($Cleanup) {
    if (Test-Path -LiteralPath $bodyPath) {
        Remove-Item -LiteralPath $bodyPath -Force
        Write-Output "cleaned: $bodyPath"
    } else {
        Write-Output "nothing to clean: $bodyPath not found"
    }
    exit 0
}

# gh pr create --body-file の互換性のため BOM なし UTF-8 で書き出す。
# Console.InputEncoding はデフォルトで system code page (日本語 Windows では Shift-JIS) のため、
# 明示的に UTF-8 に切り替えて mojibake を防ぐ
$utf8NoBom = New-Object System.Text.UTF8Encoding $false
[Console]::InputEncoding = $utf8NoBom
$body = [Console]::In.ReadToEnd()
if ([string]::IsNullOrWhiteSpace($body)) {
    Write-Error "stdin is empty or whitespace-only. Pipe PR body content into this script."
    exit 1
}

[System.IO.File]::WriteAllText($bodyPath, $body, $utf8NoBom)
Write-Output $bodyPath
