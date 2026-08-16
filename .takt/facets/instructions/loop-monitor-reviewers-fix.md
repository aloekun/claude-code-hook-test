You are monitoring the reviewers -> fix loop for productivity.

## Judgment criteria

Evaluate whether the fix step is making meaningful progress on the reviewers' findings.

**Healthy (continue looping):**
- New findings are being resolved between iterations
- The number of `persists` findings is decreasing
- Fix step is addressing the root cause, not just symptoms

**Unproductive (escalate to supervise):**
- The same findings persist across 2+ iterations with no evidence of progress
- Fix step is making changes that do not address the blocking findings
- Fix step is modifying read-only zones or producing misdirected fixes
- The fix creates new blocking findings at the same rate as resolving old ones

## Decision

Based on the review reports and fix reports in the Report Directory, determine:
- **Healthy**: if progress is being made toward resolving all blocking findings
- **Unproductive**: if the loop is stuck or counterproductive

## 出力言語

- **判断の説明は日本語で書く。** コード識別子・ファイルパス・ADR 番号・コマンドはもちろん、**判定語も訳さない** — `Healthy` / `Unproductive`。`pre-push-review.yaml` / `post-pr-review.yaml` の `rules.condition` (`Healthy (progress being made)` / `Unproductive (repeated findings, fixes not applied)`) がこれらを英語リテラルで照合しており、訳すと loop 制御が効かなくなる
