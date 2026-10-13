# ローカル LLM の採点結果 (2026-10-13)

第三者レビューの候補として、ローカル LLM (Ollama) を正解データで採点した記録。数字と条件の機械可読版は `summary.json`、判定の元データは `*.judged.json`、実行に使ったスクリプトは `run-local.mjs`。**判定は Claude によるもので、個別の検証は省いた** (結論に影響しないため。2026-10-13 ユーザー判断)。

## 採点表

| 候補 | モデル | 思考 | 再現率 | Major 再現率 | 正当な追加の指摘 | ノイズ | 1 ファイルの待ち時間 (中央値 / 最大) |
|---|---|---|---|---|---|---|---|
| qwen-think | qwen3.8:27b-q8_0 (qwen35, 27.3B, Q8_0) | あり | 8/45 | 3/21 | 13 | 8 | 114.3 秒 / 301.3 秒 |
| qwen-nothink | qwen3.8:27b-q8_0 (qwen35, 27.3B, Q8_0) | なし | 2/45 | 1/21 | 3 | 21 | 2.8 秒 / 12.4 秒 |
| gemma-nothink | gemma4:26b-a4b-it-q8_0 (gemma4, 25.8B, Q8_0) | なし | 1/45 | 1/21 | 2 | 22 | 0.9 秒 / 8.7 秒 |

- 再現率: 正解 (CodeRabbit の指摘のうち採用が確定した 45 件) のうち、同じ問題を指摘できた割合
- 正当な追加の指摘: 正解と別の問題だが、指摘として妥当なもの。ノイズ: 妥当でないもの

## 条件

