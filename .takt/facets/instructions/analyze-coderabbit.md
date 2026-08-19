# CodeRabbit Review Analysis (with Project Fitness Filter)

## Input

Read `.takt/review-comments.json`. This file contains the output from `check-ci-coderabbit.exe`, including:
- `ci`: GitHub Actions CI status (overall + per-run results)
- `coderabbit`: CodeRabbit review state (review_state, new_comments, actionable_comments, unresolved_threads)
- `findings`: Array of structured findings (severity, file, line, issue, suggestion, source)
- `action`: Terminal action from the monitor ("action_required", "stop_monitoring_success", etc.)
- `summary`: Human-readable summary

## Task

### Step 1: Read and parse
1. Read `.takt/review-comments.json` with the Read tool
2. Parse the `findings` array and `coderabbit` state

### Step 2: Project fitness filter
CodeRabbit sometimes raises findings that are not applicable to this project. Before classifying severity, evaluate each finding against the project context:

1. Read `CLAUDE.md` to understand the project's architecture decisions and constraints
2. **Determine if the PR is docs-only** under [ADR-035](../../../docs/adr/adr-035-doc-evaluation-policy.md): inspect the diff in `.takt/review-diff.txt`. The PR is docs-only when **all** changed files are `docs/**` / `*.md` / source-code doc comments / yaml comment-only, **and** no executable code logic changes. Excluded paths (`.takt/facets/instructions/**`, `.claude/**`, `.takt/workflows/**.yaml` structural changes, `docs/claude-code-web-tasks.md` = the nightly loop's task ledger per ADR-072) disqualify docs-only treatment even when the file extension is `.md`/`.yaml`
3. For each finding, check:
   - **Platform scope**: This project targets Windows only. Findings about cross-platform compatibility (e.g., `.exe` hardcoding) are NOT applicable -- downgrade to `Info`
   - **Intentional design**: Check if the finding contradicts an ADR decision. If so, mark as `not_applicable`
   - **Docs-only criteria mismatch (ADR-035)**: If the PR is docs-only AND the finding targets a criterion ADR-035 excludes (mutation / error handling / test coverage / function length / nesting depth / complexity metrics / DRY or YAGNI applied to code logic), mark as `not_applicable` with reason `"ADR-035 docs-only"`. Trust boundary / cross-reference integrity / markdown lint findings remain `applicable` even on docs-only PRs
   - **Sensitive-file protection** (Edit-blocked): If the finding targets `.claude/` (Claude Code sensitive-file protected — Edit/Write tool will refuse), mark as `user_decision_path` (NOT `not_applicable` — the issue may be real, but auto-fix cannot apply it)
   - **Scope mismatch**: If the finding targets a read-only zone (`.takt/`, `docs/adr/`, `templates/`) or a non-source path (`.git/`, `.jj/`, `node_modules/`, `target/`), mark as `not_applicable`
   - **False positive**: If the finding misunderstands the code logic, mark as `not_applicable`

Mark each finding as:
- `applicable` -- genuine issue that should be addressed
- `user_decision_path` -- finding is real but auto-fix is blocked by sensitive-file protection (`.claude/`); user decides
- `not_applicable` -- does not apply to this project (with reason)

### Step 3: Severity classification
For both `applicable` and `user_decision_path` findings, take the severity from CodeRabbit's `severity` field (do not reclassify):
- Critical > High > Major > Medium > Minor > Low > Info

The severity is preserved on `user_decision_path` findings so the user can prioritize their manual decision (a Critical `.claude/` finding still warrants attention even though auto-fix cannot apply it). For `not_applicable` findings, severity is irrelevant and may be omitted from the report.

### Step 4: Produce report and verdict

## Output Format

```markdown
## CodeRabbit Analysis Report

### Summary
- CI: [status]
- レビュー実施: 実施 (根拠: findings N 件 / actionable_comments=N) or **未実施 (陽性証拠なし)**
- CodeRabbit: [N] findings total, [M] applicable after filter
- Verdict: approved / needs_fix / user_decision

### Filtered Findings (not applicable)
| # | File (Line) | Issue | Filter Reason |
|---|-------------|-------|---------------|
| 1 | path:line   | ...   | Platform scope: Windows only |

### User Decision Path (sensitive-file protected)
| # | File (Line) | Severity | Issue | Path Reason |
|---|-------------|----------|-------|-------------|
| 1 | .claude/... | Major    | ...   | sensitive-file protection — auto-fix blocked |

### Applicable Findings by Severity

#### Critical / High / Major
| # | File (Line) | Issue | Recommended Action |
|---|-------------|-------|--------------------|
| 1 | path:line   | ...   | ...                |

#### Medium / Minor
| # | File (Line) | Issue | Recommended Action |
|---|-------------|-------|--------------------|

### Recommended Actions
1. [Prioritized action items for critical/major findings]
```

## Review evidence gate (check this BEFORE the verdict rules)

`approved` は「レビューが走った結果、直すものが無かった」という意味であり、**「レビューが走らなかった」は含まない**。両者は findings が空という同じ見え方をするため、区別せずに `approved` を出すと未レビューの PR を「指摘なし」と誤報する ([ADR-064](../../../docs/adr/adr-064-monitor-success-positive-evidence.md) の陽性証拠原則を、決定論層と同じ趣旨で本 facet にも適用する)。

**レビュー実施の陽性証拠**として採用してよいのは次の 2 つだけ:

- `findings` が 1 件以上ある
- `coderabbit.actionable_comments` が `null` でない (`0` を含む — 「レビューして 0 件だった」は陽性証拠)

どちらも**今サイクルの CR 出力に限定されている** — 上流の `check-ci-coderabbit` が `parse_actionable_comments` / `parse_new_comments` で `push_time` 以降のものだけを数えるため、過去 push に対するレビュー記録は入り込まない。「現在の head に対してレビューが走った証拠を要求する」という原則は `pr-monitor.yml` の GHA backstop と共通で、判定材料 (あちらは `reviews[].commit_id` と head SHA の照合) が層ごとに違うだけである。

次は**証拠に採用しない**:

- `coderabbit.new_comments > 0` — rate-limit 通知やコマンド応答など、レビュー以外の CR コメントでも増える。本 facet が起動する条件そのものでもあるため、これを証拠に数えると gate が常に素通りになる
  - **決定論層の `has_review_evidence()` (`src/check-ci-coderabbit/src/decide.rs`) は `new_comments > 0` を証拠に採用しており、本 facet は意図的にそれより厳しい。** 両者は判定の重みが違う — 決定論層が誤ると `continue_monitoring` に倒れる (監視を続けるだけで、外しても回復する) のに対し、本 facet の `approved` は**人間に示す終端 verdict** で、外すと未レビューの PR が「指摘なし」として読まれたまま残る。どちらかに寄せる場合は、この非対称を崩していないか確かめること
- `coderabbit.unresolved_threads > 0` — 過去サイクルの残骸を含み、今回レビューが走った証拠にならない (ADR-064 と同じ理由)
- `coderabbit.review_state` の値や CI の緑 — **check が pass でもレビュー実施の根拠にはならない**。これが順位 320 で実観測した誤報の形

**陽性証拠がどちらも無い場合、`approved` を出してはならない。** verdict は `user_decision` とし、レポートの Summary に「CodeRabbit のレビュー未実施 (指摘 0 件ではなく、レビューが走った証拠が無い)」と明記する。`needs_fix` の条件 (applicable な Critical/High/Major) を満たす場合はそちらが優先される — 指摘があるなら証拠は成立している。

## Verdict Rules (3-way)

> 下記は**上記 review evidence gate を通過した場合の**判定である。verdict は 3 値のまま増やさない (`post-pr-review.yaml` の `rules.condition` がリテラル照合するため)。

- **approved**: No applicable findings, OR all applicable findings are Info/Low severity
  - Output: `approved` condition
- **needs_fix**: Any applicable Critical, High, or Major finding exists (excluding `user_decision_path`)
  - Output: `needs_fix` condition
  - These will be automatically fixed in the next step
- **user_decision**: Only Medium or lower applicable findings exist, OR all remaining findings are `user_decision_path` (sensitive-file protected) regardless of severity
  - Output: `user_decision` condition
  - These are reported but NOT auto-fixed; the user decides
  - **Important**: A `.claude/` finding of any severity routes here to prevent fix loop pathology (auto-fix would attempt 4+ Edit calls all blocked by sensitive-file protection, wasting iterations)

## Important

- Do NOT modify any code. This is analysis only.
- Do NOT fabricate findings. Report only what is in the JSON.
- Do NOT skip the fitness filter. Every finding must be evaluated for project applicability.
- If the findings array is empty, first apply the **review evidence gate** above. With evidence, report "No actionable findings" with verdict `approved`; without evidence, report the review as not performed with verdict `user_decision`.
- If the JSON file is missing or empty, report the error and exit.
- When this is a re-analysis after a fix iteration, compare with previous reports to check for regression or persistence.

## 出力言語

- **レポート本文は日本語で書く。** コード識別子・ファイルパス・ADR 番号・コマンドはもちろん、**本 facet が出力する固定トークンも訳さない** — verdict の値 `approved` / `needs_fix` / `user_decision` (§ Verdict Rules)、および § Output Format の section 見出しと表の列名。`post-pr-review.yaml` の `rules.condition` がこの 3 値を英語リテラルで照合しており、訳すと分岐が成立しない
