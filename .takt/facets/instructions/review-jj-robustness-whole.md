Focus on **whole-tree jj-workspace robustness** — code and automation that silently breaks when the repo is used as a **non-colocated jj workspace** or across **parallel jj workspaces** (ADR-045). This facet is invoked by the weekly-review workflow (ADR-031、観点⑧) and reviews the entire tree for a specific class of environment-fragility that diff-local review never surfaces (each individual use looks fine; the hazard only appears under a workspace operation).

This axis exists because this project is unusually exposed: it is developed in `jj` workspaces (often non-colocated, i.e. no `.git` dir), with a shared build `target/`, parallel sessions, and gitignored local state. A single 2026-07 session hit four distinct bugs of this class (see criteria below) — none caught by any existing pipeline.

## Reading the source tree

Glob and Grep the surfaces where this class lives. Use `Grep` as the primary entry point (the patterns are grep-able), then `Read` the hit sites to judge whether each is a real hazard or a benign use.

1. `src/**/*.rs` — Rust crates (hooks, CLIs, libs)
2. `scripts/**/*.ps1` / `scripts/**/*.sh` — automation
3. `.claude/**/*.toml` + `package.json` scripts — how exes / `gh` / `jj` are invoked
4. `.takt/facets/instructions/**/*.md` + `.takt/workflows/**/*.yaml` — where facets shell out

Do NOT run `jj diff` — this is a whole-tree review. Use `Grep` / targeted `Read`; `Bash` is allowed **only** for read-only verification (`jj log`, `grep`, `wc`), never to mutate.

## Criterion 0 (MVP top priority): mtime as a staleness / freshness signal

File mtime is **reset to the checkout time** whenever a jj workspace materializes the working copy (`jj new`, `jj rebase`, `jj workspace add`). Any logic that treats mtime as "time since last event" is a silent-fresh bug: it perpetually looks recent and never fires (the exact bug that suppressed the weekly-review reminder for a month).

- Grep hints: `\.modified()`, `\.elapsed()`, `metadata\(`, `mtime`, `SystemTime::now()` used to compute an **age / staleness / "N days since"** decision.
- Judge: is the mtime standing in for "time since a logical event" (bug) vs. a genuine filesystem-freshness check (benign, e.g. cache invalidation on actual file change)? Only the former is a finding.
- Preferred fix to propose: derive elapsed from a **content timestamp** written by the producer (like `last_run_at`), not mtime. Reuse `reaper::parse_iso8601_to_unix` + `past_time::PastTime`.

## Criterion 1: `CARGO_MANIFEST_DIR` (or other compile-time absolute paths) for runtime file access

`env!("CARGO_MANIFEST_DIR")` bakes an **absolute path at compile time**. When a workspace dir is renamed, or a `target/` is shared/copied across workspaces, a cached test/exe reads data from a **stale or foreign path** and fails ("path not found") non-deterministically.

- Grep hints: `CARGO_MANIFEST_DIR`, `env!\(`, hard-coded absolute paths (`C:\\Users`, `/home/`) used at **runtime** for file reads.
- Judge: is the compile-time path used to locate a **runtime data file / fixture** (hazard, esp. in tests) vs. purely a compile-time constant? Flag the former.
- Preferred fix: resolve data relative to the current working dir / a runtime-discovered root, or pass the path in; for tests, note the shared-`target/` + rename hazard.

## Criterion 2: `gh` / git invocations that assume a colocated `.git`

Non-colocated jj workspaces have **no `.git` dir**, so `gh` commands that rely on git remote/branch detection fail ("not a git repository"). Similarly, raw `git` commands assume a layout jj may not provide.

- Grep hints: `gh ` invocations (`Command::new("gh")`, `gh pr`, `gh repo`, `gh api`) **without** `--repo` / `GH_REPO`; `Command::new("git")`; `.git` path assumptions.
- Judge: does the invocation depend on git auto-detection (hazard in non-colocated workspaces) vs. explicitly passing `--repo <owner/repo>` / setting `GH_REPO` (safe)? Flag detection-dependent calls in code paths that can run from a workspace.
- Preferred fix: pass `--repo` explicitly or thread `GH_REPO`; for repo detection, prefer `jj git remote` over `gh repo view`.

## Criterion 3: gitignored local-state lifecycle hazards

Local state that is gitignored + untracked can be **lost on a jj working-copy update** if it transitioned tracked→untracked (the untrack, once merged, deletes the on-disk copy during sync), and mtime-based logic over such files compounds Criterion 0.

- Grep hints: reads/writes of `.claude/*.json` / `.claude/*/` local state; `jj file untrack`; assumptions that a gitignored file persists across `jj new` / merge.
- Judge: does the code assume a gitignored state file always exists (hazard → should treat absence as a safe fail-open, e.g. `Missing`/`Stale`, not crash or over-suppress)?

## Calibration

Grep will over-match: most `.modified()` / `gh` / `env!` uses are benign. This facet's value is **judgment**, not enumeration — for each candidate, articulate the concrete workspace operation (rename / `jj new` / non-colocated run / parallel workspace) under which it breaks, and what the user would observe. If you cannot name the breaking operation, downgrade to 🤔 様子見. Do not flag a pattern the deterministic layer already guards; if a guard *should* exist, raise the layer-gap instead.

## Judgment procedure

1. Grep each criterion's hints across the tree; collect candidate sites.
2. Read each candidate; classify as real hazard vs. benign, naming the breaking workspace operation.
3. For each finding: what it is, where (file + line), the workspace operation that triggers it, the observable failure, and the proposed fix.
4. Classify severity per ADR-031 § Findings スキーマ (silent-fresh / data-access failures are typically `high`; cosmetic robustness gaps `low`–`medium`).
5. Write the report per the output contract (`review-jj-robustness-whole.md`). End with `analysis complete`.

## Output contract

- File: `review-jj-robustness-whole.md` (Report Directory)
- Format identifier: `review-jj-robustness-whole`
- Read-only (`edit: false`): report findings only; the `/weekly-review` skill + user decide adoption.
- Category hint for aggregate-weekly: `jj-mtime-staleness` / `jj-manifest-dir` / `jj-gh-no-repo` / `jj-state-lifecycle` (aggregate normalizes as needed).
- If no real hazard survives judgment, output「特筆すべき jj-robustness の findings なし」and end with `analysis complete` (do not manufacture findings from benign grep hits).
