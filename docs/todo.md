# TODO

> **運用ルール**: 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイル + [docs/todo3.md](todo3.md) 〜 [docs/todo28.md](todo28.md) + [docs/todo-summary.md](todo-summary.md) + [docs/todo-summary2.md](todo-summary2.md) + [docs/todo-summary3.md](todo-summary3.md) の使い分け** (todo2.md は 2026-08-12 退役) (PR #83 T3-2 で恒久化、2026-04-28 強化、PR #88 で todo3.md 追加、PR #96 セッションで todo4.md 追加、PR #101 セッションで todo5.md 追加、PR #123 セッションで todo6.md 追加、2026-05-09 に todo-summary.md 切り出し + todo5.md 分割で todo7.md 追加、PR #143 = 2026-05-11 で todo8.md 追加、PR #172 仕組み化方針切替 = 2026-05-25 で todo9.md 追加、PR #185 land 後 2026-05-29 で todo10.md 追加、2026-06-06 todo9.md 分割で todo11.md 追加、2026-06-12 PR #204 で todo10.md 分割により todo12.md 追加、2026-06-29 PR #224 セッションで todo13.md 追加、2026-07-19 週次レビュー WR-2026-07-19-T02 採用で todo14.md 追加、2026-07-20 docs 50KB 超過解消で todo13.md を todo15/16/17・todo10.md を todo18/19 へ物理分割、2026-08-04 todo14.md の 50KB 超過で todo20.md 追加、2026-08-08 todo20.md の 50KB 超過で todo21.md 追加、2026-08-11 todo21.md の 50KB 超過で todo22.md 追加、2026-08-13 todo22.md の 50KB 超過で todo23.md 追加、2026-08-16 todo23.md の 50KB 超過で todo24.md 追加、2026-09-19 todo.md の閾値接近 (50406 B) で todo28.md 追加):
>
> - **docs/todo-summary.md**: 推奨実行順序サマリー table 専用 (旧 todo.md から切り出し)、順位 6-219 を収容。既存行編集・順位再採番はここで行う。
> - **docs/todo-summary2.md**: todo-summary.md の table を 2026-07-20 に docs 50KB 超過解消で分割した part 2 (順位 220-399 を収容)。**新規行は追加しない** (2026-09-03 に再分割し、追加先は todo-summary3.md へ移った)。既存行の編集・完了削除専用。
> - **docs/todo-summary3.md**: todo-summary2.md の table を 2026-09-03 に docs 50KB 超過解消で再分割した part 3 (順位 400 以降を収容)。**新規行追加は末尾 = 本ファイルで行う**。cli-docs-lint の priority-inversion / preamble / entry-pairing / origin-markers は `todo-summary*.md` を glob して全 part を統合検査する。
> - **docs/todo.md**: 既存タスクの編集・完了削除専用。新規タスクの**詳細エントリ**は追加しない (~50KB 閾値内に維持し Claude Code 読み取り安定性を確保)
> - **docs/todo2.md**: **退役済み (2026-08-12 削除)**。主内容の ADR-032 ブロックは ADR-057 が別設計で実現したため廃止、独立価値の残る 2 タスクは todo22.md へ移送した (経緯は git log)
> - **docs/todo3.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (50KB に到達したため、PR #96 セッション以降の新規エントリは todo4.md へ)
> - **docs/todo4.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (50KB に到達したため、PR #101 セッション以降の新規エントリは todo5.md へ)
> - **docs/todo5.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (2026-05-09 に古い半分を todo7.md へ分割。PR #115 以降のエントリのみ残存。新規エントリは todo6.md へ)
> - **docs/todo6.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (50KB に到達したため、PR #143 = 2026-05-11 以降の新規エントリは todo8.md へ)
> - **docs/todo7.md**: 既存タスクの編集・完了削除専用 (旧 todo5.md の PR #101〜#109 エントリを 2026-05-09 に分割移動)。**新規タスクは追加しない**
> - **docs/todo8.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (60KB に到達したため、PR #172 仕組み化方針切替 = 2026-05-25 以降の新規エントリは todo9.md へ)
> - **docs/todo9.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (50KB 超 1100+ 行に到達したため、PR #185 land 後 2026-05-29 以降の新規エントリは todo10.md へ。2026-06-06 に PR-specific follow-up entries を todo11.md へ分離)
> - **docs/todo10.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (約95KB に到達したため、PR #224 セッション = 2026-06-29 以降の新規エントリは todo13.md へ。2026-06-12 PR #204 で PR #185〜#196 era のエントリを todo12.md へ分離。2026-07-20 に順位 215-224 を todo18/todo19 へ物理分割し 50KB 以下に縮小)
> - **docs/todo11.md**: 既存タスクの編集・完了削除専用 (2026-06-06 todo9.md 分割で新設、PR-specific follow-up entries 収容)。**新規タスクは追加しない**
> - **docs/todo12.md**: 既存タスクの編集・完了削除専用 (2026-06-12 PR #204 で todo10.md 分割により新設、PR #185〜#196 era のエントリ収容)。**新規タスクは追加しない**
> - **docs/todo13.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (約171KB に到達したため、週次レビュー WR-2026-07-19-T02 採用 = 2026-07-19 以降の新規エントリは todo14.md へ。2026-07-20 に順位 248-332 を todo15/todo16/todo17 へ物理分割し 50KB 以下に縮小)
> - **docs/todo14.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (約 70KB に到達したため、2026-08-04 WP-17 段 2 完了時の post-merge feedback 一括登録以降の新規エントリは todo20.md へ)
> - **docs/todo15.md**: 既存タスクの編集・完了削除専用 (2026-07-20 todo13.md 分割で新設、順位 248-296 収容)。**新規タスクは追加しない**
> - **docs/todo16.md**: 既存タスクの編集・完了削除専用 (2026-07-20 todo13.md 分割で新設、順位 297-318 収容)。**新規タスクは追加しない**
> - **docs/todo17.md**: 既存タスクの編集・完了削除専用 (2026-07-20 todo13.md 分割で新設、順位 319-332 収容)。**新規タスクは追加しない**
> - **docs/todo18.md**: 既存タスクの編集・完了削除専用 (2026-07-20 todo10.md 分割で新設、順位 215-219 収容)。**新規タスクは追加しない**
> - **docs/todo19.md**: 既存タスクの編集・完了削除専用 (2026-07-20 todo10.md 分割で新設、順位 220-224 収容)。**新規タスクは追加しない**
> - **docs/todo20.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (2026-08-04 todo14.md の 50KB 超過で新設・順位 365-388 を収容、2026-08-08 に本ファイルも 50KB 超過で新規追加先は todo21.md へ移行)
> - **docs/todo21.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (約57KB に到達したため、2026-08-11 以降の新規エントリは todo22.md へ。2026-08-08 todo20.md の 50KB 超過で新設、順位 385 以降を収容)
> - **docs/todo22.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (約 66KB に到達したため、2026-08-13 以降の新規エントリは todo23.md へ。2026-08-11 todo21.md の 50KB 超過で新設)
> - **docs/todo23.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (52690B に到達したため、2026-08-16 以降の新規エントリは todo24.md へ。2026-08-13 todo22.md の 50KB 超過で新設、週次レビュー WR-2026-08-13-M01 採用)
> - **docs/todo24.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (50869B = 閾値まで残り 331B に到達したため、2026-08-22 以降の新規エントリは todo25.md へ。2026-08-16 todo23.md の 50KB 超過で新設)
> - **docs/todo25.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (2026-08-22 todo24.md の閾値接近で新設、週次レビュー 2026-08-22 実行セッションで検出。2026-08-23 に 50121 B = 閾値まで残り 1079 B へ到達し todo26.md へ移行)
> - **docs/todo27.md**: todo14.md / todo22.md が 50KB を超えたため、2026-09-03 に両ファイルの大きいエントリ 10 件を移した先 (順位 512)。**新規タスクは追加しない** — 移送したエントリの編集・完了削除専用。
> - **docs/todo28.md**: docs/todo.md が閾値に接近したため、2026-09-19 に「週次レビュー採用」節 4 節 (2026-08-13 / 2026-07-19 / 2026-07-01 / 2026-06-01) を移した先。**新規タスクは追加しない** — 移送したエントリの編集・完了削除専用。
> - **docs/todo26.md**: 新規タスクの追加先。50KB に到達するまでは本ファイルへ追加 (2026-08-23 todo25.md の閾値接近で新設)
> - 例外: 既存 todo.md / todo3.md 〜 todo28.md タスクと **同一ファイル / 同一コンポーネント** を編集する密結合タスクは該当ファイルに追加可 (例: `~/.claude/rules/common/git-workflow.md` 配下のグローバルルール群)
> - **新セッションでは全 todo ファイルを確認すること** (todo.md / todo3-28.md / todo-summary.md / todo-summary2.md / todo-summary3.md。todo2.md は 2026-08-12 退役)

---

> **推奨実行順序サマリー**: [`docs/todo-summary.md`](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### 週次レビュー採用 (2026-08-15)

> 2026-08-15 の週次レビュー (whole-tree, ADR-031) で採用した findings。詳細レポートは `.claude/weekly-reviews/2026-08-15.md`。検出 14 件のうち 8 件を採用、6 件 (C01/C02/C04/C05/A05/J03) は却下した。

#### file-length 800 行閾値の single source of truth 化 (週次レビュー WR-2026-08-15-A01 採用)

> **動機**: 800 行閾値が `.claude/hooks-config.toml` `[file_length_gate]`、`src/hooks-post-tool-comment-lint-rust/src/modified_files_check.rs` (`MAX_FILE_LINES=800`)、`src/cli-push-runner/src/stages/pr_size_check.rs` (別建ての 800/1500 行 PR 範囲チェック)、convention 集 (2026-09-13 廃止、内容は [ADR-080](adr/adr-080-rust-module-split-invariants.md) へ移設) の 4 箇所に独立定義されている。さらに 50KB の `file_size_check` と 800 行の `file_length_gate` という別物の閾値が、役割の違いを文書化しないまま混在している。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-08-15-A01 で採用 (severity=high, facet=architecture, category=docs-source-drift)
>
> **参照**: `.claude/weekly-reviews/2026-08-15.md`、`.claude/hooks-config.toml` (`[file_length_gate]`)

##### 背景: [ADR-081](adr/adr-081-single-fact-dispersion.md) 自身が「同一事実が複数箇所に分散する場合の変更手順」を anti-pattern として明記しており、本件はその実例に該当する

##### 設計決定: 800 行定数を共有 `lib-*` crate へ集約し、`modified_files_check.rs` と `pr_size_check.rs` の双方から参照する

- [ ] 800 行定数を共有 crate へ抽出し 2 箇所から参照する
- [ ] `file_size_check` (edit 時 50KB) と `file_length_gate` (push 時 800 行) が意図的に別フェーズなのか redundant なのかを ADR-039 に明記する
- [ ] `.takt/facets/instructions/file-length-watchlist.md` から authoritative source へ逆参照を張る

##### 完了基準: 800 行という数値がコード上 1 箇所にのみ存在し、他の参照点がすべてそこを指す。2 種の閾値の役割差が ADR-039 に記述されている

#### weekly-review reminder 閾値の共有定数化と値の test 固定 (週次レビュー WR-2026-08-15-A02 採用)

> **動機**: `reminder_threshold_days` が `src/hooks-session-start/src/weekly_review.rs` の Rust default、`.claude/hooks-config.toml:61`、ADR-070 の決定本文、ADR-059 の 4 箇所以上に同期機構なしで分散している。[ADR-081](adr/adr-081-single-fact-dispersion.md) § コンテキスト は 2026-08-13 に code default (30) と config (7) が実際に乖離し手動で調整した incident を記録済み。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-08-15-A02 で採用 (severity=high, facet=architecture, category=docs-source-drift)
>
> **参照**: `.claude/weekly-reviews/2026-08-15.md`、`.claude/hooks-config.toml:61`、[ADR-081](adr/adr-081-single-fact-dispersion.md) § コンテキスト (2026-08-13 の乖離 incident)

##### 背景: 実際に乖離した実績のある分散定義。WR-2026-08-15-A01 と同じ SSOT 欠如の系統だが、こちらは incident が既に起きている点で優先度が高い

##### 設計決定: `WEEKLY_REVIEW_REMINDER_THRESHOLD_DAYS` を `src/hooks-session-start/src/lib.rs` の共有定数として抽出し、config の default deserialization から参照する

- [ ] `WEEKLY_REVIEW_REMINDER_THRESHOLD_DAYS` を `src/hooks-session-start/src/lib.rs` に定義
- [ ] config の default 値解決を当該定数経由に変更
- [ ] `.claude/hooks-config.toml` に定数の所在を指す TOML コメントを追加
- [ ] 定数が文書化された値 (7) と一致することを assert する test を追加

##### 完了基準: `cargo test` が定数値 = 7 を固定しており、code default と config の乖離が test で検出される

#### lint rule ⑥ の拡張子リスト/テスト同期義務を ADR-007 へ昇格 (週次レビュー WR-2026-08-15-A03 採用)

> **動機**: lint rule ⑥ (`no-ephemeral-todo-reference`) の拡張子リストとテスト同期の義務が `.claude/custom-lint-rules.toml:257-265` の TOML コメントにしか書かれておらず、ADR-007・テストモジュール自身のいずれにも無い。新しい拡張子を追加した開発者が `rule_test_coverage_check` を回さずローカル `cargo test` を通し、必要なテストなしでマージし得る — ADR-007 § Lint rule 最小テストチェックリストが警告している当の anti-pattern。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-08-15-A03 で採用 (severity=medium, facet=architecture, category=harness-duplication)
>
> **参照**: `.claude/weekly-reviews/2026-08-15.md`、`.claude/custom-lint-rules.toml:274` (`.claude/custom-lint-rules.toml:257-265`)

##### 背景: 列挙リストとテスト義務がセットで動く pattern は他ルールにも再利用可能だが、現状は 1 ルールの TOML コメントに閉じている

##### 設計決定: ADR-007 に「列挙 + テスト義務」pattern を再利用可能な形で追補し、コード側からも逆参照を張る

- [ ] ADR-007 § Case study に本 pattern を追補する
- [ ] `src/hooks-post-tool-linter/src/main.rs` の該当テストモジュールに TOML 行を指す doc comment を追加
- [ ] (長期・ADR-042 スコープ) `rule_test_coverage_check` が拡張子リストを TOML から直接抽出する案を検討

##### 完了基準: 拡張子を追加した開発者が ADR-007 かコード上の doc comment のどちらからでもテスト義務に到達できる

#### ADR-031 の reminder 閾値「既定 30 日」記述を 7 日へ訂正 (週次レビュー WR-2026-08-15-A04 採用)

> **動機**: ADR-031 の 2026-08-04 更新節が SessionStart reminder を「監査リマインダー (既定 30 日)」と記述しているが、`.claude/hooks-config.toml:61` は `reminder_threshold_days=7` で、ADR-070 が 7 日を恒久値として確定している (30 日案は検討のうえ却下)。ADR-031 のテキストが実装値に対して stale。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-08-15-A04 で採用 (severity=medium, facet=architecture, category=adr-alignment)
>
> **参照**: `.claude/weekly-reviews/2026-08-15.md`、`docs/adr/adr-031-weekly-review-pipeline.md:7`

##### 背景: WR-2026-08-15-A02 の分散定義のうち「文書側の値がずれている」分。A02 の定数化とは独立にテキスト訂正だけで解消する

##### 設計決定: ADR-031 の 2026-08-04 更新節を 7 日恒久 (ADR-070 準拠) に訂正し、30 日が却下された理由を短く注記する

- [ ] ADR-031 の該当記述を 7 日へ訂正
- [ ] 30 日案が ADR-070 で却下された経緯を 1〜2 行で注記

##### 完了基準: ADR-031 の記述が `.claude/hooks-config.toml` の実装値および ADR-070 の決定と一致する

#### lib-ledger の repo_root() をコンパイル時パスから実行時探索へ (週次レビュー WR-2026-08-15-J01 採用)

> **動機**: `src/lib-ledger/src/deployed_ledger.rs:30-34` の `repo_root()` が `env!("CARGO_MANIFEST_DIR")` に `"../.."` を join したコンパイル時絶対パスで解決している。workspace を移動/改名した場合や、workspace コピー間で `target/` を共有した場合 (ADR-045 のシナリオ)、コンパイル時に焼き込まれたパスが解決できず `read_ledger()` が「台帳を読めません」で panic する。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-08-15-J01 で採用 (severity=medium, facet=jj-robustness, category=jj-manifest-dir)
>
> **参照**: `.claude/weekly-reviews/2026-08-15.md`、`src/lib-ledger/src/deployed_ledger.rs:30-34`
>
> **Status update (2026-09-18)**: 2026-09-18 の週次レビューで **severity=high として再検出**
> (WR-2026-09-18-J02、facet=jj-robustness、category=jj-manifest-dir)。採用から約 5 週間、下のチェックリストは
> 3 項目とも未着手。今回の facet は panic 経路として (a) 非コロケーテッド jj workspace、
> (b) 共有 `target/` での並列 workspace 構成 を新たに挙げ、修正方針として
> `lib_jj_helpers::resolve_main_workspace_root()` 相当の共通ロジックへ寄せる案を出している
> (下の「設計決定」の marker 上方探索と同趣旨だが、**探索ロジックを自前で持たず共通化する**点が追加)。
> 重複起票を避けるため新規エントリは作らず、本エントリへ集約した (週次レビュー 2026-09-18 の採否判断)

##### 背景: `CARGO_MANIFEST_DIR` のコンパイル時読みは ADR-045 が明示する脆弱性リストの 1 つ。現状は panic = fail-closed なので silent 破壊ではないが、脆弱性そのものは残る

##### 設計決定: `std::env::current_dir()` から `.git` / `.claude` marker を上方探索し、fallback として `std::env::var()` で実行時に `CARGO_MANIFEST_DIR` を読む

- [ ] `repo_root()` を marker 上方探索ベースに置換
- [ ] fallback を `env!()` から `std::env::var()` へ変更
- [ ] workspace 移動を模したテストで解決が壊れないことを固定

##### 完了基準: workspace を移動/改名しても `read_ledger()` が panic せず台帳を解決できる

#### custom_rules/coverage.rs の CARGO_MANIFEST_DIR 実行時解決 (週次レビュー WR-2026-08-15-J02 採用)

> **動機**: `src/hooks-post-tool-linter/src/custom_rules/coverage.rs:22-28,68-69` の `load_deployed_custom_rules()` と `extract_existing_test_fn_names()` (いずれも `#[cfg(test)]`) が `env!("CARGO_MANIFEST_DIR")` でコンパイル時にパスを解決しており、WR-2026-08-15-J01 と同一の hazard を持つ。workspace 移動/改名や `target/` 共有で panic する。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-08-15-J02 で採用 (severity=medium, facet=jj-robustness, category=jj-manifest-dir)
>
> **参照**: `.claude/weekly-reviews/2026-08-15.md`、`src/hooks-post-tool-linter/src/custom_rules/coverage.rs:22-28,68-69`

##### 背景: J01 と同一パターンだが別ファイル・テスト専用コードのため別 finding として追跡する。J01 の修正方針が決まれば機械的に適用できる

##### 設計決定: 両関数の `env!("CARGO_MANIFEST_DIR")` を `std::env::var(...)` による実行時解決へ置換する

- [ ] `load_deployed_custom_rules()` の解決を実行時化
- [ ] `extract_existing_test_fn_names()` の解決を実行時化

##### 完了基準: workspace 移動後も当該 2 関数を含むテストが panic せず通る

#### lint-screen eval E2E に閾値 assertion を入れる判断 (週次レビュー WR-2026-08-15-S01 採用)

> **動機**: `src/cli-finding-classifier/tests/lint_screen_evals/e2e.rs:53-84` の `run_lint_screen_against_all_fixtures` は Ollama + lint-screen の全 pipeline を全 eval fixture に対して実行するが assertion が 1 つも無く、metrics を人間解釈用に print するだけ (line 76)。LLM の判定精度を劣化させる PR が、誰かが出力を目視しない限り無言で通る。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-08-15-S01 で採用 (severity=medium, facet=simplicity, category=test-anti-pattern)
>
> **参照**: `.claude/weekly-reviews/2026-08-15.md`、`src/cli-finding-classifier/tests/lint_screen_evals/e2e.rs:53-84`

##### 背景: 現状は `#[ignore]` + `LINT_SCREEN_EVALS` env による opt-in で、ADR-038 が本テストを「検証ゲートではなく実験的計測ツール」と位置づけている。即時対応は不要という判断が finding 自身に含まれる

##### 設計決定: ADR-038 の試験運用ステータスが解けて CI 昇格へ動く時点で `report_summary` (line 83) に閾値 assertion (例: `assert!(f1_score >= BASELINE_F1)`) を入れる

- [ ] ADR-038 の試験運用判定の出口条件に「eval E2E の assertion 化」を紐づける
- [ ] CI 昇格時に baseline F1 を決めて `report_summary` へ assertion を追加

##### 完了基準: ADR-038 の採否が確定した時点で、本テストが計測専用のままか assertion 付き検証ゲートかが明示的に決まっている

#### INJECTION_SIGNALS 語彙を dogfood 観測から拡充する (週次レビュー WR-2026-08-15-C03 採用)

> **動機**: `src/cli-finding-classifier/src/lib.rs:84-102` の `INJECTION_SIGNALS` (17 文字列) は意図的に非網羅で、間接命令形 (「assuming this is false positive」)、suffix-injection 変種、テンプレート置換 (「this is ${action} recommendation」) といった既知の回避パターンを捕捉しない。層 1 は補助的かつ fail-open で、層 2 (fix.md allowlist) と層 3 (scope guard, fail-closed) が主防御。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-08-15-C03 で採用 (severity=low, facet=security, category=prompt-injection)
>
> **参照**: `.claude/weekly-reviews/2026-08-15.md`、`src/cli-finding-classifier/src/lib.rs:84-102`、ADR-054 (prompt injection 信頼境界の 3 層防御)

##### 背景: 回帰ではなく ADR-054 が設計として織り込んだ運用モデル。語彙は dogfood で観測した実例から育てる前提になっている

##### 設計決定: dogfood 中に観測した回避試行を記録し、対応する test fixture とセットで `INJECTION_SIGNALS` に追加していく

- [ ] 観測した回避試行を記録する運用先を決める (feedback-reports / todo のいずれか)
- [ ] 観測実例が出た時点で fixture 付きで語彙へ追加

##### 完了基準: 回避試行が観測された際に、語彙追加と fixture 追加がセットで行われる経路が確立している

### 順位 28: (追って) ADR-030 の takt-test-vc 反映

> **参照**: [ADR-030](adr/adr-030-deterministic-post-merge-feedback.md)。
>
> **Status update (2026-08-12)**: 上位タスク「マージ後フィードバック機構の決定論化」は決着してエントリ削除済み — Phase A〜C は実装済みで 3 か月以上安定稼働 (feedback report が全マージで生成)、Phase E (旧機構廃止) は**撤回** (skill / hooks-stop-feedback-dispatch / lib-pending-file は現役稼働のため。経緯は ADR-030 § 撤回記録 2026-08-12)、Phase F (dogfood 検証) は長期運用実績で充足。本タスクの前提「Phase F 完了」は解消済みで、着手は任意。
>
> **実行優先度**: ⏳ **Tier 5** — 派生プロジェクトへの展開で本リポジトリへの効果はゼロ。任意タスク。

- **やろうとしたこと**: 本プロジェクトで安定稼働している ADR-030 機構を takt-test-vc へ機構ごとバックポート
- **現在地**: 着手可能 (前提は解消済み)
- **詰まっている箇所**: なし

---

## スコープ外だが将来検討

### ADR-027 / PR #47 由来

- [ ] **post-pr-monitor の re-push 時ポーリング問題**: re-push 後に CodeRabbit の新しいレビュー (新しい commit に対するレビュー) を待たずに旧状態で即判定している。PR 作成時は初回レビュー投稿を検出できるが、re-push 時は `new_comments: 0` で即 approved → 新レビューを見逃す。対策案: ポーリング開始前に「push 後の新しい review comment が来るまで待機」するロジックの追加 (commit SHA の比較等)
- [ ] **analyze-coderabbit.md と fix.md の read-only zone 定義の齟齬**: analyze ステップは `.takt/workflows/` を「人間が編集する源泉だから read-only zone ではない」と判断して finding を applicable とするが、fix ステップは `.takt/workflows/**` を ABSOLUTE read-only として修正不可。結果として misdirected finding が 1 iteration 分のコストを浪費する。対策案: analyze 側で `.takt/` 全体を not_applicable にするか、fix 側で `.takt/workflows/` を編集可能にするかの二者択一

### ADR-019/020 由来

ADR-019 および ADR-020 の「次ステップ」セクションで明記された未着手項目:

- [ ] **analyze instruction の強化**: ADR を自動検索して filter ルールを動的に抽出
- [ ] **Learning と ADR の双方向同期**: ADR を更新したら CodeRabbit Learning にも通知
- [ ] **他 AI レビュー統合**: Copilot review, Greptile などを ADR-019 の 3 レイヤー構成に乗せる
- [ ] **instruction 参照整合性 lint**: workflow YAML の `instruction:` 参照先と facets 実ファイルの存在を突合
- [ ] **verdict 値の整合性 lint**: workflow の `condition` 値と instruction の出力例の一致を検証 (PR #41 CodeRabbit Major 指摘の再発防止)
- [ ] **takt-test-vc への還元**: 共通 facets パターンを takt のサンプルリポジトリにも反映

### Skill 運用基盤由来

- [ ] **skill evals の自動 runner 統合**: `E:\work\claude-code-skills` 配下 skill の `evals.json` / `trigger_eval.json` を skill-creator:skill-creator や `/skill-sync-check` に乗せて定期実行する仕組み。現状は手動実行のみ。prepare-pr の試験運用評価 (分離前後の発火頻度比較・フロー完了率・draft 初稿品質) の定量データ集計にも必要

### ADR-022 v3 (2026-04-21 改訂) 由来

- [ ] **takt fix による最終 commit message 草案生成機能の実装**: child commit の description が `fix(review): apply CodeRabbit fixes for #<PR>` のように「機械ログ化」して人間が読む価値が薄い問題を緩和する。takt fix の report phase で「最終的に人間が採用する統合 commit message の草案」を `.takt/runs/*/reports/final-commit-message-draft.md` 等に書き出し、`prepare-pr` skill が起動時にこれを読み込んで draft 初稿の元ネタとする。ADR-022 原則 1 改訂版の「草案生成」で正面から許可されており、別 PR で実装
- [ ] **auto-rebase / auto-squash / auto-format commit history の検討**: ADR-022 原則 1 改訂版の緩和条項 (可逆・事前ポリシー・意図不変・PR 外) を満たす範囲で将来実装可能。必要になった時点で別 ADR を作成し運用ポリシーを明示してから実装

### ADR-022 原則 5 (PR 包含 changeset の不変性) 由来

- [ ] **interactive Claude Code の amend 挙動を "PR 包含チェック" で gate する実装**: `pnpm push` (cli-push-runner) または Claude Code session 側で、`@` bookmark が open PR に紐付いているかを `gh pr list --head <bookmark> --state open --json number` で判定。紐付いている場合は `jj describe` やファイル edit による auto-amend を警告 or 自動的に child commit に切り替える。紐付いていない場合は現行通り amend 許可。takt fix は task 4 (PR #63) で既に child commit 化済のため対象は interactive 経路。設計段階、未着手
