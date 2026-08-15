Focus on **whole-tree todo hygiene** — the health of the planning corpus (`docs/todo.md` + `docs/todo2.md` … `docs/todo13.md` + `docs/todo-summary.md` + `docs/claude-code-web-tasks.md`) taken as a whole. This facet is invoked by the weekly-review workflow (ADR-031、観点⑤ Todo 妥当性) and reviews the entire todo corpus, not a diff.

This is the **weekly batch** counterpart to the edit-time todo hooks. It exists because the deterministic layer only sees the entry being touched right now; it cannot see the corpus-wide decay that accumulates across dozens of untouched entries.

## Determinism layer guarantees (do NOT duplicate)

The following are enforced by deterministic hooks / CI lint at edit time and MUST NOT be re-enumerated here (raise a finding only if the layer itself has a gap):

- **Working-copy / add-edit staleness** (順位 136 hook): flags stale-looking edits and missing progress notes **on the entry being edited**.
- **Delete-time land verification** (順位 152): on `docs/todo*.md` deletion, greps for the corresponding land commit.
- **Preamble file-count + cross-reference** (`cli-docs-lint`, push-runner quality_gate): broken relative links / anchor drift are caught at push time.
- **File-size thresholds** (file-length-watchlist step): 50KB todo files / 800-line `.rs` are measured mechanically — do NOT eyeball file sizes here.

Your job is the **broad, cross-file, time-based decay** none of the above can see at edit time.

## Reading the corpus

1. `Glob docs/todo*.md` + `docs/todo-summary.md` + `docs/claude-code-web-tasks.md` — enumerate the whole corpus and note sizes.
2. Read `docs/todo.md` の preamble (冒頭の使い分けルール) first — it defines the routing contract (どの todo file が現在の新規追加先か、どれが編集専用か、順位 table は `todo-summary*.md`、等). The concrete file assignments roll over whenever a file crosses 50KB, so always take them from the current preamble — never from remembered file names.
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

## Criterion 3: 自律実行台帳 (`docs/claude-code-web-tasks.md`) の鮮度

This ledger is **read by the nightly todo loop (WP-18) as its task-selection source**, so its decay has a consequence the rest of the corpus does not have: a stale row can send an unattended agent to implement something already done, or something whose intent nobody has settled. The ledger's own § ライフサイクル designates this weekly review as the place where its freshness is checked.

Check three things, in this order:

1. **Landed-but-listed rows** — for each 順位 in the ledger's **active task tables only** (`### Batch 1` / `### Batch 2` under § 採用タスク (2), plus the § 採用タスク table when it is non-empty), `Grep` the exact table cell `| <順位> |` in `docs/todo-summary.md` / `docs/todo-summary2.md`. A row present in the ledger but **absent from both 順位 tables** has landed and should be removed (with the evidence recorded in the ledger's § 棚卸し履歴). Verify the task really landed (grep the artifact it claims to produce) before raising — a 順位 can also disappear because it was deprioritized.

   **Two scoping rules keep this from firing forever on correct content.** First, exclude the ledger's § 棚卸し履歴 and § 無人可としなかった…理由 tables: both carry 順位 columns, and 棚卸し履歴 deliberately retains already-removed 順位 as the audit record of *why* they were removed. Treating those as "listed" would raise the same finding every week for rows that are supposed to stay. Second, match the cell form `| <順位> |` rather than the bare number — a bare `120` also matches line counts, byte sizes, and other 順位 that merely contain those digits.
2. **Unlisted 順位 — report the count, do NOT judge eligibility** — state how many 順位 exist in `docs/todo-summary.md` + `docs/todo-summary2.md` that are not listed in the ledger's current task tables, so the user has a sense of the backlog the ledger does not cover.

   **Judging which of them belong in the ledger is NOT this facet's job, and neither is proposing a `無人可` mark.** Under the lane model (ADR-072 決定 18) the ledger is an *assignment table*: listing a 順位 and setting its lane (`✅` = nightly loop / `—` = human) are both human decisions. The previous instruction demanded a full eligibility judgment of every unlisted 順位 with a two-path reason for each, and it failed on two consecutive runs (2026-08-13: ~50 of 164 sampled; 2026-08-15: 13 of 251 judged — both reported "0 candidates"). That obligation is withdrawn rather than re-strengthened a third time.

   **Counting only.** Report: total 順位 per summary file, how many are already listed in the ledger, and the remainder. If you happen to notice a specific 順位 that looks like an obvious fit, you may name up to a few as *examples* — clearly marked as examples, never as a screened set.

   **This section is still MANDATORY: `## 昇格候補 (promotion candidates)`.** Its content is now the count above; a missing section is still read as "check not performed" by `/weekly-review`.

   **When enumerating todo files, restrict to numbered detail files (`todo<数字>.md`) and expand range notation before comparing.** A bare `docs/todo*.md` glob also matches `docs/todo-summary.md` / `docs/todo-summary2.md`, which are 順位 tables rather than entry destinations; and the preamble writes ranges (`todo3.md 〜 todo23.md`, `todo3-23.md`) that never match as literal strings.

   > **This count will move to a deterministic step.** PR-5 of the lane-model work plan computes the same set difference in Rust (`lib-ledger` + the 順位-table parser) and feeds it to the weekly-review workflow. When that lands, this item becomes a pointer to that output and the counting stops being an LLM task.
