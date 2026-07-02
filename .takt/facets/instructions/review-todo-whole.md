Focus on **whole-tree todo hygiene** — the health of the planning corpus (`docs/todo.md` + `docs/todo2.md` … `docs/todo13.md` + `docs/todo-summary.md`) taken as a whole. This facet is invoked by the weekly-review workflow (ADR-031、観点⑤ Todo 妥当性) and reviews the entire todo corpus, not a diff.

This is the **weekly batch** counterpart to the edit-time todo hooks. It exists because the deterministic layer only sees the entry being touched right now; it cannot see the corpus-wide decay that accumulates across dozens of untouched entries.

## Determinism layer guarantees (do NOT duplicate)

The following are enforced by deterministic hooks / CI lint at edit time and MUST NOT be re-enumerated here (raise a finding only if the layer itself has a gap):

- **Working-copy / add-edit staleness** (順位 136 hook): flags stale-looking edits and missing progress notes **on the entry being edited**.
- **Delete-time land verification** (順位 152): on `docs/todo*.md` deletion, greps for the corresponding land commit.
- **Preamble file-count + cross-reference** (`cli-docs-lint`, push-runner quality_gate): broken relative links / anchor drift are caught at push time.
- **File-size thresholds** (file-length-watchlist step): 50KB todo files / 800-line `.rs` are measured mechanically — do NOT eyeball file sizes here.

Your job is the **broad, cross-file, time-based decay** none of the above can see at edit time.

## Reading the corpus

1. `Glob docs/todo*.md` + `docs/todo-summary.md` — enumerate the whole corpus and note sizes.
2. Read `docs/todo.md` の preamble (冒頭の使い分けルール) first — it defines the routing contract (新規は todo6.md へ、編集専用は todo2-7.md、順位 table は todo-summary.md 等).
3. Sample the largest / oldest-looking files. Use `Grep` to follow task titles / 順位 numbers / `WR-` ids across files.
4. Cross-check the `docs/todo-summary.md` 順位 table against the detail entries it points to (`| N | Tier | title | todoX.md | ... |`).

Do NOT run `jj diff` — this is a whole-corpus review. Use `jj log` / `Grep` only to verify claims (e.g. whether a referenced land commit exists).

## Criterion 0 (MVP top priority): Dead / stale patterns

Entries that have decayed into noise the edit-time hook never revisits:

- **Aged-out entries**: a task entry that (a) has no related commit in recent `jj log`, AND (b) whose blocking dependencies have already landed (so it is either done-but-not-removed or obsolete), AND (c) shows no "現在地 / 詰まっている箇所" progress for a long stretch. Verify with `Grep` / `jj log` before raising — an entry that is simply *not yet started* is not dead.
- **Completed-but-not-removed**: an entry whose 完了基準 is demonstrably met by landed code/docs but which still occupies the corpus (violates 運用ルール「完了タスクは ADR か仕組みに反映後、削除する」).
- **Superseded pointers**: entries referencing an ADR / 順位 / file that has been superseded or removed (dead pointer within the planning corpus itself).

For each finding, name the **specific file + entry title** and the evidence (which dependency landed / which commit satisfies 完了基準). No evidence → downgrade to 🤔 様子見.

## Criterion 1: Cross-file duplicate entries

The corpus is split across 14 files; the same task can be registered twice as it migrates:

- The **same task** described in two `docs/todo*.md` files (e.g. a task drafted in todoN then re-drafted in todoN+1 without removing the first).
- A `docs/todo-summary.md` 順位 row whose detail entry no longer exists (or exists in a different file than the row claims).
- The reverse: a detail entry with no corresponding 順位 row (silently dropped from the execution order).

Use `Grep` on distinctive title fragments / `WR-` ids / 順位 numbers to confirm the duplication. Point to **both** locations.

## Criterion 2: Preamble routing drift

The `docs/todo.md` preamble encodes a routing contract that silently rots:

- A file the preamble calls "新規追加先" that has actually crossed 50KB (should have rolled over to the next file, per the split precedent) — cross-check against the file-length-watchlist output rather than guessing sizes.
- A file described as "編集専用・新規追加しない" that has in fact received new entries.
- Preamble file enumeration (「本ファイル + todo2.md + … の使い分け」) that omits or miscounts an existing `docs/todo*.md` file.

## Calibration

Resist checklist-thinking. The edit-time hooks + cli-docs-lint + file-length-watchlist already enforce the objective, per-entry, per-link, per-size rules. This facet earns its keep only on corpus-wide, time-based, cross-file decay that no single edit can surface. If you can only flag something by a mechanical rule the deterministic layer already runs, flag the *layer gap*, not the instance.

If a finding needs natural-language judgment about task intent (「これはもう不要では?」), that is exactly what belongs here — but articulate the evidence, and default to 🤔 様子見 when the intent is ambiguous (never propose deleting a user's planning entry on a hunch).

## Judgment procedure

1. Glob the corpus + read the `docs/todo.md` preamble (routing contract).
2. For Criterion 0/1/2, gather evidence with `Grep` / `jj log` — never raise a corpus-decay finding without a verified pointer.
3. For each finding, articulate: what it is, where it lives (file + entry title/順位), the verifying evidence, and the proposed action (remove / merge / re-route / re-number).
4. Classify each finding by severity (`critical` / `high` / `medium` / `low`) per ADR-031 § Findings スキーマ. Todo-hygiene findings are typically `low`–`medium` (corpus noise, not production risk); reserve `high` for a duplicate that could cause conflicting work.
5. Write the report per the output contract (`review-todo-whole.md`). End with `analysis complete`.

## Output contract

- File: `review-todo-whole.md` (Report Directory)
- Format identifier: `review-todo-whole`
- Read-only (`edit: false`): report findings only; the `/weekly-review` skill + user decide adoption (never edit `docs/todo*.md` from this facet).
- Category hint for aggregate-weekly: use `todo-dead-entry` / `todo-duplicate` / `todo-preamble-drift` (aggregate normalizes into the ADR-031 category set).
- If nothing survives evidence-gathering, output「特筆すべき todo-hygiene の findings なし」and end with `analysis complete` (do not manufacture findings).
