# analyze-takt-timings.ps1 — takt run の内部 step/phase 別所要時間を集計する観測ツール
#
# 目的: push パイプラインの takt 部分 (reviewers / verify / fix / supervise ...) の
# 「どの処理にどれだけ時間がかかっているか」を run ログから決定論的に抽出し、最適化検討や
# 「重いが必要」の許容判断の材料にする。R3 (push-runs JSONL) が決定論 stage (quality_gate /
# takt 全体 / push) を持つのに対し、本ツールは takt **内部** の step/phase 粒度を補完する。
#
# 計測原理: 各 phase は logs/*.jsonl に phase_start と phase_complete を持ち、両者は
# phaseExecutionId で一意に対応する。duration = phase_complete.timestamp - phase_start.timestamp。
#
# 使い方:
#   pwsh -File scripts/analyze-takt-timings.ps1                     # refute run を集計
#   pwsh -File scripts/analyze-takt-timings.ps1 -Piece pre-push-review   # baseline を集計
#   pwsh -File scripts/analyze-takt-timings.ps1 -PerRun            # run 別内訳も出す
#   pwsh -File scripts/analyze-takt-timings.ps1 -Until 2026-07-19  # 観測スナップショットの再現 (例)
#
# -Since / -Until は meta.json の startTime (UTC) と文字列比較する半開区間 [Since, Until)。
# 判定を anchor した point-in-time スナップショットを後から再現するために -Until を使う
# (計測を publish する push 自体の run を除外できる = 「観測が対象を変える」問題への対処)。

param(
  [string]$Piece = "pre-push-review-refute",
  [string]$RunsDir = ".takt/runs",
  [string]$Since = "2026-07-17",
  [string]$Until = "9999-12-31",
  [switch]$PerRun
)

if (-not (Test-Path $RunsDir)) { Write-Error "runs dir not found: $RunsDir"; exit 1 }

# ISO 文字列を UTC DateTime にする。PowerShell の文字列 -lt/-ge は culture-aware で時刻部分
# (":" 区切り) を誤比較するため、日時の窓判定は必ず DateTime で行う (順位: 観測ツールの
# 再現性は数値の正確性に直結する)。
function ConvertTo-Utc([string]$iso) {
  [datetime]::Parse($iso, [System.Globalization.CultureInfo]::InvariantCulture,
    [System.Globalization.DateTimeStyles]::AssumeUniversal -bor [System.Globalization.DateTimeStyles]::AdjustToUniversal)
}
$sinceDt = ConvertTo-Utc $Since
$untilDt = ConvertTo-Utc $Until

$rows = New-Object System.Collections.Generic.List[object]
$runCount = 0

foreach ($dir in Get-ChildItem -Directory $RunsDir) {
  $metaPath = Join-Path $dir.FullName "meta.json"
  if (-not (Test-Path $metaPath)) { continue }
  # crashed / in-progress run の truncated・破損 meta.json は ConvertFrom-Json が throw する。
  # 1 件の破損 run で集計ループ全体を止めないよう、その run だけ skip する (下の phase 行 parse と同じ流儀)。
  try { $meta = Get-Content $metaPath -Raw | ConvertFrom-Json } catch { continue }
  if ($meta.piece -ne $Piece) { continue }
  # startTime は takt meta モデル上 Option (in-progress / クラッシュ run で欠損し得る)。
  # null を ConvertTo-Utc に渡すと全集計がクラッシュするため、その run だけ skip する。
  if (-not $meta.startTime) { continue }
  $startDt = ConvertTo-Utc $meta.startTime
  if ($startDt -lt $sinceDt -or $startDt -ge $untilDt) { continue }
  $logsDir = Join-Path $dir.FullName "logs"
  if (-not (Test-Path $logsDir)) { continue }
  $log = Get-ChildItem $logsDir -Filter *.jsonl | Select-Object -First 1
  if (-not $log) { continue }
  $runCount++

  $starts = @{}
  foreach ($line in Get-Content $log.FullName) {
    if ($line -notmatch '"phase_(start|complete)"') { continue }
    try { $o = $line | ConvertFrom-Json } catch { continue }
    $id = $o.phaseExecutionId
    if (-not $id) { continue }
    if ($o.type -eq "phase_start" -and $o.timestamp) { $starts[$id] = $o.timestamp }
    elseif ($o.type -eq "phase_complete" -and $o.timestamp -and $starts.ContainsKey($id)) {
      $secs = [math]::Round(([datetime]$o.timestamp - [datetime]$starts[$id]).TotalSeconds, 1)
      $rows.Add([pscustomobject]@{
        run = $dir.Name.Substring(0, 15); step = $o.step; phase = $o.phaseName; secs = $secs
      })
    }
  }
}

if ($rows.Count -eq 0) { Write-Output "piece=${Piece}: 対象 run/phase なし"; exit 0 }

Write-Output "## takt step/phase 別所要時間 (piece=$Piece, since=$Since, runs=$runCount)"
Write-Output ""
Write-Output "| step | phase | 回数 | avg(s) | median(s) | min(s) | max(s) | 合計占有(s) |"
Write-Output "|---|---|---|---|---|---|---|---|"
$rows | Group-Object step, phase | Sort-Object { ($_.Group | Measure-Object secs -Sum).Sum } -Descending | ForEach-Object {
  $g = $_.Group.secs | Sort-Object
  $m = $_.Group | Measure-Object secs -Average -Minimum -Maximum -Sum
  $mid = [int][math]::Floor($g.Count / 2)
  $median = if ($g.Count % 2 -eq 0) { [math]::Round(($g[$mid - 1] + $g[$mid]) / 2, 1) } else { $g[$mid] }
  $parts = $_.Name -split ", "
  "| {0} | {1} | {2} | {3} | {4} | {5} | {6} | {7} |" -f `
    $parts[0], $parts[1], $g.Count, [math]::Round($m.Average, 1), $median, $m.Minimum, $m.Maximum, [math]::Round($m.Sum, 0)
}

if ($PerRun) {
  Write-Output ""
  Write-Output "### run 別 (execute phase のみ)"
  Write-Output ""
  Write-Output "| run | step | execute(s) |"
  Write-Output "|---|---|---|"
  $rows | Where-Object phase -eq "execute" | Sort-Object run | ForEach-Object {
    "| {0} | {1} | {2} |" -f $_.run, $_.step, $_.secs
  }
}