3. **`✅` rows whose 注意 column no longer reads as unattended-ready** — re-read the 注意 text of each `✅` row against conditions 1 and 2 of the ledger's § 自律実行可否の 2 段階分類 (no 「再選定」「着手時判断」「見積り」「検討」 or equivalent; implementation uniquely determined). A row edited since the lane was assigned may now carry a judgment the human who set `✅` never saw. Report it as **material for the human's lane decision**, not as a verdict.

   **Do NOT scan remote bookmarks or in-flight PRs for duplicate work.** Condition 3 (重複の恐れがない) was abolished on 2026-08-16 — it read the same `claude/nightly-<順位>` branch that ADR-072 決定 3 reads as "already in progress, skip", and the two readings contradicted each other. Under the lane model, duplication is prevented by *not making conflicting assignments* (a human taking over a task moves its lane to `—`), not by detection. A duplicate-detection finding raised here is out of scope and will be rejected.

Severity guidance for this criterion: landed-but-listed rows are `low`–`medium` (ledger noise, but they can send the nightly loop at a finished task); a `✅` row whose 注意 column now carries judgment-reserving wording is `medium` (the lane may need revisiting, but a human decides).

**Never propose adding or removing a `✅` mark yourself as a settled decision** — lanes are assigned by a human (ADR-022, ADR-072 決定 18). Raise the material with evidence and let the `/weekly-review` adoption step carry it to the user.

## Calibration

Resist checklist-thinking. The edit-time hooks + cli-docs-lint + file-length-watchlist already enforce the objective, per-entry, per-link, per-size rules. This facet earns its keep only on corpus-wide, time-based, cross-file decay that no single edit can surface. If you can only flag something by a mechanical rule the deterministic layer already runs, flag the *layer gap*, not the instance.

If a finding needs natural-language judgment about task intent (「これはもう不要では?」), that is exactly what belongs here — but articulate the evidence, and default to 🤔 様子見 when the intent is ambiguous (never propose deleting a user's planning entry on a hunch).

## Judgment procedure

1. Glob the corpus + read the `docs/todo.md` preamble (routing contract).
2. Gather evidence with `Grep` / `jj log` — never raise a corpus-decay finding without a verified pointer. Every criterion, Criterion 3 included, is answerable from repository contents alone; no remote bookmark or PR lookup is required (or wanted — see Criterion 3-3).
3. For each finding, articulate: what it is, where it lives (file + entry title/順位), the verifying evidence, and the proposed action (remove / merge / re-route / re-number).
4. Classify each finding by severity (`critical` / `high` / `medium` / `low`) per ADR-031 § Findings スキーマ. Todo-hygiene findings are typically `low`–`medium` (corpus noise, not production risk); reserve `high` for a duplicate that could cause conflicting work.
5. Write the report per the output contract (`review-todo-whole.md`). End with `analysis complete`.

## Output contract

- File: `review-todo-whole.md` (Report Directory)
- Format identifier: `review-todo-whole`
- **Required section**: `## 昇格候補 (promotion candidates)` must always be present (Criterion 3-2). Its content is the **count of unlisted 順位**, not a screened candidate set — see Criterion 3-2 for what to report and what not to judge. A missing section is reported to the user as "check not performed" (never silently treated as 0 件).
- Read-only (`edit: false`): report findings only; the `/weekly-review` skill + user decide adoption (never edit `docs/todo*.md` or `docs/claude-code-web-tasks.md` from this facet — the ledger's lane marks in particular are a human decision).
- Category hint for aggregate-weekly: use `todo-dead-entry` / `todo-duplicate` / `todo-preamble-drift` / `ledger-staleness` (aggregate normalizes into the ADR-031 category set).
- If nothing survives evidence-gathering, output「特筆すべき todo-hygiene の findings なし」and end with `analysis complete` (do not manufacture findings).
