Focus on **whole-tree architecture coherence**. This facet detects drift between the codebase's actual structure and the design rationale codified in ADRs / global rules / harness layers. It is invoked by the weekly-review workflow (ADR-031) and is the **primary detection layer** for harness drift, docs↔code divergence, and ADR violations that no diff-local facet can see.

There is no diff-local analog of this facet. Architecture coherence is intrinsically whole-tree.

## Criterion 0 (MVP top priority): Harness adherence — rule / pipeline / hook duplication

This project deliberately migrates "rules" (LLM-prompt-enforced) → "pipelines" (procedural) → "hooks" (mechanically enforced) when feasible (順位 146-151 Bundle "既存ルール仕組み化"). Detect three failure modes:

1. **Rule + hook overlap**: A rule in `~/.claude/rules/common/*.md` or `CLAUDE.md` codifies a constraint that a PostToolUse / PreToolUse hook also enforces. Either the rule is residual after hook landing (delete the rule, point to the hook) or the hook is a partial mechanization (consolidate the rule into the hook's coverage).
2. **Pipeline + hook overlap**: A push-runner stage in `src/cli-push-runner/src/stages/` reproduces a check that the same hook performs at write time. Either the stage is defense-in-depth (acceptable, but `docs/adr/` should justify) or the stage is residual.
3. **Multi-layer rule fragmentation**: One conceptual constraint expressed in 3+ places (rule + pipeline + hook + ADR) without `docs/adr/` documenting which is the authoritative source and which are reminders.

For each finding, name the **specific files** that overlap, and propose which layer should own the constraint going forward. Memory rule `feedback_pipeline_over_rules.md` is the bias to apply: when a constraint can be mechanized, the pipeline / hook layer should own it; rules become pointers, not enforcement.

Hook + pipeline + rule entry points to enumerate:

- `~/.claude/rules/common/*.md` — global rules (referenced via `Read` if needed; do not modify)
- `CLAUDE.md` — project rules
- `.claude/custom-lint-rules.toml` + `.claude/hooks-config.toml` — declarative hook config
- `src/cli-push-runner/src/stages/*.rs` — push-time stages
- `src/hooks-*/src/main.rs` — hook implementations
- `.takt/workflows/*.yaml` — facet-time review prompts (rules expressed as LLM instructions)

## Criterion 1: ADR alignment

Spot drift between ADRs and the code they describe. Priority ADRs (read these whenever you raise an alignment finding):

- ADR-007 (custom-linter layer boundary)
- ADR-012 (src/ naming convention)
- ADR-021 (jj change detection principles)
- ADR-022 (automation responsibility separation — `edit: false` + write zones)
- ADR-030 (deterministic post-merge-feedback)
- ADR-031 (weekly review pipeline — this facet's own spec)
- ADR-036 (Bundle Z 3-layer review architecture)

For non-priority ADRs, read on demand only when a finding suggests violation. Do NOT load all ADRs end-to-end (context budget).

Patterns to flag:

- ADR specifies behavior X, code implements behavior X' (drift)
- Code introduces an architecture decision that no ADR captures (undocumented decision)
- ADR was superseded but the code still references the superseded ADR's structure
- ADR documents a 3-layer pattern (e.g. ADR-036) but a workflow uses only 2 layers without rationale

## Criterion 2 (sub criterion): docs 内整合性

Cross-document consistency within `docs/`:

- ADR cross-references that point to non-existent sections (similar to PR #94 / #111 / #132 broken-cross-ref pattern; cli-docs-lint (順位 95+96 = PR #179) is the mechanical baseline — flag patterns that escape the mechanical check)
- ADR families that should be linked but are not (e.g. two ADRs covering related decisions with no "see also")
- Index drift: `CLAUDE.md` ADR list missing recently landed ADRs, or listing ADRs that have been retired without "Superseded" annotation

Skip individual broken-link cases that cli-docs-lint already catches; this criterion is the meta-pattern detector ("the mechanical check missed a class").

## Criterion 3 (sub criterion): docs ↔ source 矛盾

Drift between docs and the source they describe. Read only the priority docs:

- `CLAUDE.md`
- ADRs from § Criterion 1 priority list

Patterns:

- ADR says "Rust impl uses X pattern", code uses Y (e.g. ADR-024 specifies shared-jj-helpers location, code uses inline duplication)
- ADR specifies a directory layout, code violates it (ADR-012 naming convention)
- ADR documents a kill-switch / experimental flag, code lacks the flag or applies a different env var name
- `~/.claude/rules/common/*.md` describes a convention, code violates it for a non-trivial fraction of cases (single exceptions are local; a pattern of violations is architectural)

Do NOT chase low-level style drift that the deterministic layer (`hooks-post-tool-comment-lint-rust` etc.) already catches. This criterion is for whole-tree-shaped drift only.

## Criterion 4: Module boundaries, cyclic dependencies, layer violations

Whole-tree patterns the diff-local facet cannot see:

- **Cyclic dependencies** between crates / modules (`cargo tree` or `cargo modules` for verification)
- **Layer violations**: e.g. a `stages/` module reaching into another crate's private impl, or a `cli-*` crate calling another `cli-*` crate directly instead of via a shared lib (ADR-024)
- **God modules**: single modules that absorb responsibilities from 3+ unrelated concerns
- **ADR-012 violations**: directory names that do not match the ADR-012 naming convention

For each finding, name the specific files / crates and propose the smallest restructuring (`extract this trait`, `move X to lib Y`, etc.). Do NOT recommend large refactors without estimating effort and dependency depth.

## Scope constraints

- Read selectively. Glob to enumerate, Read to verify, Grep to confirm. Do NOT load the full `src/` tree.
- For ADRs: priority list above; load others on demand only when a finding suggests they are relevant.
- When in doubt about scope, prefer fewer high-confidence findings over many low-confidence ones. Aggregate-weekly will weight by severity / frequency / adoption risk — your role is to surface the architectural concern with evidence.

## Judgment procedure

1. Glob the structural surfaces (`.claude/`, `.takt/`, `src/cli-*/src/stages/`, `src/hooks-*/src/`, `docs/adr/`, `CLAUDE.md`).
2. Start with Criterion 0 (harness adherence) — this is the MVP top priority and the continuous source of "既存ルール仕組み化" candidates.
3. For each suspected pattern, verify the file evidence (`Grep` for symbol existence, `Read` ADR sections referenced).
4. Classify each verified concern by severity (`critical` / `high` / `medium` / `low`) and category (one of: `harness-duplication` / `adr-alignment` / `docs-internal` / `docs-source-drift` / `module-boundary` / `cyclic-dep` / `layer-violation` / `adr-naming`).
5. Write the report per the output contract (`architecture-whole-review.md`). End with `analysis complete`.

## Scope boundary

Severity / Frequency / Adoption Risk / Recommendation rubric is delegated to aggregate-weekly. Surface the concern with file-level evidence; let aggregate weight it.
