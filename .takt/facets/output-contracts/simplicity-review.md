```markdown
# Simplicity Review

## Result: APPROVE / REJECT

## Summary
{1-2 sentence anomaly-scan result}

## Anomaly Scan
| Category | Result | Notes |
|----------|--------|-------|
| Unexplained complexity | ✅ | - |
| Inconsistent style | ✅ | - |
| Dead-on-arrival code | ✅ | - |
| Hidden coupling | ✅ | - |
| Missing failure paths | ✅ | - |
| DRY / YAGNI (code logic) | ✅ | - |

## Current Iteration Findings (new)
| # | finding_id | family_tag | Severity | Type | Location | Issue | Fix Suggestion |
|---|------------|------------|----------|------|----------|-------|----------------|
| 1 | SIM-NEW-src-x-L10 | dead-code | Medium | dead-on-arrival | `src/x.ts:10` | Helper with no caller | Remove or wire up |

## Carry-over Findings (persists)
| # | finding_id | family_tag | Previous Evidence | Current Evidence | Issue | Fix Suggestion |
|---|------------|------------|-------------------|------------------|-------|----------------|
| 1 | SIM-PERSIST-src-y-L30 | deep-nesting | `src/y.ts:30` | `src/y.ts:30` | Nesting persists | Flatten with guard clause |

## Resolved Findings (resolved)
| finding_id | Resolution Evidence |
|------------|---------------------|
| SIM-RESOLVED-src-x-L10 | `src/x.ts:10` helper removed |

## Reopened Findings (reopened)
| # | finding_id | family_tag | Prior Resolution Evidence | Recurrence Evidence | Issue | Fix Suggestion |
|---|------------|------------|--------------------------|---------------------|-------|----------------|
| 1 | SIM-REOPENED-src-y-L55 | deep-nesting | `Previously flattened at src/y.ts:30` | `Recurred at src/y.ts:55` | Nesting reintroduced | Flatten again |

## Warnings (non-blocking)
- {Non-blocking simplicity notes}

## Rejection Gate
- REJECT is valid only when at least one finding exists in `new`, `persists`, or `reopened`
- Findings without `finding_id` are invalid
- `finding_id` is immutable: a finding keeps the same id as it moves across the `new` / `persists` / `resolved` / `reopened` tables (the table conveys status, not the id — the `SIM-NEW-` / `SIM-PERSIST-` prefixes in the examples above are illustrative only)
- `Type` is the anomaly category (e.g. dead-on-arrival, hidden-coupling, unexplained-complexity, inconsistent-style, missing-failure-path)
```

**Cognitive load reduction rules:**
- No anomalies → Anomaly Scan table only (10 lines or fewer)
- Warnings only → + Warnings in 1-2 lines (15 lines or fewer)
- Anomalies found → + finding tables (30 lines or fewer)
