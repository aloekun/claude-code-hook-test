# TODO

> **運用ルール**: 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイル + [docs/todo3.md](todo3.md) 〜 [docs/todo23.md](todo23.md) + [docs/todo-summary.md](todo-summary.md) + [docs/todo-summary2.md](todo-summary2.md) の使い分け** (todo2.md は 2026-08-12 退役) (PR #83 T3-2 で恒久化、2026-04-28 強化、PR #88 で todo3.md 追加、PR #96 セッションで todo4.md 追加、PR #101 セッションで todo5.md 追加、PR #123 セッションで todo6.md 追加、2026-05-09 に todo-summary.md 切り出し + todo5.md 分割で todo7.md 追加、PR #143 = 2026-05-11 で todo8.md 追加、PR #172 仕組み化方針切替 = 2026-05-25 で todo9.md 追加、PR #185 land 後 2026-05-29 で todo10.md 追加、2026-06-06 todo9.md 分割で todo11.md 追加、2026-06-12 PR #204 で todo10.md 分割により todo12.md 追加、2026-06-29 PR #224 セッションで todo13.md 追加、2026-07-19 週次レビュー WR-2026-07-19-T02 採用で todo14.md 追加、2026-07-20 docs 50KB 超過解消で todo13.md を todo15/16/17・todo10.md を todo18/19 へ物理分割、2026-08-04 todo14.md の 50KB 超過で todo20.md 追加、2026-08-08 todo20.md の 50KB 超過で todo21.md 追加、2026-08-11 todo21.md の 50KB 超過で todo22.md 追加、2026-08-13 todo22.md の 50KB 超過で todo23.md 追加):
>
> - **docs/todo-summary.md**: 推奨実行順序サマリー table 専用 (旧 todo.md から切り出し)、順位 6-219 を収容。既存行編集・順位再採番はここで行う。
> - **docs/todo-summary2.md**: todo-summary.md の table を 2026-07-20 に docs 50KB 超過解消で分割した後半 (順位 220 以降を収容)。新規行追加は末尾 = 本ファイルで行う。cli-docs-lint の priority-inversion / preamble は両 summary を統合検査。
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
> - **docs/todo23.md**: 新規タスクの追加先。50KB に到達するまでは本ファイルへ追加 (2026-08-13 todo22.md の 50KB 超過で新設、週次レビュー WR-2026-08-13-M01 採用)
> - 例外: 既存 todo.md / todo3.md 〜 todo23.md タスクと **同一ファイル / 同一コンポーネント** を編集する密結合タスクは該当ファイルに追加可 (例: `~/.claude/rules/common/git-workflow.md` 配下のグローバルルール群)
> - **新セッションでは全 todo ファイルを確認すること** (todo.md / todo3-23.md / todo-summary.md / todo-summary2.md。todo2.md は 2026-08-12 退役)

---

> **推奨実行順序サマリー**: [`docs/todo-summary.md`](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### 週次レビュー採用 (2026-08-13)

> 2026-08-13 の週次レビュー (whole-tree, ADR-031) で採用した findings。詳細レポートは `.claude/weekly-reviews/2026-08-13.md`。J01 (fetch_head mtime) は既存 entry (WR-2026-07-19-J01) と重複のためスキップした。

#### CLAUDE.md の ADR-030 supersedes 注記を撤回済み内容に合わせて削除 (週次レビュー WR-2026-08-13-A01 採用)

> **動機**: `CLAUDE.md:34` が ADR-030 を「Supersedes ADR-014 full, ADR-029 partial」と宣言しているが、ADR-030 自身が 2026-08-12 にこの主張を撤回済み。ADR-014/029 は設計上 ADR-030 と並んで試験運用のまま。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-08-13-A01 で採用 (severity=critical, facet=architecture, category=docs-source-drift)
>
> **参照**: `.claude/weekly-reviews/2026-08-13.md`、`CLAUDE.md:34`

##### 背景: ADR-030 の撤回注記と CLAUDE.md 索引の乖離。1 行の docs 修正で解消する

##### 設計決定: `CLAUDE.md:34` の「Supersedes ADR-014 full, ADR-029 partial」注記を削除し `*(試験運用 / ...)*` のみ残す

- [ ] `CLAUDE.md:34` の該当注記を削除

##### 完了基準: `CLAUDE.md:34` の ADR-030 行が supersedes 主張を含まず、ADR-030 の現状 (撤回済み) と整合する

#### 台帳の ✅無人可 5 行を condition 3 違反により — へ降格 (週次レビュー WR-2026-08-13-T02 採用)

> **動機**: `docs/claude-code-web-tasks.md` の `✅ 無人可` 5 行 (順位 203/216/228/239/240) が § 自律実行可否の condition 3 (重複の恐れなし) に違反。各順位に未マージ PR (#373/#394/#379/#391/#378) が存在し、夜間 todo ループが重複/競合 PR を作りうる。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-08-13-T02 で採用 (severity=high, facet=todo, category=todo-duplicate)
>
> **参照**: `.claude/weekly-reviews/2026-08-13.md`、`docs/claude-code-web-tasks.md` (採用タスク Batch 1/2)

##### 背景: ADR-072 の夜間ループは無人可マークを機械的に読む。マークが古いと重複実装 PR を生む

##### 設計決定: 該当 5 行の `✅ 無人可` を `—` へ降格し、理由を § 無人可としなかった…理由 表に記録する。PR がマージ or 恒久 close + untrack されるまで

- [ ] 順位 203/216/228/239/240 の 無人可 を `—` へ (**人間が実施** — マークは人間が付ける、ADR-022)
- [ ] § 無人可としなかった…理由 表に根拠を追記

##### 完了基準: 当該 5 行が `無人可=—`、理由表に PR 番号付きで記録

#### 台帳 Batch 1 の closed-without-merge 行を棚卸し履歴へ移動し、in-flight を明示 (週次レビュー WR-2026-08-13-T01 採用)

> **動機**: `docs/claude-code-web-tasks.md` Batch 1 の順位 203/228/240 は nightly PR (#373/#379/#378) が unmerged close されたのに active のまま残存。順位 239 は open PR (#391) が in-flight として反映されていない。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-08-13-T01 で採用 (severity=medium, facet=todo, category=todo-dead-entry)
>
> **参照**: `.claude/weekly-reviews/2026-08-13.md`、`docs/claude-code-web-tasks.md` (採用タスク Batch 1)

##### 背景: 台帳の鮮度は「行が消えていること」でしか表現されないため、closed 行の残存が棚卸し漏れになる

##### 設計決定: closed-without-merge 行 (203/228/240) を Batch 1 から削除し § 棚卸し履歴 に closed 理由を記録、open-PR 行 (239) を in-flight として明示する

- [ ] 203/228/240 を Batch 1 から削除 + § 棚卸し履歴 記帳
- [ ] 239 の in-flight 状態を明示

##### 完了基準: Batch 1 に unmerged-close 行が残らず、棚卸し履歴に根拠が残り、順位 239 が in-flight として明示されている

#### docs/todo23.md を新設し、新規追加先ポインタを更新する — todo22.md 50KB 超過 (週次レビュー WR-2026-08-13-M01 採用)

> **動機**: `docs/todo22.md` が 54179B (>50KB) だが `docs/todo.md:30` の新規追加先ポインタが todo22.md のまま。routing 契約が実ファイルサイズに追随していない。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-08-13-M01 で採用 (severity=medium, facet=multi, category=todo-preamble-drift)。file-length-watchlist の機械観測と review-todo-whole の記述矛盾を突合して検出。
>
> **参照**: `.claude/weekly-reviews/2026-08-13.md`、`docs/todo.md:30`、`docs/todo22.md`

##### 背景: todo20→21→22 と同じ 50KB 分割パターンの継続。preamble routing の drift 解消

##### 設計決定: `docs/todo23.md` を作成し、`docs/todo.md:30` の新規追加先を todo23.md へ更新する。todo22.md は「編集専用・新規追加しない」へ

- [ ] todo23.md 新設
- [ ] todo.md preamble (L30 周辺) の routing 更新 — 使い分けリストへの todo23.md 行追加と、冒頭の列挙範囲 (「本ファイル + todo3.md 〜 todoN.md」) の両方 (cli-docs-lint は列挙範囲と実ファイル数の一致を検証しないため手動確認)

##### 完了基準: 新規追加先が todo23.md を指し、todo22.md が編集専用に切り替わり、preamble の列挙範囲が実ファイル群と一致する

#### ADR-031 に ADR-070 (Phase 1-2 の cloud routine 移行) への前方参照を追記 (週次レビュー WR-2026-08-13-A02 採用)

> **動機**: ADR-031 の 4-phase 設計に、ADR-070 (Phase 1-2 の cloud routine 移行) への前方参照が無く、ADR-031 を単独で読むと誤解を招く。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-08-13-A02 で採用 (severity=medium, facet=architecture, category=adr-alignment)
>
> **参照**: `.claude/weekly-reviews/2026-08-13.md`、`docs/adr/adr-031-weekly-review-pipeline.md` (ステータス/abstract)

##### 背景: cross-ADR coupling が documented だが見落としやすい。安価な doc 明確化

##### 設計決定: ADR-031 の status/abstract に ADR-070 参照の Note を追加し、Phase 1-2 の trigger のみが移行した旨を明示する

- [ ] ADR-031 に ADR-070 前方参照 Note を追加

##### 完了基準: ADR-031 単独読者が ADR-070 への移行を辿れる

#### todo.md の Tier-5 zero-priority entry を backlog へ移動 or retire (週次レビュー WR-2026-08-13-T03 採用)

> **動機**: `docs/todo.md:160-171` の Tier-5/optional・zero-priority entry (追って ADR-030 の takt-test-vc 反映) が 2 か月以上進捗なくメイン corpus に残り視覚ノイズになっている。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-08-13-T03 で採用 (severity=low, facet=todo, category=todo-preamble-drift)。aggregate 推奨は ❌却下だったがユーザー判断で採用。
>
> **参照**: `.claude/weekly-reviews/2026-08-13.md`、`docs/todo.md:160-171`

##### 背景: entry 自体は self-aware で正しくスコープされているが、配置がメイン corpus でノイズ

##### 設計決定: 新設 `## Future / Backlog (No Current Priority)` section へ移動する。不要と判断すれば retire

- [ ] entry を backlog section へ移動 or retire 判断

##### 完了基準: 該当 entry がメイン進行中 corpus から外れる

### 週次レビュー採用 (2026-07-19)

> **注 (2026-07-19)**: 本セッションの週次レビューで採用した T01 (docs/todo.md preamble drift) と T02 (todo13.md 50KB 超過 → todo14.md 新設) は、PR #303 の CodeRabbit 対応 (fix commit) で master preamble を 15 ファイルへ全面更新 + todo14.md 新設 + routing 更新まで完了したため、完了タスクとして削除した (`docs/todo.md` preamble / `docs/todo14.md` / `docs/todo-summary.md` に成果が残る)。J01 / J02 はコード修正が未着手のため下記に継続。

#### fetch_head_is_recent() の mtime 依存を埋め込み timestamp に置換 (週次レビュー WR-2026-07-19-J01 採用)

> **動機**: `fetch_head_is_recent()` が `.git/FETCH_HEAD` の mtime のみで fetch 鮮度を判定している。jj workspace 操作 (working copy materialization) で mtime がリセットされると false positive となり、実際は stale でも staleness nudge が発火しない可能性がある。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-07-19-J01 で採用 (severity=high, facet=jj-robustness, category=jj-mtime-staleness)
>
> **参照**: `.claude/weekly-reviews/2026-07-19.md` WR-2026-07-19-J01、`src/hooks-session-start/src/jj_helpers.rs:12-25`、[ADR-039](adr/adr-039-experimental-feature-standard-pattern.md) (jj-robustness facet の bounded lifetime dogfood 文脈)

##### 背景: 本 bug class (jj 操作による mtime リセット) は 2026-07 セッションで実観測済みで、新設 jj-robustness facet (ADR-039 bounded lifetime dogfood) が再検出した good signal。ただし jj new / workspace 操作が実際に `.git/FETCH_HEAD` の mtime を書き換える具体的機序は本レビューで再現検証しておらず、実装前に経験的確認を推奨する

##### 設計決定: mtime 依存を廃し、jj git fetch 成功後に `.claude/fetch-last-run.json` 等へ埋め込みタイムスタンプを書き込み、そこから鮮度判定する方式に置換する (weekly-review last-run / telemetry と同じ「内容 timestamp は checkout 不変」方式、CR #233 の mtime リセット教訓と整合)

- [ ] jj 操作が FETCH_HEAD mtime を書き換える機序を経験的に確認 (前提検証)
- [ ] 埋め込み timestamp 方式へ置換 + mtime リセットを模擬する回帰テスト
- [ ] 本エントリ削除

##### 完了基準: jj workspace 操作後も fetch 鮮度が正しく判定されること (mtime リセット模擬の回帰テストで seal)

#### gh 呼び出しに --repo を付与 — 非 colocated jj workspace の PR 検出 silent 失敗 (週次レビュー WR-2026-07-19-J02 採用)

> **動機**: `detect_owner_repo()` (cli-merge-pipeline/src/github.rs:92-99) および `get_pr_info()` / `find_pr_via_jj_bookmarks()` (cli-pr-monitor/src/util.rs:31-68) が `--repo` 無しで `gh repo view` / `gh pr list` を呼び出しており、非 colocated jj workspace (`.git` 無し) で gh の自動検出が失敗し merge/monitor パイプラインが silent に PR 検出不能となる。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-07-19-J02 で採用 (severity=high, facet=jj-robustness, category=jj-gh-no-repo)
>
> **参照**: `.claude/weekly-reviews/2026-07-19.md` WR-2026-07-19-J02、`src/cli-merge-pipeline/src/github.rs:92-99`、`src/cli-pr-monitor/src/util.rs:31-68`、[ADR-045](adr/adr-045-jj-workspace-parallel-sessions.md)、PR #238 (実インシデント)
>
> **Status update (2026-08-12)**: 部分進捗を確認 — `cli-pr-monitor/src/util.rs` の `get_pr_head_commit()` は `--repo` 付与済み。未対応は同ファイルの `get_pr_info()` (`gh repo view`) / `find_pr_via_jj_bookmarks()` (`gh pr list --head`) と `cli-merge-pipeline/src/github.rs` の `detect_owner_repo()` の 3 箇所。同根因への防御として `gh-repo-env-guard` preset は land 済み (PR #238 系)。

##### 背景: 既に実インシデント化しており、`.claude/hooks-config.toml` の gh-repo-env-guard preset コメントが PR #238 / ADR-045 を明記している。既存 guard は誤った回避策 (`GH_REPO=` の場当たり利用) をブロックするのみで、根本原因 (呼び出し箇所の `--repo` 欠落) は未修正。J01 と同じ ADR-039 dogfood 文脈

##### 設計決定: `GH_REPO` 環境変数 or jj remote 由来で owner/repo を明示的に解決し、全 gh 呼び出しに `--repo` を付与する

- [ ] github.rs / util.rs の gh 呼び出しに owner/repo 解決 + `--repo` 付与
- [ ] 非 colocated workspace を模擬した PR 検出の回帰テスト
- [ ] 本エントリ削除

##### 完了基準: 非 colocated jj workspace でも merge/monitor パイプラインが PR を正しく検出できること (回帰テストで seal)

---

### 週次レビュー採用 (2026-07-01)

#### Stop hook `[stop_quality]` と push-runner `[quality_gate]` の lint/test 重複を解消 (週次レビュー WR-2026-07-01-A01 採用)

> **動機**: `.claude/hooks-config.toml` `[stop_quality]` と `push-runner-config.toml` `[quality_gate]` が同一チェック (pnpm lint / cargo clippy --workspace -- -D warnings / pnpm test / pnpm test:e2e / pnpm build) を重複実行している。`push-runner-config.toml` は「Rust lint + test group: push pipeline でのみ実行。PostToolUse / Stop hook では実行せず」と明記しているにもかかわらず `[stop_quality]` が cargo clippy 等を実行しており、コメントで宣言した責務境界と実態が乖離している。ADR-015 が push-time 品質ゲートを push-runner-config に移行した際の Stop hook cleanup 漏れ (systemic harness-duplication)。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-07-01-A01 で採用 (severity=high, facet=architecture, category=harness-duplication)
>
> **⚠ 計画書 PR-W5 との整合 (競合注意)**: 旧 file-length-enforcement-plan の PR-W5 は `[stop_quality.steps]` に **file-length step を追加**する予定だった。analyzer 推奨の Option A (`[stop_quality]` 全削除) をそのまま採ると file-length step の受け皿が消えて競合する。**Option A' (整合版)**: 重複する lint/clippy/test step のみ削除し、session 固有チェック (file-length gate 等) の受け皿として `[stop_quality]` セクション自体は残す。
>
> **Status update (2026-08-12)**: **ブロッカー解消 — 着手可能**。PR-W5 は #234 で land 済みで `[stop_quality.steps]` に file-length step が実在し、file_length gate は本採用確定 (計画書は削除済み、分割制約は dev-conventions.md へ移設)。「PR-W5 確定待ち」の前提は消えた。
>
> **参照**: `.claude/weekly-reviews/2026-07-01.md` WR-2026-07-01-A01、`.claude/hooks-config.toml` `[stop_quality]` (修正対象)、`push-runner-config.toml` `[quality_gate]` (lint/test single authority 候補)、ADR-004 (Stop hook 品質ゲート)、ADR-015 (push-runner 移行)、ADR-022 (責務分離)

##### 背景: ADR-015 で push-time quality gate を push-runner-config.toml に集約した際、ADR-004 由来の Stop hook `[stop_quality]` の lint/test step が削除されず残存。push-runner-config.toml 自身のコメントが「Stop hook では実行しない」と意図を明記しているため意図と実装の乖離が明白。ただし `[stop_quality]` は PR-W5 の file-length gate 受け皿としての将来用途があるため、セクション全削除ではなく重複 step の選択的除去が必要

##### 設計決定: Option A' (推奨、PR-W5 整合版) — `[stop_quality]` から push-runner `[quality_gate]` と重複する lint/clippy/test step のみを削除し、session 固有チェック (PR-W5 の file-length step 等) の受け皿としてセクションは維持。quality_gate を lint/test の single authority とする。ADR-004 と ADR-015 に責務境界 (Stop hook = session 固有 / push gate = lint/test authority) を明記。Option B (意図的 defense-in-depth として両 ADR にコスト試算コメント追記) は代替案

- [ ] `[stop_quality.steps]` の file-length step (land 済み) を残す前提で重複 step の削除範囲を確定する
- [ ] `[stop_quality]` の重複 lint/test step を特定し選択的削除 (file-length step は残す)
- [ ] ADR-004 + ADR-015 に責務境界を明記 (Stop hook = session 固有チェック限定)
- [ ] Stop hook / push gate の dogfood で lint/test が push 側のみで走ることを確認
- [ ] 本エントリ削除 + todo-summary.md 行追加削除

##### 完了基準: lint/clippy/test が push-runner `[quality_gate]` のみで実行され `[stop_quality]` からは重複除去、PR-W5 の file-length gate と非競合 (`[stop_quality]` セクションは session 固有チェック用に存続)、ADR-004/015 に責務境界が明文化

### 週次レビュー採用 (2026-06-01)

#### `cli-merge-pipeline/feedback.rs` で `owner_repo` 検証を追加 (Phase E dogfood WR-2026-06-01-C02 採用)

> **動機**: `src/cli-merge-pipeline/src/feedback.rs:156-207` は `owner_repo` を検証せずに `gh CLI --repo` 引数に渡しているが、対応する hook (`hooks-stop-feedback-dispatch`) および `lib-pending-file` は `is_valid_owner_repo()` で検証済み。hook path を迂回した破損 pending file が gh 呼び出しに到達する余地があり defense-in-depth が欠如している (`feedback_review_severity_auto_fix` は本指摘が weekly-review 経由のため適用外、user 採用承認済)。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-06-01-C02 で採用 (severity=medium, facet=security, category=injection)
>
> **参照**: `.claude/weekly-reviews/2026-06-01.md` WR-2026-06-01-C02、`src/cli-merge-pipeline/src/feedback.rs:156-207` (修正対象 — 当時)、`src/lib-pending-file/src/lib.rs` `is_valid_owner_repo()` (既存 validator)、ADR-022 § defense-in-depth 原則
>
> **Status update (2026-08-12)**: 参照先の `feedback.rs` は PR #230 (PR-W3) で `src/cli-merge-pipeline/src/feedback/` 配下に module 分割済み (dead pointer 解消)。`fetch_pr_time_range` 相当の修正対象は分割後の `feedback/pr_metadata.rs` 周辺。分割後 module に `is_valid_owner_repo` 呼び出しが無いことを確認済み = タスク自体は依然未実装で有効。

##### 背景: cli-merge-pipeline の feedback path は merge 完了後に `gh pr view` / `gh api` で PR メタデータを取得する経路で、入力 `owner_repo` は pending file 由来。hook 経由の通常 path では `is_valid_owner_repo()` が呼ばれるが、broken pending file (Claude 編集ミス / 手動修正等) が cli-merge-pipeline に直接到達した場合は無検証で gh CLI に渡る (cli-merge-pipeline は hook と独立して起動可能)

##### 設計決定: `fetch_pr_time_range()` 先頭で `is_valid_owner_repo(owner_repo)` を呼び出し、無効時は `Err` 返却。もしくは関数 signature を `&PendingFile` 受取に変更し型不変条件で保証 (より構造的)

- [ ] Option A 採用判断 (1 行 guard) or Option B 採用判断 (型 signature 変更)
- [ ] `is_valid_owner_repo()` の re-export / dependency 確認 (lib-pending-file → cli-merge-pipeline)
- [ ] test 追加: 無効 owner_repo (`../../../etc`, `owner;rm -rf`, `owner` (no slash) 等) で `Err` 返却を assert
- [ ] cargo test + cargo clippy pass
- [ ] 本エントリ削除 + todo-summary.md 行追加削除

##### 完了基準: 無効 `owner_repo` が gh CLI に到達しない (defense-in-depth)、test で各 invalid pattern (path-traversal / shell-injection / format-violation) を assert、lib-pending-file の既存 validator と挙動一致

#### CLAUDE.md ADR index に ADR-032 (reserved) スタブ追加 + ADR-033 参照を実在 ADR に差し替え (Phase E dogfood WR-2026-06-01-A01 採用)

> **動機**: CLAUDE.md の ADR インデックスが ADR-031 → ADR-033 へ飛び **ADR-032 が欠落**、`docs/adr/adr-033-todo-numbering-simplification.md:40-42, 81, 95, 130` は `ADR-032 PR-β` を 4 箇所参照しているが対応ファイル不在。Cross-File Reference Lifecycle ルール (permanent → 不在の永続成果物を参照不可) に違反した dead-pointer。(採用当時は旧 todo2.md の起案予定エントリで reserved 状態として trackable だった — 2026-08-12 の前提変更で欠番扱いに変わった。下記注記参照。)
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-06-01-A01 で採用 (severity=medium, facet=architecture, category=docs-internal)
>
> **⚠ 再検出 (2026-07-01)**: 本タスクは 2026-06-01 採用後 **1 か月未着手**のため、2026-07-01 週次レビューで同一問題が WR-2026-07-01-A02 (severity=**high** に昇格) として再検出された。ADR-031 重複検出方針により重複エントリは作らず本タスクに集約。優先度の引き上げを推奨。
>
> **⚠ 前提変更 (2026-08-12)**: ADR-032 が予約していたテーマ「docs-only fast-path」は **[ADR-057](adr/adr-057-docs-only-deterministic-routing.md) として別番号・別設計で実現し 2026-08-12 採用確定**した。したがって「reserved スタブを追加する」当初案 (Option A) は陳腐化 — **ADR-032 は永久欠番**とし、タスクの中身は「欠番の明示 + ADR-033 の dead pointer 4 箇所の解消」に変わる。起案予定の出所だった旧 todo2.md も同日退役 (削除) 済み。
>
> **参照**: `.claude/weekly-reviews/2026-06-01.md` WR-2026-06-01-A01、`.claude/weekly-reviews/2026-07-01.md` WR-2026-07-01-A02 (再検出)、`CLAUDE.md:5-45` (ADR index、修正対象)、`docs/adr/adr-033-todo-numbering-simplification.md:40-42, 81, 95, 130` (`ADR-032 PR-β` 参照 4 箇所、修正対象)、[ADR-057](adr/adr-057-docs-only-deterministic-routing.md) (実現先)

##### 背景: ADR-032 は「docs-only fast-path」関連の試験運用 ADR として起案予定だったが未作成のまま、テーマ自体が ADR-057 で実現した (起案予定エントリを収容していた旧 todo2.md は 2026-08-12 退役)。一方 ADR-033 は task naming 例示として `ADR-032 PR-β` を使用済みで、CLAUDE.md は ADR-031 → ADR-033 へジャンプする状態。reader が CLAUDE.md から ADR-032 を辿ろうとすると broken-link、ADR-033 から ADR-032 を辿ろうとしても dead-pointer

##### 設計決定 (2026-08-12 改訂): ADR-032 は永久欠番として扱う — CLAUDE.md ADR index に `- ADR-032: (欠番 — docs-only fast-path として予約されたが ADR-057 が別設計で実現)` の 1 行を追加し、ADR-033 内の `ADR-032 PR-β` 4 箇所を `(欠番 ADR-032 のタスク名例、実現は ADR-057)` 等の欠番明示 wording に変更する

- [ ] CLAUDE.md ADR index に欠番明示行を追加 (位置: ADR-031 と ADR-033 の間)
- [ ] `docs/adr/adr-033-todo-numbering-simplification.md:40-42, 81, 95, 130` の `ADR-032 PR-β` 参照 4 箇所を欠番明示 wording に差し替え
- [ ] `grep -rn 'ADR-032' docs/ CLAUDE.md` で他の dead-pointer 残存確認
- [ ] markdownlint / `pnpm exec cli-docs-lint` 等で broken-link 解消確認
- [ ] 本エントリ削除 + todo-summary.md 行追加削除

##### 完了基準: CLAUDE.md ADR index が ADR-031 → ADR-032 (欠番明示) → ADR-033 で連続化、ADR-033 の `ADR-032 PR-β` 参照 4 箇所が dead-pointer ではなくなる、`grep -rn 'ADR-032'` で残存 dead-pointer 0 件

### (追って) ADR-030 の takt-test-vc 反映

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
