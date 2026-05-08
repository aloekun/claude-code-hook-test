Focus on **anomaly detection** in the changed diff -- patterns that look unusual, unexplained, or out of step with the surrounding codebase. Do NOT enumerate against a fixed checklist; the deterministic layer already handles structural metrics.

## Obtaining the diff

The diff has been pre-collected by push-runner (Rust exe) and saved to `.takt/review-diff.txt`.
**Read this file first** using the Read tool. This is the authoritative review target.
Do NOT run `git diff` or `jj diff` yourself -- the file already contains the correct diff scope.

### Optional: lint-screen pre-pass (Phase c §8.E, ADR-038 試験運用)

If `.takt/lint-screen-report.md` exists, push-runner has already run a mistral:7b lint pre-pass on the diff. Read this file as **supplementary context** (treat as advisory, not authoritative):

- Coverage: rule names from a fixed canonical list (`unused-import` / `no-var` / `no-unused-vars` / `magic-number` / `dead-code` / `deep-nesting` / `complexity`)
- Quality: agreement 75% with Claude baseline (Phase b' conditional GO) — false positives and recall misses are expected
- Use it to:
  - Cross-check anomalies you already noticed (consensus signal)
  - **Skip dimensions** the lint-screen already covered (avoid duplicate findings of unused-import / magic-number etc.)
- Do NOT use it to:
  - Adopt findings verbatim without diff verification
  - Override your own judgment on subjective anomalies (deep-nesting boundary, complexity)

If the report shows `screen_decision: informational` and zero findings, that is a **weak signal of a clean diff** — still review yourself, but you can be more concise in approval rationale.

## Determinism layer guarantees (do NOT duplicate)

The following dimensions are enforced by deterministic hooks at write time and by `fix-metrics-check.ps1` during fix iterations. Skip them — flagging them duplicates the deterministic layer and produces noise:

- **Comment policy** (Bundle Z #B-α / `hooks-post-tool-comment-lint-rust`): Non-doc comments are blocked at PostToolUse. Existing comments in the diff have already passed the allowlist (`// SAFETY:` / `// TODO:` / rustdoc etc.).
- **Function length** (順位 48, same hook): Functions >50 lines are blocked at write time (touch-trigger ratchet, grandfathered until touched). New >50 functions or growth past 50 cannot land in changed regions.
- **Function metrics during fix** (Bundle Z #B-β / `fix-metrics-check.ps1`): non-doc comment count, function length, max nesting depth cannot increase per function during fix iterations. Pre/post comparison enforces this structurally.

Reviewing these dimensions is duplicative. Skip them.

## Anomaly criteria (subjective judgment required)

Read the diff straight through. Note any pattern that prompted "this looks unusual / unexpected / hard to explain" — patterns deterministic checks cannot catch:

- **Unexplained complexity**: Logic choices with no obvious motivation given the surrounding code; algorithm complexity that seems disproportionate to the problem
- **Inconsistent style**: Naming or structural patterns that diverge from neighboring code without rationale
- **Dead-on-arrival code**: Branches, parameters, or abstractions with no apparent caller or use site
- **Hidden coupling**: Changes that silently depend on global state, environment, ordering, or undocumented invariants
- **Missing failure paths**: Operations that can fail (I/O, parse, network, optional unwrap) with no visible error handling
- **Non-obvious magic values**: Numeric or string literals whose meaning isn't clear from context

For each anomaly, articulate **what looks unusual**, **why it caught your attention**, and **what alternative would be expected**. If you cannot articulate the "why", it likely isn't an anomaly worth flagging.

## Scope constraint

Review primarily within the changed diff. **Limited** cross-file lookups are permitted only to *verify* an anomaly already raised by the diff (e.g., confirming a hidden coupling, checking whether a referenced symbol exists, distinguishing dead-on-arrival code from a legitimate caller elsewhere). Do NOT use this allowance to expand into project-wide architecture review, unrelated call chains, or speculative exploration. Every anomaly finding must still be traceable to a specific hunk in the diff — cross-file evidence supports the finding, it does not become its own finding.

## Scope of DRY / YAGNI (do NOT raise findings outside this scope)

The DRY and YAGNI dimensions in anomaly detection apply **only to executable code logic**.

- **DRY scope**: Flag duplicated *code logic* (copy-paste functions, repeated control flow, redundant computations). Do NOT flag duplication that is documentation, doc-vs-code restatement, or test independence.
- **YAGNI scope**: Flag *speculative code abstractions* (unused parameters, premature interfaces, over-engineered patterns in production code). Do NOT flag planning-document "future candidates" / "Phase 2 検討" / ADR rejected-alternative sections, or comments documenting known constraints.

If a finding cannot be tied to executable code logic, it is out of scope. See [ADR-035: docs-only PR 評価ポリシー](../../../docs/adr/adr-035-doc-evaluation-policy.md) for the full list of criteria that do NOT apply to docs-only diffs (mutation / error handling / test coverage / function length / DRY / YAGNI all fall under this).

## Calibration: avoid over-narrowing

The shift to anomaly detection is meant to remove the duplicative checklist work, not to skip review. If reading the diff leaves you with a concrete unease that you can articulate, raise it — even if it doesn't fit a named criterion. Conversely, if you can only flag something by mechanically applying a rule, the deterministic layer already handles that case.

## Judgment procedure

1. Read the diff from `.takt/review-diff.txt`
2. Read straight through. After the first pass, list any pattern that read as "unusual / unexpected / hard to explain"
3. For each anomaly, classify as blocking (significant unexplained risk) or non-blocking (worth raising but not a blocker)
4. If there is even one blocking anomaly, judge as REJECT
