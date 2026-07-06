```markdown
# Refutation Report

## Verdict: ALL_REFUTED / SOME_SURVIVE

## Survived Findings (passed to fix)
| # | finding_id | Source | Severity | Location | Issue | Fix Suggestion | Evidence it reproduces |
|---|------------|--------|----------|----------|-------|----------------|------------------------|
| 1 | SEC-NEW-src-db-L42 | security | High | `src/db.ts:42` | Raw query string | Use a parameterized query | Confirmed by reading line 42: user input is interpolated into the SQL string |

## Rejected Findings (refuted -- audit log)
| # | finding_id | Source | Location | Original Issue | Rejection reason |
|---|------------|--------|----------|----------------|------------------|
| 1 | SIM-NEW-src-x-L10 | simplicity | `src/x.ts:10` | Dead-on-arrival helper | Refuted: helper is called from `src/y.ts:88`, so it is not dead |

## Verdict Gate
- `SOME_SURVIVE` requires at least one row in Survived Findings; those are the ONLY findings the fix step acts on.
- `ALL_REFUTED` means Survived Findings is empty (every reviewer finding was refuted).
- Every blocking finding from the reviewer reports (new / persists / reopened, each with a `finding_id`) MUST appear in exactly one of the two tables. Preserve the original `finding_id` verbatim so downstream audit can match it.
```

**Cognitive load reduction rules:**
- Nothing survives -> Verdict `ALL_REFUTED` + Rejected table only.
- Everything survives -> Verdict `SOME_SURVIVE` + Survived table only (empty Rejected table may be omitted).
- Mixed -> both tables.
