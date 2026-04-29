Focus on reviewing **code simplicity** within the changed diff only.

## Obtaining the diff

The diff has been pre-collected by push-runner (Rust exe) and saved to `.takt/review-diff.txt`.
**Read this file first** using the Read tool. This is the authoritative review target.
Do NOT run `git diff` or `jj diff` yourself -- the file already contains the correct diff scope.

## Scope constraint

Review ONLY the lines changed in the diff. Do NOT explore cross-file dependencies, call chains, or project-wide architecture. Every finding must be traceable to a specific hunk in the diff.

## Review criteria (all diff-local)

- **Nesting depth**: Flag blocks nested >4 levels; suggest flattening via early returns or extraction
- **Function length**: Flag functions exceeding 50 lines
- **Early return opportunities**: Identify guard clauses that would reduce nesting
- **Redundant / duplicate code**: Flag copy-paste patterns or unnecessarily verbose logic within the diff
- **Magic numbers**: Flag unexplained numeric or string literals; suggest named constants
- **YAGNI violations**: Flag speculative abstractions, unused parameters, or over-engineered patterns that serve no current need
- **Naming clarity**: Flag ambiguous variable/function names that obscure intent

## Scope of DRY / YAGNI (do NOT raise findings outside this scope)

The DRY and YAGNI criteria above apply **only to executable code logic**.

- **DRY scope**: Flag duplicated *code logic* (copy-paste functions, repeated control flow, redundant computations). Do NOT flag:
  - Documentation hierarchies that intentionally restate context (e.g., a summary table followed by detailed bullet points)
  - Repetition between docs and code (docs explain, code executes — they serve different audiences)
  - Test code mirroring production code structure (test independence > test DRY)
- **YAGNI scope**: Flag *speculative code abstractions* (unused parameters, premature interfaces, over-engineered patterns in production code). Do NOT flag:
  - Planning documents listing "future candidates", "Phase 2 検討", or "out of scope but worth considering" sections — these capture design intent for shared understanding, not speculative implementation
  - ADR alternatives sections describing rejected options — these document the decision rationale
  - Comments documenting *known constraints or limitations* of the current implementation (these are not speculation; they are recorded reality)

If a finding cannot be tied to executable code logic, it is out of scope — do not raise it.

## Judgment procedure

1. Read the diff from `.takt/review-diff.txt`
2. For each changed hunk, check against the 7 criteria above
3. For each detected issue, classify as blocking (significantly harms readability/maintainability) or non-blocking (minor suggestion)
4. If there is even one blocking issue, judge as REJECT
