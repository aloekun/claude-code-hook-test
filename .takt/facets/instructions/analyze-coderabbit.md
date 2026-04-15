# CodeRabbit Review Analysis

## Input

Read `.takt/review-comments.json`. This file contains the output from `check-ci-coderabbit.exe`, including:
- `ci`: GitHub Actions CI status (overall + per-run results)
- `coderabbit`: CodeRabbit review state (review_state, new_comments, actionable_comments, unresolved_threads)
- `findings`: Array of structured findings (severity, file, line, issue, suggestion, source)
- `action`: Terminal action from the monitor ("action_required", "stop_monitoring_success", etc.)
- `summary`: Human-readable summary

## Task

1. Read the JSON file with the Read tool
2. Parse the `findings` array and `coderabbit` state
3. Classify each finding by severity: Critical > High > Major > Medium > Minor > Low > Info
4. Group findings by file path
5. For each Critical/High/Major finding, provide:
   - Root cause analysis (why this is a problem)
   - Recommended fix approach
   - Impact if not addressed

## Output Format

Produce a structured Markdown report:

```markdown
## CodeRabbit Analysis Report

### Summary
- CI: [status]
- CodeRabbit: [N] findings ([X] critical/high, [Y] medium, [Z] low)
- Verdict: PASS / FAIL

### Findings by Severity

#### Critical / High
| # | File (Line) | Issue | Recommended Action |
|---|-------------|-------|--------------------|
| 1 | path:line   | ...   | ...                |

#### Medium
...

#### Low / Info
...

### Recommended Actions
1. [Prioritized action items for critical/high findings]
```

## Verdict Rules

- **FAIL**: Any Critical or High or Major severity finding exists
- **PASS**: Only Medium or lower severity findings (or no findings)

## Important

- Do NOT modify any code. This is analysis only.
- Do NOT fabricate findings. Report only what is in the JSON.
- If the findings array is empty, report "No actionable findings" with verdict PASS.
- If the JSON file is missing or empty, report the error and exit.
