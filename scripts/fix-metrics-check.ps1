# fix-metrics-check.ps1 — Bundle Z Phase 2 (#B-β) deterministic check helper
#
# Usage:
#   pwsh -NoProfile -File scripts/fix-metrics-check.ps1 <file_path> [<pre_state_revset>]
#
# 振る舞い:
#   1. <pre_state_revset> (default: '@-') から <file_path> の pre-state を取得
#   2. .claude/hooks-post-tool-comment-lint-rust.exe --metrics で pre/post の両方の
#      file metrics を JSON 取得
#   3. 関数を name で突き合わせ、以下のいずれかが post で増加していたら exit 1:
#        - file 全体の non_doc_comment_count
#        - 任意の関数の length
#        - 任意の関数の max_nesting_depth
#   4. すべて非増加なら exit 0
#
# 設計上の制約 (本 PR で意図的に v1 として除外、`docs/pipeline-token-efficiency.md` PR 2 参照):
#   - change-site 周辺への scope 絞り込みは未実装 (file 全体 + 関数単位の比較)
#   - Rust 限定 (PoC、将来言語拡張で別 metric tool を検討)
#   - pre-state は jj revset で指定 (git ベースの場合は別途調整必要)

param(
    [Parameter(Mandatory = $true, Position = 0)][string]$FilePath,
    [Parameter(Position = 1)][string]$PreStateRevset = '@-'
)

$ErrorActionPreference = 'Stop'

# Windows + 日本語ロケール環境で jj の UTF-8 stdout が Shift-JIS と解釈されると
# Japanese 文字が mojibake → tree-sitter parser が早期失敗 (関数 1 件しか発見しない等)。
# 明示的に UTF-8 に揃える。
$prevOutEncoding = [Console]::OutputEncoding
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$repoRoot = Split-Path -Parent $PSScriptRoot
$exeRelPath = '.claude/hooks-post-tool-comment-lint-rust.exe'
$exePath = Join-Path $repoRoot $exeRelPath

if (-not (Test-Path -LiteralPath $exePath)) {
    Write-Error "$exeRelPath not found. Run: pnpm build:hooks-post-tool-comment-lint-rust"
    exit 2
}

$absFilePath = if ([System.IO.Path]::IsPathRooted($FilePath)) {
    $FilePath
} else {
    Join-Path $repoRoot $FilePath
}

if (-not (Test-Path -LiteralPath $absFilePath)) {
    Write-Error "post-state file not found: $absFilePath"
    exit 2
}

$relFilePath = if ([System.IO.Path]::IsPathRooted($FilePath)) {
    [System.IO.Path]::GetRelativePath($repoRoot, $FilePath).Replace('\', '/')
} else {
    $FilePath.Replace('\', '/')
}

$preTemp = Join-Path ([System.IO.Path]::GetTempPath()) ("fix-metrics-pre-{0}.rs" -f ([Guid]::NewGuid()))

try {
    Push-Location $repoRoot
    try {
        $preContent = & jj file show -r $PreStateRevset -- $relFilePath 2>&1
        if ($LASTEXITCODE -ne 0) {
            Write-Output (@{
                metrics_check = "skipped"
                reason        = "jj file show -r $PreStateRevset -- $relFilePath failed (file may be new in this revision)"
                jj_output     = ($preContent -join "`n")
            } | ConvertTo-Json -Depth 4)
            exit 0
        }
        $utf8NoBom = New-Object System.Text.UTF8Encoding $false
        [System.IO.File]::WriteAllText($preTemp, ($preContent -join "`n"), $utf8NoBom)
    } finally {
        Pop-Location
    }

    $preJson = & $exePath --metrics $preTemp
    if ($LASTEXITCODE -ne 0) {
        Write-Error "metrics computation failed for pre-state ($preTemp), exit=$LASTEXITCODE"
        exit 2
    }
    $postJson = & $exePath --metrics $absFilePath
    if ($LASTEXITCODE -ne 0) {
        Write-Error "metrics computation failed for post-state ($absFilePath), exit=$LASTEXITCODE"
        exit 2
    }

    $pre = $preJson | ConvertFrom-Json
    $post = $postJson | ConvertFrom-Json

    $violations = New-Object System.Collections.ArrayList

    if ($post.non_doc_comment_count -gt $pre.non_doc_comment_count) {
        [void]$violations.Add(@{
            metric = "non_doc_comment_count"
            pre    = $pre.non_doc_comment_count
            post   = $post.non_doc_comment_count
            delta  = $post.non_doc_comment_count - $pre.non_doc_comment_count
        })
    }

    $preFnByName = @{}
    foreach ($pf in $pre.functions) { $preFnByName[$pf.name] = $pf }

    foreach ($pf in $post.functions) {
        $matchedPre = $preFnByName[$pf.name]
        if ($null -eq $matchedPre) { continue }

        if ($pf.length -gt $matchedPre.length) {
            [void]$violations.Add(@{
                metric        = "function_length"
                function_name = $pf.name
                pre           = $matchedPre.length
                post          = $pf.length
                delta         = $pf.length - $matchedPre.length
            })
        }
        if ($pf.max_nesting_depth -gt $matchedPre.max_nesting_depth) {
            [void]$violations.Add(@{
                metric        = "max_nesting_depth"
                function_name = $pf.name
                pre           = $matchedPre.max_nesting_depth
                post          = $pf.max_nesting_depth
                delta         = $pf.max_nesting_depth - $matchedPre.max_nesting_depth
            })
        }
    }

    if ($violations.Count -gt 0) {
        Write-Output (@{
            metrics_check = "fail"
            file          = $relFilePath
            pre_revset    = $PreStateRevset
            violations    = $violations
        } | ConvertTo-Json -Depth 4)
        exit 1
    }

    Write-Output (@{
        metrics_check = "pass"
        file          = $relFilePath
        pre_revset    = $PreStateRevset
    } | ConvertTo-Json -Depth 4)
    exit 0
} finally {
    # Temp file cleanup: -ErrorAction Ignore は意図的 (削除失敗しても呼び出し元の
    # exit code を上書きしないため、かつ削除失敗は機能影響なし)。
    if (Test-Path -LiteralPath $preTemp) {
        Remove-Item -LiteralPath $preTemp -Force -ErrorAction Ignore
    }
    [Console]::OutputEncoding = $prevOutEncoding
}
