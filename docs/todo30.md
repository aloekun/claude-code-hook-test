# TODO (Part 30)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: **新規タスクの追加先** (2026-11-23 から)。[docs/todo29.md](todo29.md) が 50,897 B (閾値 51,200 B まで残り約 300 B) に達し、週次レビュー (2026-11-23) の採用分を収める余地が無かったため新設した (WR-2026-11-23-T04)。50KB に到達するまでは本ファイルへ追加する。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 週次レビュー採用 (2026-11-23)

### 順位 621: pr-monitor の bookmark 前進にスタック安全策を入れ、push-runner と同じ関数を通す

> **動機**: jj bookmark 前進のロジックが `cli-push-runner` と `cli-pr-monitor` に複製されている。順位 597 のスタック安全策 (target の祖先にある bookmark が 1 つだけで、子孫に別の bookmark が無い場合だけ前進する) は push-runner 側 (`push_jj_bookmark/fallback.rs`) にしか無い。pr-monitor の `advance_bookmarks_via_list` には子孫の確認もスタックの検査も無いので、pr1 ← pr2 のスタックで pr2 の bookmark が置き去りになると、pr1 を pr2 の tip まで動かす恐れがある。順位 376 / #620 と同じ形の事故になる。pr-monitor 側のテストにはスタック系のケースが無い。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-11-23-S01 で採用 (severity=high, facet=simplicity, category=harness-duplication)。**順位 607 (todo29.md) の拡張**として扱う。607 は主経路を push-runner に揃える作業で、本タスクは fallback の安全策と共通化を足す。todo29.md が閾値の間際なので本ファイルに書いた
>
> **参照**: `.claude/weekly-reviews/2026-11-23.md`、`src/cli-pr-monitor/src/stages/push_jj_bookmark.rs` (`advance_bookmarks_via_list` L76、`get_bookmarks_in_range` L164)、`src/cli-push-runner/src/stages/push_jj_bookmark/fallback.rs`

#### 背景

2026-11-23 に確認した。push-runner の `fallback.rs` は冒頭の doc と回帰テスト (`push_jj_bookmark.rs:463`) で順位 597 を参照している。pr-monitor の `push_jj_bookmark.rs` には 597 への言及も子孫の検査も無い。

#### 設計決定

- 順位 607 と同じ関数を変えるので、607 と同時に着手するか、607 の後に着手する
- 先に 597 の安全策を pr-monitor 側へ入れ、そのあと `advance_bookmarks_via_list` / `dispatch_bookmark_advance` / `get_bookmarks_in_range` を `lib-jj-helpers` へ移す。subprocess の呼び出しは注入にし、crate ごとのログの prefix はそこで保つ

- [ ] pr-monitor 側に、スタック (pr1 ← pr2) で pr1 を動かさないことを示す実 jj の回帰テストを足す (先に落ちることを確かめる)
- [ ] 597 の安全策を pr-monitor 側へ入れる
- [ ] 着手時判断: `lib-jj-helpers` へ移す範囲 (3 関数すべてか、fallback だけか) を 607 の着地後の形を見て決める

#### 完了基準

- スタック構成の回帰テストが両 crate で通る
- bookmark 前進の判定が 1 か所にあり、両経路がそれを通ることがテストで固定されている

### 順位 622: post-pr の scope guard に、ハーネス設定と CI 定義の deny-list を決定論で入れる

> **動機**: scope guard の allowlist は CodeRabbit コメント本文の `file` 列から作られる (`allowlist_from_findings`)。この本文は PR 作成者が文言を注入できる非信頼テキストである。`.claude/settings.json` や `.github/workflows/*` を指す finding があると、fix がそこを編集しても許可されたものとして scope guard が PASS し、auto-push でマージ対象に入る。`.claude/settings.json` の hook は開発者のマシンで実行されるので、影響は PR 作成者からリポジトリの利用者全体に広がる。ハーネス設定と CI 定義の deny-list は、fix.md にも決定論層にも無い。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-11-23-C01 で採用 (severity=high, facet=security, category=prompt-injection)。CodeRabbit が攻撃者の文言を Location に反映し、fix がそれに従うことが前提なので、確定した脆弱性ではなく高リスクとして扱う
>
> **参照**: `.claude/weekly-reviews/2026-11-23.md`、`src/cli-pr-monitor/src/stages/scope_guard.rs:84-90`、`lib_scope_guard::allowlist_from_paths`、[ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md)、[ADR-043](adr/adr-043-security-gates-fail-closed.md)