- 正解データ: `scripts/review-bench/ground-truth.json` (#457-#556、45 件 / 25 PR / Major 21 件)
- 実行スクリプト: scripts/review-bench/results/2026-10-13-local-llm/run-local.mjs
- 実行環境: Ollama 0.35.1 (ローカル GPU: RTX PRO 5000 48GB)
- 入力の範囲: 正解の付いたファイル 40 本の差分だけを、ファイル単位で渡す (PR の他のファイルは見せない)
- 入力の形: hunk の各行に新ファイル側の行番号を付ける。1 ファイル 60,000 文字で打ち切り (今回は打ち切り 0 件)
- プロンプト: 徹底的なシニアレビュアー (run-local.mjs の PROMPT)。スタイルの指摘は求めない。JSON で {line, severity, text} を返させる
- 生成オプション: format: json / temperature 0.1 / num_ctx 32768
- 実行回数: 各候補 1 回 (ばらつきは未計測)
- 一致の判定: 行 ±25 で候補を絞り、同じ問題か・妥当かは人が判定する (score.mjs)
- 判定者: Claude による判定を採用した (2026-10-13 ユーザー判断: ローカル LLM が CodeRabbit の代わりにならないという結論は判定が多少ずれても変わらないため、個別の検証は省いた)。確かめられない主張はノイズ側に倒した
- qwen-think の出力枠 (num_predict): 16384。実行したファイル 40 本、失敗 1 本、指摘 0 件のファイル 19 本、指摘 29 件、合計 73.9 分
- qwen-nothink の出力枠 (num_predict): 4096。実行したファイル 40 本、失敗 0 本、指摘 0 件のファイル 20 本、指摘 26 件、合計 2.5 分
- gemma-nothink の出力枠 (num_predict): 4096。実行したファイル 40 本、失敗 0 本、指摘 0 件のファイル 17 本、指摘 25 件、合計 0.7 分

## 注意

- 思考ありモデルを num_predict 4096 で動かすと、長い差分で思考だけで出力枠を使い切り、JSON が途中で切れる (初回の実行で qwen3.8 は 40 本中 19 本、gemma4 は 26 本が失敗)。思考ありは出力枠を広げるか、think: false を明示する
- CodeRabbit の確定指摘を正解にしているため、再現率は『CodeRabbit と同じ問題を拾えるか』を測る。第三者レビューの価値は正当な追加の指摘の側にも現れる (ADR-046 と同じ観察)
- zeroFindingFiles (指摘 0 件のファイル) には、モデルが findings 配列の無い JSON を返した失敗が混じっている可能性がある。この実行は、それを失敗として記録する検査を run-local.mjs に足す前の版で行い、生の応答も保存していないため区別できない。再現率・正当な追加の指摘・ノイズは出てきた指摘だけから数えるので影響しない
- 外部サービス (別 SaaS / API モデル) の採点は未実施。2026-10-13 のユーザー判断で、費用は無料の範囲に限り、今は外部を試さないことにした

## 判定の中身

判定を確かめるための一覧。`fid` は判定済みシートの候補番号。

### qwen-think

**正解と同じ問題を指摘した候補**

| 正解 | 重大度 | 正解の要約 | fid | 候補の指摘 |
|---|---|---|---|---|
| 465-3903729287 | Major | 本文中の例示 `fence` を含む全文 `fence` を正しく処理してください。 | 0 | A document-wrapped report that also contains an internal code block (e.g. 4 fence lines total: outer open, inner open, inner close, outer close) will not satisfy `fences.len() == 2`, so the outer fence is not stripped an… |
| 466-3904251218 | Minor | 削除失敗を ref 移動として分類しないでください。 | 1 | `classify_after_failed_delete` returns `AbortedRefMoved` whenever the recheck is `Present(_)` without comparing the recheck SHA to the original observation SHA. If the delete fails for a transient reason (network timeout… |
| 469-3914196322 | Minor | 使用例へ必須の `--limit` を追加してください。 | 2 | The module-level usage example (lines 16–20) invokes the tool without the `--limit` flag, but `parse_args` makes `--limit` mandatory (line 122: `limit.ok_or("--limit が必要です")?`). Following the documented command will exit… |
| 470-3914932177 | Major | `gh` 呼び出しにタイムアウトを設定してください。 | 5 | Neither `spawnSync` call specifies a `timeout` option. If `gh` blocks on an interactive auth prompt (e.g. run in a terminal without a pre-configured token), the script hangs indefinitely with no way to recover or report … |
| 470-3914932190 | Minor | `process.exit` で一時ディレクトリの cleanup を飛ばさないでください。 | 3 | `process.exit()` terminates the process immediately without unwinding the call stack, so the `finally` block on line 95–97 (`rmSync(workDir, …)`) never executes. The temp directory created on line 76 leaks on every run; … |
| 475-3933467149 | Major | 却下条件を満たした後の判断を反映してください。 | 14 | Line 157 (existing, immediately above) states '残余は 1 件' (1 remaining before the rejection threshold of 3), implying the threshold has not yet been met. The new line 158 then states the total is 6 and the threshold is met… |
| 508-4052422220 | Minor | 順位521の「47日」を正しい日付差へ統一してください。 | 22 | The elapsed time from 2026-08-01 to 2026-09-18 is 48 days (30 remaining days in August + 18 days in September), not 47 as stated. This minor arithmetic error could cause confusion when the bounded-lifetime deadline is la… |
| 510-4054188061 | Minor | 副作用のある呼び出しを関数ポインタの束縛に置き換えてください。 | 24 | `inject_git_dir_for_gh` almost certainly calls `std::env::set_var("GIT_DIR", …)`, which is a process-wide mutation. Because Rust runs `#[test]` functions concurrently in the same binary by default, this can race with `ge… |

**正解外で妥当と判定した指摘**

| fid | PR | ファイル:行 | 指摘 |
|---|---|---|---|
| 4 | #470 | `scripts/ledger-residue-scan.mjs:80` | If `spawnSync` fails to launch the child (e.g. `run-artifact.mjs` is missing or `process.execPath` is invalid), `scan.error` is set but never written to stderr; the caller only sees exit code 2 with no diagnostic message… |
| 6 | #471 | `scripts/rebase-nightly-pr.mjs:284` | The verification step calls `run()` (not `runOrFail()`) but never checks `check.error`. If the spawn itself fails (e.g. `run-artifact.mjs` is missing or `process.execPath` is invalid), `check.status` is `null`, so the co… |
| 7 | #471 | `scripts/rebase-nightly-pr.mjs:76` | When a child process is terminated by a signal (rather than exiting or timing out), `result.status` is `null` and `result.signal` holds the signal name, but the error message interpolates `result.status` producing "exit … |
| 8 | #472 | `src/cli-docs-lint/src/origin_markers.rs:72` | All violations are emitted with `line: 0`, so the lint output cannot point to the specific line in the summary file where the offending row lives; in a large table this forces the developer to search manually by rank num… |
| 9 | #472 | `src/cli-docs-lint/src/origin_markers.rs:63` | `detail_body` terminates the entry at the first `\n### ` sequence, so if the entry body contains a fenced code block (or any line) that begins with `### `, the body is truncated early and evidence appearing after that po… |
| 10 | #473 | `docs/claude-code-web-tasks.md:157` | The root-cause explanation states the deliverable is under `.claude/` (dot-prefixed) so the agent's `Edit(work/**)` glob cannot match it, but the declared deliverable is `.claude/custom-lint-rules.toml + tests/fixtures/i… |
| 11 | #473 | `docs/claude-code-web-tasks.md:158` | Same inaccuracy as line 157: the explanation says the deliverable is under `.claude/` and therefore structurally uncompletable, but the deliverable also includes `tests/fixtures/incidents/{bad,good}/` which is outside `.… |
| 12 | #473 | `docs/todo26.md:69` | The example `#[cfg(test_util)]` does not actually contain the substring `#[cfg(test)]` (the closing `)]` is absent in `test_util`), so it does not demonstrate the claimed false-positive. The real search string is likely … |
| 15 | #479 | `src/cli-merge-pipeline/src/stray.rs:144` | If `wait_with_timeout_basic` returns `Ok(None)` (timeout) without killing the child process, the subsequent `stdout_handle.join()` / `stderr_handle.join()` calls will block indefinitely because the drain threads are wait… |
| 17 | #504 | `config/custom-lint-rules.toml:842` | The regex character class `[\w.-]*` before `config\.toml` excludes path separators (`/`, `\`), so a single `.join(".claude/hooks-config.toml")` call (valid Rust) would not be detected. The comment on line 825 uses `<...>… |
| 18 | #505 | `docs/claude-code-web-tasks.md:330` | The note for task 455 still states in present tense that the agent-side live verification is "waiting for rank 519" and that the glob reach is "still just inference from glob semantics," but the new line 333 records 519 … |
| 23 | #508 | `docs/todo28.md:5` | The file is named `todo28.md` (Part 28) but the routing preamble states new entries should go to `docs/todo26.md`, a lower-numbered (older) file. This is a routing contradiction that will misdirect future TODO entries to… |
| 27 | #551 | `src/cli-nightly-outcome/src/classify.rs:69` | The variant doc says "agent を回して捨てている" (the agent was run and its work discarded), and the render message on line 295 says "agent の実装は捨てています", but the code returns Superseded even when both publish and handoff are "skipp… |

**ノイズと判定した指摘**

| fid | PR | ファイル:行 | 指摘 |
|---|---|---|---|
| 13 | #475 | `docs/adr/adr-074-auto-lane-screening-criteria.md:158` | The new bullet classifies the 4 failures as 選別ミス (screening mistakes), but the definition in line 155 lists examples (missed judgment reservation, path drift, environment dependency) that do not include 'wrong file decla… |
| 16 | #482 | `src/cli-nightly-outcome/src/agent_reason.rs:88` | `strip_prefix(STOP_PREFIX)` leaves the separator space in `rest` (e.g. `" 注釈が…"`), but the test `a_prefixed_first_line_is_the_reason` expects the leading space to be absent. Since `screen_for_public_output` only removes … |
| 19 | #505 | `docs/todo13.md:223` | The config path changed from `.claude/custom-lint-rules.toml` to `config/custom-lint-rules.toml`; if the file has not actually been relocated in this or a prior PR, the implementer will look in the wrong directory and th… |
| 20 | #505 | `docs/todo13.md:247` | The ADR-080 link `adr/adr-080-rust-module-split-invariants.md` is relative to `docs/todo13.md`, resolving to `docs/adr/…`; if ADRs live at the repository root (`adr/`) or another location, this will be a broken link for … |
| 21 | #507 | `config/custom-lint-rules.toml:997` | `bad_fixture` and `good_fixture` both point to the same file `no-pid-timestamp-temp-naming.rs`. If the harness uses `bad_fixture` for positive (should-detect) tests and `good_fixture` for negative (should-not-detect) tes… |
| 25 | #514 | `docs/adr/adr-019-coderabbit-review-hybrid-policy.md:317` | The table row for PR #494 has its two data columns swapped relative to the other rows: the first column ("PR 作成〜自発 walkthrough", a time interval) contains "自発が先" (a before/after relationship), while the second column ("要… |
| 26 | #514 | `docs/adr/adr-019-coderabbit-review-hybrid-policy.md:330` | Decision 1 says to skip the explicit request when HEAD is already "in review" (pending), but the ADR does not document a timeout or escalation path for the case where CodeRabbit's review hangs in `pending` indefinitely; … |
| 28 | #551 | `src/cli-nightly-outcome/src/classify.rs:113` | The `superseded` check is placed after `publish == Success` but before the handoff match, so a genuine `publish` failure (e.g., a git-push network error) is silently masked to green when `superseded` is also true; there … |

### qwen-nothink

**正解と同じ問題を指摘した候補**

| 正解 | 重大度 | 正解の要約 | fid | 候補の指摘 |
|---|---|---|---|---|
| 470-3914932190 | Minor | `process.exit` で一時ディレクトリの cleanup を飛ばさないでください。 | 3 | The `process.exit()` call inside the `try` block prevents the `finally` block from executing, causing the temporary directory (`workDir`) to leak on every successful or failed scan run. |
| 504-4029409074 | Major | 一致範囲を同一の path-building 式に制限してください。 | 13 | The regex pattern `current_(?:dir\|exe)\(\)[\s\S]{0,400}?\.join\(\s*"[\w.-]*config\.toml"\s*\)` may produce false positives if `current_dir()` or `current_exe()` is used for unrelated purposes (e.g., logging, environment… |

**正解外で妥当と判定した指摘**

| fid | PR | ファイル:行 | 指摘 |
|---|---|---|---|
| 5 | #471 | `scripts/rebase-nightly-pr.mjs:284` | The `run` function is used for the final check without checking `result.error` (e.g., if `node` or the script path is invalid). If `result.error` is set, `result.stdout` and `result.stderr` may be undefined, and `result.… |
| 18 | #508 | `docs/todo28.md:5` | The routing preamble states that new entries should be recorded in `docs/todo26.md`, but the file immediately below (line 46) contains an open task to create `docs/todo23.md` and update the routing pointer to it, creatin… |
| 24 | #551 | `.github/workflows/nightly-todo.yml:551` | The glob `latest-master/docs/todo-summary*.md` will not expand if no matching files exist, causing the loop to iterate over the literal string and pass a non-existent path to `--summary-file`, which will likely cause the… |

**ノイズと判定した指摘**

| fid | PR | ファイル:行 | 指摘 |
|---|---|---|---|
| 0 | #466 | `src/cli-branch-cleanup/src/classify.rs:130` | is_object_id accepts any non-empty ASCII hex string, so a malformed ls-remote line whose first field is a short hex token (e.g. a truncated SHA or a hex-looking ref fragment) is classified as Present and that value is la… |
| 1 | #466 | `src/cli-branch-cleanup/src/classify.rs:114` | observe_from_output only inspects the first line and ignores the rest of the output, so multi-line or trailing diagnostic output from git ls-remote is silently discarded; while usually harmless, it can mask unexpected se… |
| 2 | #469 | `src/cli-ledger-residue-scan/src/main.rs:137` | The saturation check `array.len() >= limit` treats a result set that exactly equals the limit as an error, which is a false positive when the repository has exactly `limit` merged PRs and the fetch is complete; this can … |
| 4 | #471 | `scripts/rebase-nightly-pr.mjs:284` | The script invokes `run-artifact.mjs` with the argument `cli-ledger-removal-check`, but the documentation (lines 20-22) and the conflict remedy instructions (line 208) refer to `cli-ledger-cleanup` or imply the check is … |
| 6 | #472 | `src/cli-docs-lint/src/origin_markers.rs:114` | The check looks up the detail file using `entry.detail_file` (the full path string from the summary table, e.g. 'todo1.md' or 'sub/todo1.md'), but `read_detail_files` keys the map by `path.file_name()` (just the basename… |
| 7 | #475 | `docs/adr/adr-074-auto-lane-screening-criteria.md:158` | The text states that the 4 new failures combined with the 2 previous ones make 6, which satisfies the rejection condition of 3 or more, but it is unclear if the 2 previous failures were also 'selection errors' as defined… |
| 8 | #475 | `docs/claude-code-web-tasks.md:158` | The note for task 356 states that the marker `claude/nightly-356` will be deleted after task 310 is re-introduced and passes, but the note for task 310 (line 167) states that its marker `claude/nightly-310` will be delet… |
| 9 | #476 | `docs/adr/adr-072-nightly-todo-loop.md:536` | The revision states that the `Diverged` check is not applied to open PRs because their `headRefOid` updates on push, creating a window where it might not match `ls-remote`. However, the original bug (line 532) was caused… |
| 10 | #480 | `docs/claude-code-web-tasks.md:143` | The new rule states that directory-declaration annotations should not list existing files as candidates, yet the example on line 149 (and the table entry on line 165) still references 'todo14.md' as the source for placem… |
| 11 | #482 | `src/cli-nightly-outcome/src/agent_reason.rs:88` | The prefix is stripped but the remainder is not trimmed before screening, so a message like '[NIGHTLY_AGENT_STOP] reason' yields a Stated value starting with a leading space, which is likely unintended for a single-line … |
| 12 | #504 | `config/custom-lint-rules.toml:864` | The `bad_fixture` and `good_fixture` fields in the `[rules.incident]` table point to the same file (`no-manual-hooks-config-path.rs`), which is logically incorrect as a bad fixture should demonstrate a violation while a … |
| 14 | #505 | `docs/claude-code-web-tasks.md:168` | The new task description states the rule detects manual naming containing both PID and timestamp, but the rationale explicitly excludes 'PID alone' naming as out of scope because it is the recommended form; this creates … |
| 15 | #505 | `docs/todo13.md:236` | The claim that `process::id()` appears in 64 places in `src/` is inconsistent with the later claim (line 264) that only 1 place combines PID with a time component; if the rule only fires on PID+time, the 64-count is irre… |
| 16 | #505 | `docs/todo13.md:249` | The plan to replace the sole offending site at `src/cli-nightly-outcome/tests/e2e.rs:217` with `tempfile` is listed as a task, but the completion criteria do not explicitly require that this migration be verified (e.g., … |
| 17 | #507 | `config/custom-lint-rules.toml:997` | The `good_fixture` field is set to the same filename as `bad_fixture` (`no-pid-timestamp-temp-naming.rs`), which is logically incorrect for a lint rule configuration that expects distinct fixtures for positive and negati… |
| 19 | #511 | `src/lib-telemetry/src/lib.rs:99` | Changing the `reason` field from `Option<&'a str>` to `Option<&'static str>` is a breaking API change that will cause compilation errors for any existing callers passing non-static string slices (e.g., from `String` or o… |
| 20 | #512 | `src/cli-pr-monitor/src/stages/coderabbit_reviewed.rs:99` | The jq filter `select(.user.login=="coderabbitai[bot]")` is fragile because the bot's login name is not guaranteed to be exactly `coderabbitai[bot]` (it can vary by installation or be just `coderabbitai`); a mismatch wou… |
| 21 | #513 | `.github/workflows/review-request.yml:434` | The `reviewed_via_commit_status` function queries the legacy `/commits/{sha}/status` endpoint, which only returns the combined status of the *first* commit in the PR (or the base if the PR is empty), not the current HEAD… |
| 22 | #513 | `.github/workflows/review-request.yml:446` | The `reviewed_via_reviews_api` function filters reviews by `user.login == "coderabbitai[bot]"`, but CodeRabbit typically posts reviews as a GitHub App (e.g., `coderabbitai[bot]` is correct for comments, but reviews might… |
| 23 | #538 | `src/cli-docs-lint/src/rustdoc_links.rs:129` | The test `a_link_one_level_too_shallow_is_detected` uses a link with 4 `../` segments (`../../../../docs/...`) from a file at depth 4 (`src/crate/src/stages/gate/mod.rs`), which correctly resolves to the root `docs/` dir… |
| 25 | #551 | `.github/workflows/nightly-todo.yml:558` | The command `master-ref/target/release/cli-nightly-task-select ... >> "$GITHUB_OUTPUT"` appends the CLI's stdout directly to the output file; if the CLI prints any non-key=value lines (e.g., logs, warnings, or the rank i… |

### gemma-nothink

**正解と同じ問題を指摘した候補**

| 正解 | 重大度 | 正解の要約 | fid | 候補の指摘 |
|---|---|---|---|---|
| 475-3933467149 | Major | 却下条件を満たした後の判断を反映してください。 | 13 | The logic for the trial evaluation is contradictory: the text states that the failure criteria (3 or more errors) were met with 6 errors, but then notes that the proposed mitigation ('narrowing to test additions only') w… |

**正解外で妥当と判定した指摘**

| fid | PR | ファイル:行 | 指摘 |
|---|---|---|---|
| 5 | #470 | `scripts/ledger-residue-scan.mjs:94` | If `spawnSync` fails to execute the child process (e.g., `run-artifact.mjs` is missing or not executable), `scan.status` will be `null`, causing the script to exit with `EXIT_ERROR` (2) instead of a more descriptive erro… |
| 8 | #472 | `docs/defect-convergence-plan.md:3` | The summary text states 'Phase F 完了' (Phase F complete), but the table and subsequent text show '機4b' is '未着手' (not started), which contradicts the claim that Phase F is finished if Phase 4 is part of Phase F. |

**ノイズと判定した指摘**

| fid | PR | ファイル:行 | 指摘 |
|---|---|---|---|
| 0 | #465 | `src/cli-push-runner/src/stages/takt_verdict/detect.rs:91` | The `strip_document_fence` function uses `lines.into_iter().take(end).skip(start)`, but `end` is the index of the closing fence. Since `take(n)` takes the first `n` elements, this will include the closing fence line itse… |
| 1 | #466 | `docs/defect-convergence-plan.md:3` | The status summary claims 'Phase 4 (機4)' is next, but the table shows '機4' is '未着手' (Not Started) while '機3' is marked as '完了 (本 PR)' (Completed). However, the summary also lists '機3' as completed in the status list, cre… |
| 2 | #466 | `docs/defect-convergence-plan.md:3` | The status line claims 'Phase F 完了' (Phase F Completed), but the table shows '機3' (part of Phase 3/F) was just completed in this PR, which is consistent, but the transition to 'Phase 4' should be verified against the act… |
| 3 | #466 | `src/cli-branch-cleanup/src/classify.rs:114` | The use of `split(' ').next().unwrap_or("")` is safe, but if the input line contains only a tab and no SHA, it returns an empty string which is then correctly caught by `is_object_id`. |
| 4 | #469 | `src/cli-ledger-residue-scan/src/main.rs:137` | The check `array.len() >= limit` causes a false error if the number of merged PRs is exactly equal to the limit. It should be `array.len() > limit` to allow the scan to proceed when the result set is complete but matches… |
| 6 | #471 | `scripts/rebase-nightly-pr.mjs:284` | The script uses `process.execPath` to run `run-artifact.mjs`, but it passes the script path as a positional argument. Since `run-artifact.mjs` is a Node.js script, it must be invoked as `node scripts/run-artifact.mjs ...… |
| 7 | #471 | `scripts/rebase-nightly-pr.mjs:284` | The `check` execution (line 284) uses `run` which has a 120s timeout. If the ledger removal check takes longer than 2 minutes, the script will hang/fail without a clear error message from the tool itself. |
| 9 | #472 | `src/cli-docs-lint/src/origin_markers.rs:53` | The `take_while(char::is_ascii_digit).count()` logic might fail to correctly identify a run ID if it is immediately followed by non-digit characters without a space, though the `split("run ")` helps mitigate this. |
| 10 | #473 | `docs/adr/adr-015-push-runner-takt-migration.md:134` | The 'docs-only push' metric shows a median of 3.0 minutes, which is significantly higher than the 1-minute target, and the text notes this as '未達' (not achieved). |
| 11 | #473 | `docs/claude-code-web-tasks.md:159` | The documentation for task 281 includes a long note about moving it to a human lane due to permission denials, but it repeats the exact same reasoning as task 455, which might be redundant or confusing for readers. |
| 12 | #473 | `docs/todo26.md:107` | The documentation notes that GitHub slug generation should not collapse consecutive spaces, which is a specific requirement to prevent false positives in anchor verification. |
| 14 | #475 | `docs/claude-code-web-tasks.md:156` | The task status for rank 199 is changed from `✅` to `—`, but the explanation for why it is no longer 'completed' (due to a contradiction with `todo10.md`) is only added to the summary table at the end of the file, which … |
| 15 | #480 | `docs/claude-code-web-tasks.md:149` | The example block (lines 147-150) contains a commented-out line that contradicts the new rule (line 143) which forbids listing candidate files in annotations. While it is an example of what NOT to do, it might confuse us… |
| 16 | #482 | `src/cli-nightly-outcome/src/agent_reason.rs:65` | The filter for `assistant` messages uses `entry.get("type")`, but the JSON schema described in lines 44-45 implies `type` is a field within the `message` object, not the top-level entry. This will cause all assistant mes… |
| 17 | #504 | `config/custom-lint-rules.toml:842` | The regex pattern `[\s\S]{0,400}?` is potentially dangerous and may cause catastrophic backtracking or excessive memory consumption when scanning large Rust files, especially if the `current_dir()` call is followed by a … |
| 18 | #505 | `docs/claude-code-web-tasks.md:168` | The documentation states that PID-only naming is excluded because it is recommended by rule 455, but it also notes that there are 64 such instances in `src/`. This creates a potential conflict where the lint rule explici… |
| 19 | #505 | `docs/todo13.md:223` | The reference path for `custom-lint-rules.toml` changed from `.claude/custom-lint-rules.toml` to `config/custom-lint-rules.toml`. Ensure this reflects the actual repository structure change. |
| 20 | #507 | `config/custom-lint-rules.toml:976` | The regex pattern uses `[^;]{0,400}?` which can lead to catastrophic backtracking or excessive scanning if a large block of code exists between the PID and the timestamp without a semicolon. While the limit of 400 is pre… |
| 21 | #512 | `src/cli-pr-monitor/src/stages/coderabbit_reviewed.rs:97` | The `gh api` command uses `--paginate` with a `select` filter in `jq`. If the number of reviews is large, `gh` will fetch all pages, but the `jq` filter is applied to the resulting stream. While functional, this can be e… |
| 22 | #513 | `.github/workflows/review-request.yml:434` | The `ascii_downcase` check on the context `coderabbit` is good for robustness, but ensure the actual GitHub status context is indeed lowercase or matches this expectation. |
| 23 | #538 | `src/cli-docs-lint/src/rustdoc_links.rs:129` | The test case `a_link_one_level_too_shallow_is_detected` contains a logic error in its assertion: the link provided has 4 levels of `../` (`../../../../`), which is actually deeper than the 3 levels required for `crate/s… |
| 24 | #551 | `.github/workflows/nightly-todo.yml:558` | The `cli-nightly-task-select` command uses `>> "$GITHUB_OUTPUT"` to write to the output file, but the command itself is part of a shell script that might produce multiple lines of output. If the executable outputs multip… |
