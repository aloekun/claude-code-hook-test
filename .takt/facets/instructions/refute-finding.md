<!-- Used by: pre-push-review-refute workflow (verify step). Experimental (WP-06 / ADR-047). -->

Refute the findings raised by the pre-push reviewers. This is an **adversarial verification** step: your job is to try to *disprove* each finding by reading the actual code, so that false positives never reach the expensive `fix` step.

## Input: reviewer reports

The reviewers (`simplicity-review` + `security-review`) have written their findings to the Report Directory. Read both:

- `<Report Directory>/simplicity-review.md`
- `<Report Directory>/security-review.md`

Only findings listed under **new / persists / reopened** (blocking findings that carry a `finding_id`) are in scope. Warnings and non-blocking notes are not your concern -- ignore them.

## The diff under review

The authoritative diff is `.takt/review-diff.txt` (pre-collected by push-runner). Read it, then Read the actual source files each finding points to. Do NOT run `jj diff` / `git diff` yourself.

## Refutation procedure (per finding)

For each blocking finding, attempt to refute it:

1. Read the exact location the finding points to (`file:line`) in the current working tree.
2. **Reproduces?** Does the described problem actually exist in the code as written?
3. **Premise holds?** Does the referenced symbol / caller / invariant actually exist and behave as the reviewer assumed? A finding built on a false premise (e.g. "symbol X is undefined" when X is defined elsewhere in scope) is refuted.
4. Decide:
   - **reject (refuted)** -- the finding does not reproduce, its premise is false, **or you cannot confirm it is real with code-level confidence**.
   - **survive** -- you confirmed, by reading the code, that the finding reproduces AND its premise holds.

## Bias: when uncertain, reject

This step exists to cut false-positive-driven fix iterations. The cost is asymmetric:

- A wrongly **survived** finding costs a full `fix -> reviewers` cycle (expensive on pre-push).
- A wrongly **rejected** finding is recaught downstream by the post-pr CodeRabbit layer (the safety net).

Therefore **the tie goes to reject**: only let a finding survive when you have positive, code-level evidence that it is real. If after reading the code you are still unsure, reject it.

## Output

Write `refutation-report.md` following its output contract:

- **Survived Findings** -- carry over the original `finding_id`, source, severity, location, issue, **and the reviewer's original fix suggestion**, and add the code-level **evidence it reproduces**. These are the only findings `fix` acts on, and `fix` reads this table as its work list, so make each row **self-contained**: a reader must be able to fix from the row alone (the `finding_id` lets them pull extra context from the reviewer report if needed).
- **Rejected Findings** -- carry over `finding_id`, source, location, and original issue, plus a concrete **rejection reason** (what you read that disproves it). This table is the audit log for dogfood measurement (reject rate / reject-error rate vs. later CodeRabbit re-findings), so make each reason specific and checkable.
- **Verdict**: `ALL_REFUTED` when the Survived table is empty, otherwise `SOME_SURVIVE`.

Every blocking finding from the reports must land in exactly one of the two tables. Do NOT edit any source code -- this step only reads and judges.