#### 背景

パスの正規化は `lib_scope_guard` に委譲されていて、CI 経路 (`cli-fix-push-gate`、ADR-067) と判定を共有している。deny-list も `lib_scope_guard` に置けば両経路に同時に効く。pre-push 経路への展開は順位 364 が追跡している (WR-2026-11-23-C04 は 364 と同じ内容なので却下した)。

#### 設計決定

- (a) deny-list を `lib_scope_guard` に決定論で足す。候補は `.claude/**` / `.github/workflows/**` / `.takt/**` / `pr-monitor-config.toml` / `push-runner-config.toml` / `templates/**`。allowlist に入っていても deny-list を優先する (fail-closed)
- (b) allowlist を「finding の `file` 列」と「PR 自身の diff の対象ファイル」の積集合にする
- (c) fix.md の read-only zone に `.claude/settings.json` と `pr-monitor-config.toml` を加える (助言層。決定論層の代わりにはしない)

- [ ] 着手時判断: (b) の積集合で、PR の diff に無いファイルへの正当な修正 (テストの追加など) が止まらないかを、過去の fix の実績で確かめる
- [ ] (a) を実装し、deny-list のパスを指す finding が observe / enforce の両 mode で期待どおりに振り分けられるテストを足す
- [ ] (c) を fix.md に反映する

#### 完了基準

- `.claude/settings.json` を指す finding に従った fix が、enforce mode で block される
- post-pr 経路と CI 経路 (`cli-fix-push-gate`) が同じ deny-list を通る

### 順位 623: ADR-054 層 1 の `injection_suspect` を fix の入力に配線するか、ADR を実装に合わせる

> **動機**: ADR-054 の層 1 は、`injection_suspect` と分類された finding を下流で自動修正の対象から外すと定めている。しかし実装では `state.classified_findings` に保存されるだけで、読む箇所が無い。auto-push に渡るのは生の finding なので、層 1 は記録するだけで防御として働いていない。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-11-23-C02 で採用 (severity=medium, facet=security, category=adr-alignment)。上の deny-list と同じ攻撃経路の防御なので、まとめて検討する
>
> **参照**: `.claude/weekly-reviews/2026-11-23.md`、`src/cli-finding-classifier/src/lib.rs:107-127,191`、`src/cli-pr-monitor/src/stages/poll/iteration.rs`、`src/cli-pr-monitor/src/state.rs`

#### 背景

2026-11-23 に確認した。`injection_suspect` / `InjectionSuspect` は `cli-finding-classifier` の外では参照されていない。`classified_findings` を参照するのは pr-monitor の `poll/iteration.rs` と `state.rs` だけである。層 1 はローカル LLM による分類 (ADR-038) で、失敗しても止まらない fail-open の助言層である。

#### 設計決定

- 配線するなら、除外と停止のどちらにするかを決める。除外は残りの finding で fix を続け、停止は `injection_suspect` を含む回の auto-push を止める
- 配線しないなら、ADR-054 の層 1 の記述を「記録のみ」に直し、防御として数えない

- [ ] 着手時判断: 除外 / 停止 / 記録のみ のどれにするかを、ADR-038 の分類の誤検知率 (telemetry か過去の state) を見て決める
- [ ] 決めた方を実装するか ADR を直す。配線する場合は `injection_suspect` を含む state から fix 入力を作るテストを足す

#### 完了基準

- ADR-054 の層 1 の記述と実装の挙動が一致している

### 順位 624: ファイル長と関数長の閾値を、hook の定数を持ち主にして揃える

