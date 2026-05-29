Focus on **whole-tree security anomaly detection**. Categorical vulnerability classes (injection / auth / data exposure / crypto / unsafe code / path traversal) remain in scope, but applied at tree level: look for **patterns that span multiple files** or **invariants that the whole codebase silently relies on**, not single-line vulnerabilities the diff-local facet already covers.

The diff-local `review-security.md` is a separate facet with diff scope; this file is NOT a derivative for shared use. ADR-031 アンチパターン § 281 prohibits common-izing whole-tree review with diff-local review.

## Reading the source tree

Read selectively rather than end-to-end:

1. `src/**/*.rs` — Rust crates, with priority on:
   - Anything containing `unsafe` blocks (`grep -l "unsafe " src/`)
   - Anything reading env vars / arguments / external input (`std::env::var` / `clap` argument structs / `serde_json::from_str` on untrusted input)
   - Anything writing files / spawning processes (`std::fs::write` / `std::process::Command`)
2. `scripts/**/*.{ps1,sh}` — shell scripts, with priority on commands that take user input or call subprocesses
3. `.claude/**/*.{toml,json}` — hook configuration (block lists, allow lists, secret patterns)
4. `.takt/facets/instructions/**/*.md` + `.takt/workflows/**/*.yaml` — facet prompts (prompt injection surface) and workflow `allowed_tools` (privilege scope)
5. `docs/adr/*.md` — referenced when an ADR documents a security boundary (e.g. ADR-022 responsibility separation, ADR-028 exec gates)

Use `Glob` + `Grep` to locate hotspots; do NOT load every file.

## Project-Specific Context (read before judging)

Before evaluating, read:

1. `CLAUDE.md` — Project overview and ADR index
2. `docs/adr/adr-012-src-naming-convention.md` — naming convention (so each crate's responsibility is clear)
3. `docs/adr/adr-022-automation-responsibility-separation.md` — facet `edit: false` and write zone constraints (a security-relevant invariant)
4. `docs/adr/adr-028-pnpm-create-pr-gate.md` — external-visible artifact execution gates

Do not treat documented precedence rules, override mechanisms, or kill-switches as vulnerabilities by themselves. To raise a blocking finding, make the exploit path concrete: who controls what input, and what newly becomes possible.

## Vulnerability dimensions (use as memory aid, not a checklist)

Same dimensions as the diff-local facet, but flagging criterion is whole-tree-shaped: **the exploit path may span multiple files**.

- **Injection attacks** (whole-tree): actor-controlled input that flows through 2+ files before reaching an interpreter without escaping
- **Authentication / authorization flaws** (whole-tree): missing checks at one entry point that another entry point assumes are present (asymmetric guard coverage)
- **Data exposure risks** (whole-tree): secret patterns that appear in commit-able files (logs, examples, test fixtures) — pair with `grep -rE` for known token prefixes (`AKIA` / `sk-` / `ghp_` / `sk-ant-` etc.)
- **Cryptographic weaknesses** (whole-tree): weak algorithms or PRNGs centralized in one module and consumed by many call sites — single fix point but wide blast radius
- **Unsafe code coverage** (Rust): all `unsafe` blocks reviewed for `// SAFETY:` comments covering every invariant; if comments omit invariants the code relies on, flag the omission
- **Path traversal** (whole-tree): unsanitized path handling in shared helpers (e.g. `~/.takt/shared-jj-helpers` per ADR-024)
- **Prompt injection surface** (LLM-specific, whole-tree): facet prompts that read user-controlled artifacts (PR titles, commit messages, transcript text) without quoting or sanitization. `analyze-pr.md` / `analyze-session.md` and similar are the primary surface

## Anomaly mode (preferred entry point)

Read the tree once, hopping by reference rather than file order. If a pattern reads as **unusual / unexplained / hard to justify** from a security standpoint, that is your primary signal. Dimensions above are memory aids, not substitutes.

For each finding, articulate:

- **What is unusual or risky**
- **Who controls the relevant input or configuration** (user / external API / commit / PR title / etc.)
- **What newly becomes possible** (data access, privilege, code execution, prompt modification, secret exfiltration)
- **Files involved** (whole-tree findings often span 2+ files)

If you cannot articulate the third bullet, the finding is speculative — downgrade or drop it.

## Asymmetric guard coverage (whole-tree specific)

A class of whole-tree finding the diff-local facet structurally cannot see: one entry point that protects an invariant, another entry point that does not.

Example: PR body building in cli-pr-monitor sanitizes against shell argument truncation (PR #181 dogfood) but a hypothetical second PR-creation path elsewhere skips the sanitization. Flag the asymmetry, not the unprotected path alone.

Use `Grep` to find sibling entry points that perform conceptually similar operations, and verify whether their guard coverage matches.

## Judgment procedure

1. Glob the primary directories (§ Reading the source tree). Prioritize hotspots noted above.
2. Read selectively; follow references via `Grep` rather than depth-first traversal.
3. For each pattern, verify the concrete exploit path (input control, files traversed, what becomes possible).
4. Classify each verified concern by severity (`critical` / `high` / `medium` / `low`) per ADR-031 § Findings スキーマ.
5. Write the report per the output contract (`security-whole-review.md`). End with `analysis complete`.

## Scope boundary

The aggregate-weekly facet decides the final severity for the weekly report. Your role is to surface concerns with concrete exploit paths and file-level evidence; rubric-fitting (Severity / Frequency / Recommendation) is delegated to aggregate.
