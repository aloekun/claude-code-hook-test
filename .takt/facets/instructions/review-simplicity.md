Focus on **anomaly detection** in the changed diff -- patterns that look unusual, unexplained, or out of step with the surrounding codebase. Do NOT enumerate against a fixed checklist; the deterministic layer already handles structural metrics.

## Obtaining the diff

The diff has been pre-collected by push-runner (Rust exe) and saved to `.takt/review-diff.txt`.
**Read this file first** using the Read tool. This is the authoritative review target.
Do NOT run `git diff` or `jj diff` yourself -- the file already contains the correct diff scope.

## Determinism layer guarantees (do NOT duplicate)

The following dimensions are enforced by deterministic hooks at write time and by `hooks-post-tool-comment-lint-rust --fix-metrics-check` during fix iterations. Skip them — flagging them duplicates the deterministic layer and produces noise:

- **Comment policy** (Bundle Z #B-α / `hooks-post-tool-comment-lint-rust`): Non-doc comments are blocked at PostToolUse. Existing comments in the diff have already passed the allowlist (`// SAFETY:` / `// TODO:` / rustdoc etc.).
- **Function length** (順位 48, same hook): Functions >50 lines are blocked at write time (touch-trigger ratchet, grandfathered until touched). New >50 functions or growth past 50 cannot land in changed regions.
- **Function metrics during fix** (Bundle Z #B-β / `hooks-post-tool-comment-lint-rust --fix-metrics-check`): non-doc comment count, function length, max nesting depth cannot increase per function during fix iterations. Pre/post comparison enforces this structurally.

Reviewing these dimensions is duplicative. Skip them.

## Anomaly criteria (subjective judgment required)

Read the diff straight through. Note any pattern that prompted "this looks unusual / unexpected / hard to explain" — patterns deterministic checks cannot catch:

- **Unexplained complexity**: Logic choices with no obvious motivation given the surrounding code; algorithm complexity that seems disproportionate to the problem
- **Inconsistent style**: Naming or structural patterns that diverge from neighboring code without rationale
- **Dead-on-arrival code**: Branches, parameters, or abstractions with no apparent caller or use site (before flagging, check "PR chain declarations" below -- a declared successor PR is a legitimate caller-to-be)
- **Hidden coupling**: Changes that silently depend on global state, environment, ordering, or undocumented invariants
- **Missing failure paths**: Operations that can fail (I/O, parse, network, optional unwrap) with no visible error handling
- **Non-obvious magic values**: Numeric or string literals whose meaning isn't clear from context

For each anomaly, articulate **what looks unusual**, **why it caught your attention**, and **what alternative would be expected**. If you cannot articulate the "why", it likely isn't an anomaly worth flagging.

## Scope constraint

Review primarily within the changed diff. **Limited** cross-file lookups are permitted only to *verify* an anomaly already raised by the diff (e.g., confirming a hidden coupling, checking whether a referenced symbol exists, distinguishing dead-on-arrival code from a legitimate caller elsewhere). Do NOT use this allowance to expand into project-wide architecture review, unrelated call chains, or speculative exploration. Every anomaly finding must still be traceable to a specific hunk in the diff — cross-file evidence supports the finding, it does not become its own finding.

## Scope of DRY / YAGNI (do NOT raise findings outside this scope)

The DRY and YAGNI dimensions in anomaly detection apply **only to executable code logic**.

- **DRY scope**: Flag duplicated *code logic* (copy-paste functions, repeated control flow, redundant computations). Do NOT flag duplication that is documentation, doc-vs-code restatement, or test independence.
- **YAGNI scope**: Flag *speculative code abstractions* (unused parameters, premature interfaces, over-engineered patterns in production code). Do NOT flag planning-document "future candidates" / "Phase 2 検討" / ADR rejected-alternative sections, or comments documenting known constraints. For abstractions whose consumer is a **declared successor PR**, see "PR chain declarations" below.

If a finding cannot be tied to executable code logic, it is out of scope. See [ADR-035: docs-only PR 評価ポリシー](../../../docs/adr/adr-035-doc-evaluation-policy.md) for the full list of criteria that do NOT apply to docs-only diffs (mutation / error handling / test coverage / function length / DRY / YAGNI all fall under this).

## PR chain declarations (ADR-069)

Multi-PR chains (the PR size gate forces features >1500 lines to split) necessarily produce **leading PRs that introduce infrastructure whose consumer arrives in a successor PR**. Treating every missing consumer as blocking would structurally reject every chain's leading PR, so chain-declared items get a narrower treatment:

- **When the declaration is valid**, missing-consumer findings (dead-on-arrival code, premature interfaces / abstractions) against the **declared items** are **non-blocking warnings**, not REJECT grounds. Still record them in Warnings so the chain's tail stays auditable (if the successor never lands, the next reviewer sees the trail).
- **A valid declaration** must satisfy all of:
  1. It lives in a **planning document inside this diff** (e.g. the plan doc / a `docs/todoN.md` entry updated in the same PR). Planning documents outside the diff do NOT qualify, even when module docs reference them -- an out-of-diff document was not reviewed as part of this change, so it can be stale or self-servingly pre-written, and accepting it would let an unreviewed file relax this review.
  2. It names the **successor PR and the specific pairing**: which new crate / exe / function will be consumed by which planned change. Generic "will be used later" does not qualify.
  3. The declared names **match the code**: the crate / exe / function names in the declaration must equal those in the diff. A declaration that contradicts the diff (the 2026-08-02 case: the in-diff plan doc described wiring to a *different* exe than the one the comments promised) does NOT downgrade -- flag it as blocking, citing the contradiction.
- **Fail-closed**: no declaration found, pairing not specific, or names mismatch → the finding stays blocking as usual. This section narrows nothing for undeclared speculation.
- **Fix Suggestion ordering (ADR-068 carry-over)**: when you do raise a finding that admits multiple remedies, list the **least destructive remedy first** (e.g. "correct the declaration/comment" before "defer/remove the abstraction"). The fix step follows suggestion order, and a most-destructive-first ordering caused a gut-revert incident (2026-08-02).

## Calibration: avoid over-narrowing

The shift to anomaly detection is meant to remove the duplicative checklist work, not to skip review. If reading the diff leaves you with a concrete unease that you can articulate, raise it — even if it doesn't fit a named criterion. Conversely, if you can only flag something by mechanically applying a rule, the deterministic layer already handles that case.

## Judgment procedure

1. Read the diff from `.takt/review-diff.txt`
2. Read straight through. After the first pass, list any pattern that read as "unusual / unexpected / hard to explain"
3. For each anomaly, classify as blocking (significant unexplained risk) or non-blocking (worth raising but not a blocker)
4. If there is even one blocking anomaly, judge as REJECT