> **動機**: ファイル長と関数長の閾値が層によって違う。hook (`hooks-post-tool-comment-lint-rust` の `file_length.rs`) は 800 行まで通し、メッセージには「200-400 行が目安」と書く。takt builtin の knowledge `architecture.md` は「1 ファイル 300 行超で REJECT」「30 行を超える関数は分割を検討」と書く。同じ push で 2 つの判定が食い違う。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-11-23-A02 で採用 (severity=medium, facet=architecture, category=harness-duplication)
>
> **参照**: `.claude/weekly-reviews/2026-11-23.md`、`src/hooks-post-tool-comment-lint-rust/src/file_length.rs:13,71`、`node_modules/takt/builtins/en/facets/knowledge/architecture.md:10,44`、`scripts/weekly-scan.mjs:19` (`RS_LINE_THRESHOLD = 800`)、`.takt/workflows/weekly-review.yaml:111`、[ADR-056](adr/adr-056-review-policy-anomaly-shadow.md)、[ADR-081](adr/adr-081-single-fact-dispersion.md)

#### 背景

2026-11-23 に確認した。300 行 / 30 行はリポジトリの facet ではなく、takt builtin の knowledge (`node_modules` 配下) にある。`knowledge: architecture` は `post-pr-review.yaml` / `post-merge-feedback.yaml` / `weekly-review.yaml` が参照している。2026-08-15 の週次レビューで取り下げた「800 行閾値の SSOT 化」(todo28.md) は Rust 側の 800 の重複を扱ったもので、builtin knowledge との食い違いは対象外だった。

#### 設計決定

- 持ち主は hook の `MAX_FILE_LINES` (と関数長の定数) にする
- builtin knowledge の数値は直接編集できないので、ADR-056 の policy と同じく、リポジトリ側で knowledge を shadow して数値を除く (または hook の定数を参照と書く)
- `weekly-scan.mjs` は JavaScript なので Rust の定数を直接読めない。800 は値として残し、hook の定数への相互参照のコメントを付ける (ADR-081 の手順。数値の重複は、相互参照のある箇所に限って許す)

- [ ] 着手時判断: knowledge を丸ごと shadow するか、数値の行だけを上書きする手段が takt にあるかを確かめて決める
- [ ] shadow を入れ、3 つの workflow がそれを読むことを確かめる
- [ ] `file_length.rs` のメッセージの「200-400 行が目安」を残すかどうかを決める

#### 完了基準

- takt のレビューが 300 行超のファイルを、ファイル長だけを理由に REJECT しない
- 閾値の正が hook の定数にあり、ほかの箇所は数値を持たないか、持つ場合は hook の定数への相互参照が付いている (facet knowledge の shadow は数値を持たない)

### 順位 625: 台帳の棚卸し手順の検査範囲を `docs/todo-summary*.md` に広げる

> **動機**: 台帳 `docs/claude-code-web-tasks.md` の棚卸し手順 (§ 自律実行可否の手順 1・2) と着手フローは、検査範囲を `docs/todo-summary.md` / `todo-summary2.md` と書き、`todo-summary3.md` を含まない。有効な台帳行 17 件のうち 8 件 (428, 542, 544, 546, 551, 557, 564, 569) は summary3 にしか載っていないので、手順どおりに適用すると 8 件すべてが land 済みと誤判定される。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-11-23-T01 で採用 (severity=medium, facet=todo, category=todo-preamble-drift)
>
> **参照**: `.claude/weekly-reviews/2026-11-23.md`、`docs/claude-code-web-tasks.md:80,140,482,483`

#### 背景

2026-11-23 に確認した。facet (`review-todo-whole.md`) は既に `docs/todo-summary*.md` を glob しているので、facet の指示文は直す必要が無い。直すのは台帳の文面だけである。`docs/todo.md` の preamble と cli-docs-lint は全 part を glob で統合検査している。

#### 設計決定

- 台帳の該当箇所 (80 / 140 / 482 / 483 行目付近) を `docs/todo-summary*.md` (全 part) に書き換える。part 名を列挙しない (part が増えるたびに同じずれが起きるため)
- §棚卸し履歴の過去の記録 (277 / 286 / 350 行目付近) は当時の事実なので直さない

