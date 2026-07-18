<!-- Used by: pre-push-review (all review steps). Shadows the takt builtin `review` policy for pre-push only. See ADR-056. The refute variant (ADR-047) was retired 2026-07-19. -->

# Review Policy (anomaly mode)

Shared judgment criteria for the pre-push review steps (reviewers /
supervisor). This policy deliberately defines **no REJECT checklist of its own**.
What counts as a blocking problem is owned by each step's instruction facet; this
policy only constrains **how** a finding must be evidenced, scoped, and tracked.

Rationale: the deterministic layer intercepts structural violations at write time
(PostToolUse lint hooks) and during fix iterations (`fix-metrics-check.ps1`), so a
reviewer-side checklist duplicates a layer that already ran and turns unremarkable
code into fix iterations. See ADR-036 (three-layer review) and ADR-056 (this policy).

## Principles

| Principle | Criteria |
|-----------|----------|
| Fact-check | Verify against the actual code before raising anything. Never speculate |
| Eliminate ambiguity | "Clean this up a bit" is prohibited. Give file, line, and a proposed fix |
| Practical fixes | Propose implementable changes, not theoretical ideals |
| Articulable concern | If you cannot state what looks unusual and why, it is not a finding |

## Where REJECT criteria come from

Each step's instruction facet defines what qualifies as blocking:

- `review-simplicity` — an articulable anomaly (unexplained complexity, hidden coupling, dead-on-arrival code, ...)
- `review-security` — a concrete exploit path (who controls the input, what newly becomes possible)
- `supervise` — the current iteration's blocking findings are resolved

This policy adds none of its own. In particular there is **no list of "REJECT
without exception" patterns**: DRY violations, TODO comments, unused code, fallback
values, and similar named patterns are grounds for a finding only when the step's
own criteria are met and the evidence rules below are satisfied. Structural metrics
alone (file length, duplication count, nesting depth, comment style) are never
sufficient grounds — the deterministic layer owns them.

## Scope Determination

| Situation | Verdict |
|-----------|---------|
| Problem introduced by this change | Blocking |
| Code made unused *by this change* (arguments, imports, branches) | Blocking — change-induced |
| Pre-existing problem in a changed file | Non-blocking — record only |
| Problem in an unchanged file | Non-blocking — record only |
| Refactoring beyond the task scope | Non-blocking — note as a suggestion |

"It sits in a file this change touched" does not make a pre-existing problem
blocking. Opportunistic cleanup of surrounding code is out of scope for pre-push
review: that judgment needs whole-PR context and belongs to the post-PR layer
(ADR-019 / ADR-027).

## APPROVE

Approve when no blocking finding remains. **Non-blocking warnings do not block
approval** — record them and approve. Warnings ride downstream to the post-PR
layer rather than gating the push.

## Fact-Checking

| Do | Do Not |
|----|--------|
| Open the file and read the actual code | Assume "it should be fixed already" |
| Grep for call sites before calling code dead | Raise issues from memory |
| Cross-reference type definitions and schemas | Guess that a premise holds |
| Distinguish generated files (reports) from source | Review generated files as if they were source |

## Writing Specific Feedback

Every finding must state:

- **Which file and line**
- **What the problem is**, and why it read as unusual
- **How to fix it**
- **If proposing consolidation or abstraction, why that placement is the natural one**

```text
❌ "Review the structure"
❌ "Refactoring is needed"

✅ "src/auth/service.ts:45 — validateUser() is duplicated in 3 places.
     Extract into a shared function."
```

## Finding ID Tracking (`finding_id`)

Findings are tracked by ID so that iterations converge instead of circling.

- Every blocking finding carries a `finding_id`
- The same problem reuses the same `finding_id` across iterations — status is conveyed by the table a finding appears in, not by its id
- A `finding_id` means one and only one problem. If the problem, its evidence, or its reproduction conditions change, issue a new id
- Findings without a `finding_id` are invalid and cannot be used as REJECT grounds
- REJECT is valid only when at least one finding is `new`, `persists`, or `reopened`

## Reopen Conditions (`resolved` -> open)

Reopening a resolved finding requires all three:

1. Reproduction steps (command / input)
2. Expected result vs. actual result
3. Failing file / line evidence

If any of the three is missing, the reopen is invalid. If the reproduction
conditions changed, it is a different problem — issue a new `finding_id`.

## Recurring Findings

If the same finding keeps recurring across iterations, the fix instruction itself
is likely the problem. Propose a different approach rather than repeating the same
instruction.
