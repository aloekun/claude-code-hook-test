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
2. For each finding, check:
   - **Platform scope**: This project targets Windows only. Findings about cross-platform compatibility (e.g., `.exe` hardcoding) are NOT applicable -- downgrade to `Info`
   - **Intentional design**: Check if the finding contradicts an ADR decision. If so, mark as `not_applicable`
   - **Scope mismatch**: If the finding targets a read-only zone (`.takt/`, `docs/adr/`, `templates/`), mark as `not_applicable`
   - **False positive**: If the finding misunderstands the code logic, mark as `not_applicable`

Mark each finding as:
- `applicable` -- genuine issue that should be addressed
- `not_applicable` -- does not apply to this project (with reason)

### Step 3: Severity classification
For `applicable` findings only, classify by severity:
- Critical > High > Major > Medium > Minor > Low > Info

### Step 4: Produce report and verdict

## Output Format

```markdown
## CodeRabbit Analysis Report

### Summary
- CI: [status]
- CodeRabbit: [N] findings total, [M] applicable after filter
- Verdict: approved / needs_fix / user_decision

### Filtered Findings (not applicable)
| # | File (Line) | Issue | Filter Reason |
|---|-------------|-------|---------------|
| 1 | path:line   | ...   | Platform scope: Windows only |

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

## Verdict Rules (3-way)

- **approved**: No applicable findings, OR all applicable findings are Info/Low severity
  - Output: `approved` condition
- **needs_fix**: Any applicable Critical, High, or Major finding exists
  - Output: `needs_fix` condition
  - These will be automatically fixed in the next step
- **user_decision**: Only Medium or lower applicable findings exist (no Critical/High/Major)
  - Output: `user_decision` condition
  - These are reported but NOT auto-fixed; the user decides

## Important

- Do NOT modify any code. This is analysis only.
- Do NOT fabricate findings. Report only what is in the JSON.
- Do NOT skip the fitness filter. Every finding must be evaluated for project applicability.
- If the findings array is empty, report "No actionable findings" with verdict `approved`.
- If the JSON file is missing or empty, report the error and exit.
- When this is a re-analysis after a fix iteration, compare with previous reports to check for regression or persistence.