- [ ] 台帳の手順と着手フローの文面を直す
- [ ] `pnpm lint:docs` が通ることを確かめる

#### 完了基準

- 台帳の現行の手順に、part 名を決め打ちした検査範囲が残っていない

### 順位 626: merge-pipeline と pr-monitor の PR 検出から、git HEAD に頼る `gh pr view` を外す

> **動機**: 引数なしの `gh pr view --json number` は、共有 store の git HEAD から PR を決める。並列 jj workspace (ADR-045) では git HEAD が全 workspace で 1 つなので、別の workspace で feature branch を checkout していると、その branch の PR が返る。返り値が Ok なので bookmark の検索に進まず、merge-pipeline は別 workspace の PR を黙って対象にする。`merge-pr` が誤った PR をマージしうる。git HEAD が detached なら失敗して正しくフォールバックするので、HEAD が PR を持つ名前付き branch を指すときだけ起きる。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-11-23-J01 で採用 (severity=medium, facet=jj-robustness, category=jj-gh-no-repo)。WR-2026-09-03-J01 の残り
>
> **参照**: `.claude/weekly-reviews/2026-11-23.md`、`src/cli-merge-pipeline/src/github.rs:147-149` (`detect_pr_number` の第 1 段)、`src/cli-pr-monitor/src/util.rs:40-42` (`get_pr_info` の Strategy A)

#### 背景

2026-11-23 に確認した。両 crate とも、`gh pr view` が数値を返せばそれを採用し、失敗したときだけ jj bookmark の検索 (`get_jj_bookmarks_with_remote_fallback` / `find_pr_via_jj_bookmarks`) に進む。bookmark の検索はリモート専用の bookmark にもフォールバックする (順位 397)。

#### 設計決定

- jj 環境では bookmark の検索 (`gh pr list --head <bookmark>`) を先頭にする
- `gh pr view` を残す場合は、返った PR の `headRefName` が検索対象の bookmark と一致するときだけ採用する

- [ ] `gh pr view` が別 branch の PR を返す状況を注入したテストを両 crate に足す (先に落ちることを確かめる)
- [ ] 着手時判断: `gh pr view` を外すか、`headRefName` の照合を付けて残すかを、bookmark の無い状況 (git だけの派生プロジェクト) での挙動を見て決める
- [ ] 2 crate の検出順を同じにする

#### 完了基準

- git HEAD が別の PR の branch を指していても、merge-pipeline と pr-monitor が現在の jj bookmark の PR を選ぶ

### 順位 627: 順位 6 (Branch Protection の整備) の要否と完了状況を確かめる

> **動機**: 順位 6 (GitHub Branch Protection の整備、todo22.md) が 3 か月以上動いていない。最後の Status update は 2026-08-12 で、作業計画の 4 項目はどれも未チェックである。完了基準は `gh api` での確認を求めているが、実際の branch protection の設定は確かめていないので、既に完了している可能性もある。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-11-23-T02 で採用 (severity=low, facet=todo, category=todo-dead-entry)
>
> **参照**: `.claude/weekly-reviews/2026-11-23.md`、`docs/todo22.md` (### 順位 6)

#### 背景

順位 6 は旧 docs-only 高速パス計画の Phase pre から独立させたタスクで、ブロックを Required status checks へ集めることを目的にしている。docs-only の高速パスは ADR-057 が別の設計で実現した。

#### 設計決定

- 実際の設定を `gh api repos/{owner}/{repo}/branches/master/protection` で確かめてから、残すか、直すか、削除するかを決める

- [ ] branch protection の現状を確かめ、順位 6 の完了基準と突き合わせる
- [ ] 着手時判断: 満たしていれば順位 6 を削除する。満たしていなければ、ADR-057 の後でも必要かを判断し、作業計画を今の CI 構成 (ADR-065 の matrix) に合わせて書き直す

#### 完了基準

- 順位 6 が削除されているか、今の CI 構成に合った作業計画に書き直されている
