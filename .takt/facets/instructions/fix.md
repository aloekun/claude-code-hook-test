Use reports in the Report Directory and fix the issues raised by the reviewer.

## Read-only zones (ABSOLUTE -- violating these silently corrupts the review contract)

The `fix` step has broad `Edit` / `Write` / `Bash` permissions, but the following paths are **immutable inputs** to the workflow and MUST NEVER be edited, created under, deleted from, or moved by this step:

- **`.takt/runs/**`** -- Run-local report directories. The harness owns these.
- **`.takt/facets/**`**, **`.takt/workflows/**`**, **`.takt/config.yaml`** -- takt configuration assets.
- **`docs/adr/**`** -- Decision records are not review targets.
- **`templates/**`** -- Hook configuration templates.
- **`.claude/hooks-config.toml`** -- Hook configuration is not a review target.
- **`push-runner-config.toml`** -- Pipeline configuration is not a review target.

Your fixes MUST target the **source tree under review**: files under `src/` (Rust crates and TypeScript/Python sources). If a finding's suggested fix appears to require editing any file under the read-only zones listed above, treat it as a misdirected suggestion: report the mismatch in your `## Work results` section under a `### Misdirected finding` sub-heading and leave the read-only zone untouched.

If you catch yourself about to run a Bash command that writes into a read-only zone (including redirection like `>`, `>>`, `tee`, `sed -i` etc.), **stop**.

## Fix principles

- When a finding includes a "suggested fix", follow it rather than inventing your own workaround -- **except** when the suggestion targets a read-only zone; in that case, report the conflict and skip the fix.
- Fix the target code directly. Do not deflect findings by adding tests or documentation instead.

## Report reference policy

- Use the latest review reports in the Report Directory as primary evidence.
- Past iteration reports are saved as `{filename}.{timestamp}` in the same directory. For each report, run Glob with a `{report-name}.*` pattern, read up to 2 files in descending timestamp order, and understand persists / reopened trends before starting fixes.

## Completion criteria (all must be satisfied)

- All findings in this iteration (new / reopened) have been fixed in the correct source tree (not in any read-only zone).
- Potential occurrences of the same `family_tag` have been fixed simultaneously (no partial fixes that cause recurrence).

**Important**: After fixing, run the build and tests for the affected crate(s).

## `--ignored` integration test gate (conditional — REQUIRED when triggered)

If the fixes in this iteration did **either** of the following:

- modified any test file (any `.rs` file containing `#[test]` / `#[ignore]` attributes, or files under a `tests/` directory), or
- changed the behavior or signature of any `pub` / `pub(crate)` function,

you MUST also run the ignored integration tests and confirm PASS **before** emitting `convergence_verdict: fully_resolved`:

    cargo test -- --ignored --test-threads=1

Rationale: plain `cargo test` does NOT execute `#[ignore]` integration tests, and the automated push paths after this workflow may not re-run them before your changes reach the PR (PR #224: a fix to `create_fix_commit` broke 2 `#[ignore]` repush integration tests and landed on the PR unverified). If the trigger condition applies and the run failed — or you did not run it — emit `convergence_verdict: partial` instead.

## Pre-completion deterministic check (Bundle Z Phase 2 / #B-β)

For each `.rs` file modified in this iteration, run:

    pwsh -NoProfile -File scripts/fix-metrics-check.ps1 <relative_file_path>

The helper compares pre-fix (`@-`) vs post-fix (working copy) and exits non-zero if any of these increased:

- file-level non-doc comment count (`///`, `//!`, `// TODO:` etc. are excluded — see `src/hooks-post-tool-comment-lint-rust/src/main.rs` `ALLOWED_LINE_PREFIXES` / `ALLOWED_BLOCK_PREFIXES`)
- per-function length in lines
- per-function max nesting depth (block depth inside function body)

**On exit 1 (`metrics_check: fail`)**: read the violations JSON, then either:

- **Refactor** (preferred): extract function, early return / `let ... else`, flatten `match` arms, guard clauses
- **Override** (only if incidental to fix): document under `## Work results` → `### Metrics override` sub-heading with the function name, metric, pre/post values, and reasoning

**On exit 2** (infrastructure error, e.g., exe missing or jj revset failure): not a fix-quality issue — surface it under `## Work results` so the user can investigate, but do not block fix completion.

New files (post-only) are reported `metrics_check: skipped` and do not block fix completion. Markdown / yaml / PowerShell etc. files are not in scope (Rust-only PoC).

## Pre-completion diff refresh (REQUIRED — fix→review iteration freshness)

After completing all fixes (Edit/Write operations) AND before emitting the `convergence_verdict` line, refresh `.takt/review-diff.txt` so the next reviewer iteration (if `convergence_verdict: partial`) sees the post-fix state:

    jj diff -r @ > .takt/review-diff.txt

This refresh is **unconditional**:

- **`convergence_verdict: partial`** — critical. Without refresh, reviewers in the next iteration read the pre-fix snapshot taken by `cli-push-runner` Stage 1.5 and produce false-positive `persists` findings on already-fixed code, escalating the loop to 6-iter outliers (PR #103 observation).
- **`convergence_verdict: fully_resolved`** — harmless. The next workflow step is COMPLETE, so the refreshed file is not consumed; the ~1s `jj diff` cost is negligible.

`.takt/review-diff.txt` is **not** in any read-only zone (the read-only list at the top of this instruction covers `.takt/runs/**`, `.takt/facets/**`, `.takt/workflows/**`, `.takt/config.yaml` — `.takt/review-diff.txt` lives at the `.takt/` root and is an explicitly allowed write target for this step).

## Required output (include headings)

## Work results
- {Summary of actions taken}

### Read-only zone compliance
- {Confirm no writes attempted under read-only zones}

## Changes made
- {Summary of changes, listing the exact file paths modified}

## Build results
- {Build execution results}

## Test results
- {Test commands executed and results — list each command line explicitly, including `cargo test -- --ignored --test-threads=1` when the conditional gate above applies}

## Convergence gate

| Metric | Count |
|--------|-------|
| new (fixed in this iteration) | {N} |
| reopened (recurrence fixed) | {N} |
| persists (carried over, not addressed this iteration) | {N} |
| misdirected (suggestion pointed at a read-only zone, skipped) | {N} |

## Convergence verdict (REQUIRED — Phase 3 / #C-2 fix-trust shortcut)

After completing fixes, evaluate the gate above and emit one of two verdicts. The next workflow step is selected from this verdict, so it must accurately reflect the gate state.

- **fully_resolved** — `persists == 0` AND `misdirected == 0`. All findings of this iteration were either fixed or correctly skipped. No remaining work for the analyze step to re-examine. When the "`--ignored` integration test gate" trigger condition applies, a PASS of `cargo test -- --ignored --test-threads=1` is an additional precondition for this verdict.
- **partial** — `persists > 0` OR `misdirected > 0`. Some findings carried over (still need fixing in a later iteration) or were skipped due to misdirection (and need to be reported). Re-analysis is required.

Place the verdict at the **end of your report** as a single bare line in this exact form (no surrounding quotes, no trailing punctuation):

```text
convergence_verdict: fully_resolved
```

or:

```text
convergence_verdict: partial
```

**Honesty constraint**: This verdict gates whether the analyze step runs again. Reporting `fully_resolved` while leaving findings unaddressed bypasses the safety re-check. If you are uncertain whether a finding was truly resolved (e.g., you applied a fix but did not verify the build passes), emit `partial` so the analyze step can re-evaluate. The same applies to the `--ignored` integration test gate: if its trigger condition applies and you did not run it (or it failed), emit `partial`.
