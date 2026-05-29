Focus on **whole-tree anomaly detection** for simplicity — patterns that read as cumulative complexity, dead-on-arrival abstractions, or test design that does not protect behavior. This facet is invoked by the weekly-review workflow (ADR-031) and reviews the entire source tree, not a diff.

The diff-local `review-simplicity.md` is a **separate facet** with different scope; this file MUST NOT be merged with it (ADR-031 § アンチパターン: `review-simplicity.md` を whole-tree 用と共有してはならない). Their concerns are orthogonal: diff-local guards "is the change locally readable", whole-tree guards "has accumulated complexity outgrown the reader's capacity".

## Reading the source tree

Glob the primary directories in this order and read selectively. Do NOT load every file end-to-end; sample the largest / most-recently-changed files first and follow references:

1. `src/**/*.rs` — Rust crates (largest surface area)
2. `scripts/**/*.ps1` / `scripts/**/*.sh` — automation scripts
3. `.takt/facets/instructions/**/*.md` + `.takt/workflows/**/*.yaml` — facet prompts and workflow chains (LLM behavior surface)
4. `.claude/**/*.toml` + `.claude/**/*.json` — hook configuration (deterministic layer surface)
5. `docs/adr/*.md` + `CLAUDE.md` — design rationale (referenced for ADR alignment checks)

Use `Glob` first to enumerate, then `Read` selectively. Use `Grep` to follow specific symbols / patterns. Do NOT run `git diff` / `jj diff` — this is a whole-tree review, not a diff review.

## Determinism layer guarantees (do NOT duplicate)

The following dimensions are enforced by deterministic hooks (PostToolUse / push-runner quality_gate) and need not be re-checked here:

- **Comment policy** (`hooks-post-tool-comment-lint-rust`): Non-doc comments blocked at write time
- **Function length** (順位 48, same hook): Functions >50 lines blocked (touch-trigger ratchet)
- **Function metrics during fix** (`fix-metrics-check.ps1`): non-doc comment count, function length, max nesting depth cannot increase per function during fix iterations
- **File length** (順位 147, planned): when landed, replaces manual whole-tree file size review

Skip these dimensions. If the determinism layer is the right home for a pattern, raise a finding suggesting the layer extension, but do not enumerate per-file violations the deterministic layer already catches.

## Criterion 0 (MVP top priority): Test logic anomalies

Test code is the part of the tree the deterministic layer least covers. Examine `**/*_test.rs`, `**/*tests/*.rs`, `**/__test*.ps1`, `**/test_*.py` etc. for:

- **Behavior vs implementation detail drift**: tests that assert internal call ordering, mock invocation count, or struct field shape when the behavior they purport to verify is functional output
- **Boundary coverage gaps**: pure functions or guards whose `None` / empty / threshold-equal / overflow cases are not independently exercised (see also order-of-operations guards under § Multi-condition guard test isolation in `~/.claude/rules/common/code-review.md`)
- **Mock-heavy integration tests**: tests that mock the very dependency they claim to integrate-test (mocked DB in a migration test, mocked HTTP in an end-to-end test)
- **Silent regressions**: tests that pass without observably exercising the production path (e.g. `assert!(true)` after a failing setup that was swallowed)
- **Test DRY violations**: shared helper that hides variant differences. Per `feedback_test_dry_antipattern.md`, each test variant should be independently set up; helpers that conceal the per-variant `setup → act → assert` triple are an anti-pattern even if they reduce LoC

For each finding, articulate **what the test fails to protect** and **what production change would slip past it**. If you cannot articulate the second bullet, downgrade the finding.

## Criterion 1: Cumulative complexity (whole-tree only)

Patterns that no single diff can flag because each individual hop looks reasonable:

- **Indirection chains longer than 3 hops**: A → B → C → D where each layer adds one transformation but no decision
- **Parallel hierarchies**: parallel module trees that re-implement the same conceptual operation (e.g. two error converters that diverge slowly)
- **Abstraction premium without callers**: traits / interfaces with one implementor, or generic types with one concrete instantiation, that were introduced for "future extensibility" but no second caller materialized

For each finding, name the **specific files** that constitute the chain and propose a concrete consolidation. If you cannot name files, the finding is speculative.

## Criterion 2: Dead-on-arrival code (whole-tree)

Code paths with no observable caller anywhere in the tree:

- Functions / methods / types reachable only from their own unit tests
- Configuration fields / enum variants never read
- Generic parameters never instantiated with more than one type

Use `Grep` to verify zero callers before raising. A finding without a verified zero-caller search is speculative.

## Criterion 3: Overspec'd abstractions (whole-tree)

Abstractions that exceed the requirement they document:

- Generic types where a concrete type would compile
- Builder patterns for 1-2-parameter structs
- Trait objects where a function pointer or enum would suffice
- Layered error types that wrap each other without adding context

For each finding, articulate **what the simpler form would lose** (and verify the loss is not currently exploited).

## Calibration

Whole-tree review tempts checklist-thinking ("file X has 800 lines, so flag it"). Resist. The deterministic layer already enforces objective metrics. This facet exists to catch the patterns no checklist names. If you can only flag something by mechanically applying a rule, the deterministic layer already handles it (or should — flag the gap, not the per-file count).

Conversely, if reading the tree leaves you with a concrete unease that you can articulate, raise it — even if it doesn't fit a named criterion. Per `feedback_pipeline_over_rules.md`, fuzzy detection belongs in the pipeline layer (this facet), not in unenforceable rules.

## Judgment procedure

1. Glob the primary directories (§ Reading the source tree). Skim file names + sizes.
2. Sample the largest / most-recently-modified files first; cover all 5 directory groups.
3. For each section read, note any pattern matching Criteria 0-3.
4. For each finding, articulate: what it is, where it lives (file + line range), why it caught attention, what alternative would be expected, and **what behavior or invariant is at risk**.
5. Classify each finding by severity (`critical` / `high` / `medium` / `low`) per ADR-031 § Findings スキーマ.
6. Write the report per the output contract (`simplicity-whole-review.md`). End with `analysis complete`.
