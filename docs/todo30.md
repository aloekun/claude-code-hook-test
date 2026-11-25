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

### 順位 628: scope guard の allowlist を、finding のファイルと PR の diff の対象ファイルの積集合にする

> **動機**: scope guard の allowlist は CodeRabbit コメント本文の `file` 列から作られ、その本文は PR 作成者が文言を注入できる。順位 622 でハーネス設定と CI 定義は拒否リストで守ったが、それ以外の場所 (たとえば PR と無関係な `src/` のファイル) は、finding が指せば fix が書き換えても通る。PR の diff に無いファイルを allowlist から外せば、注入で広げられる範囲を PR 自身の変更に閉じ込められる。
>
> **本タスクの位置づけ**: 順位 622 の設計決定 (b) を切り出した (2026-11-24、622 の着手時のヒアリング)。拒否リスト (a) で主な攻撃経路は塞いだので、(b) は実績の確認を済ませてから入れる
>
> **参照**: `src/lib-scope-guard/src/lib.rs` (`find_out_of_scope`)、`src/cli-pr-monitor/src/stages/scope_guard.rs` (`allowlist_from_findings`)、`src/cli-fix-push-gate/src/inputs.rs`、[ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md) § 追記 (2026-11-24)

#### 背景

ADR-054 の「欠点 / 留意点」は、正当な修正が別ファイルの連動修正を要する場合の過剰 block を挙げ、緩和として PR diff のファイルを allowlist に**足す**案を書いている。本タスクは逆向きに**絞る**案なので、過剰 block が増える側に働く。

#### 設計決定

- PR の diff の対象ファイルは post-pr 経路なら `jj diff -r <base>..<pre_takt_cid> --summary`、CI 経路なら git diff から取る。どちらも `lib_scope_guard` に積集合の関数を置いて共有する

- [ ] 着手時判断: `.takt/runs/` の過去の post-pr fix を調べ、PR の diff に無いファイル (テストの追加など) を直した例があるかを確かめる。あれば、テストファイルなどを例外にするか、(b) を見送るかを決める
- [ ] 積集合を両経路に入れ、PR の diff に無いファイルを指す finding に従った fix が scope 外になるテストを足す

#### 完了基準

- PR の diff に無いファイルへの fix が、finding が指していても enforce mode で block される
- post-pr 経路と CI 経路が同じ積集合の関数を通る

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

---

## post-merge feedback (#664〜#667) の一括採否から起票 (2026-11-25)

> 対象は #664〜#667 の feedback report (候補 15 件) である。Tier 1 / 2 の採用候補を作業内容で 3 件にまとめ、ユーザーが 3 件とも採用した。#667 Tier 1 #1 と Tier 2 #1 は同じ作業なので順位 629 にまとめた。#665 Tier 2 #1 は順位 630 にした。#664 Tier 2 #1 (ask ルールの変異確認) は一度きりの手順なので起票せず、2026-11-25 に実施した: `pnpm -s mutate` は `.claude/` を書き換えないので、テストと同じ照合を scratchpad のスクリプトで再現し、14 本の ask ルールすべてに、そのルールにしか当たらない起動の形が 1〜5 件あることを確かめた (どのルールを外してもテストが落ちる)。#665 Tier 2 #2 (会話記録を `sessionId` で絞る) は順位 617 と同じ作業なので、617 に観測を足した。
>
> 却下したのは次のとおり。#666 Tier 2 #1 (fix.md / fix-supervisor.md の read-only 一覧と、`lib_scope_guard` の拒否リストの食い違いを検査する) は、前提が合わない。read-only 一覧は pre-push と post-pr の両方の fix に効く規則で、`docs/adr/**` を含み `.github/` を含まない。post-pr 経路だけの拒否リストとは別の方針で、同じ事実の分散 (ADR-081) ではない。拒否リストの中身は、facet からコードの定義を指す形になっている。#666 Tier 2 #2 (`run_takt_stage` の配線テスト) は、判定が純関数としてテスト済みで、配線のテストには state のパスの注入が要り、後段に決定論の scope guard が控えるため。#664 Tier 2 #2 (`pnpm merge-pr 664` の位置引数)、#665 Tier 1 #1 (git preset が引用符の中の語に反応する。順位 620 で「解決しない誤検知」として受け入れた形)、#665 Tier 1 #2 (変更行だけの整形検査)、#666 Tier 1 #1 (merge-pr 中の feedback の途中終了。原因が未確認) は、観測が 1 回だけのため。Tier 3 は、Tier 3 でしか拾えないものが無いので全件を却下した (#667 の一過性のレビュー ID は crate の慣習に合わせて残す、#666 の analyze-session の読み取りの目安は順位 617 で記録が減れば軽くなる、表形式のテストは過剰な一般化、テストの文字列への lint の除外は対象のルールを特定できていない)。起票の前に、各項目の前提をコードと実行で確かめた ([ADR-075](adr/adr-075-verify-premises-before-acting.md))。

### 順位 629: jj の preset から本文を外す方式の判断を ADR に記録し、jj の大域オプションの位置の取りこぼしを塞ぐ

> **動機**: 順位 620 (#667) で、jj 系の preset の判定の前に `-m` の値と heredoc の本文を外すようにした (`src/hooks-pre-tool-validate/src/inert_text.rs`)。外してよい受け手は許可リスト (`HEREDOC_READERS` / `MESSAGE_TAKERS`) で決める。この方式は、着手前のヒアリングで決めた「処理系を拒否リストで並べる」形から、pre-push の fix step が拒否リストの並べ漏れ (`awk -f /dev/stdin`) を塞ぐために切り替えたものである。人間がレビューして採用したが、判断の記録はモジュールの doc にしかない。ADR-068 は設計級の変更を fix の裁量で決めないとしているので、判断を ADR に残す (#667 の feedback、Tier 1 #1)。
>
> あわせて、`command_is_one_of` は大域オプションをサブコマンドの**前**でしか見ていない。jj は `--config` をサブコマンドの後ろでも受け付ける (2026-11-25 に `jj log ... --config ui.color=never` が通り、存在しないオプションは拒否されることを確かめた)。`jj describe --config ... -m '...'` は許可リスト内として値を外す。`-m` か `--stdin` があればエディタは起動しないので実害は小さいが、前に書いた形だけを止めるのは非対称である (#667 の feedback、Tier 2 #1)。
>
> **由来**: `[improvement]`。#667 Tier 1 #1 と Tier 2 #1 をまとめた。
>
> **種類**: Tier 1 — ADR の新設と判定の修正。工数 S〜M。
>
> **参照**: [ADR-068](adr/adr-068-fix-step-authority-boundary.md)、[ADR-043](adr/adr-043-security-gates-fail-closed.md)、[ADR-007](adr/adr-007-custom-linter-layer-boundary.md)

#### 作業計画

- [ ] 新規 ADR を書く。外す範囲 ((a)+(b)) と対象の preset (jj 系)、受け手を許可リストで決める理由 (拒否リストは並べ漏れで本文を外す)、外した後に字句解析器で同じコマンドかを確かめる二重の防御、受け入れた見逃しと解決しない誤検知を残す。モジュールの doc の判断の部分は ADR を指す形にする
- [ ] `command_is_one_of` で、jj の `--config` / `--config-toml` / `--config-file` がサブコマンドの後ろにあっても許可リスト外にする。jj の他の大域オプション (`-R` など) を後ろに書いた形の扱いも、着手時に決めて ADR に書く
- [ ] 回帰テストを足し、判定を外す変異が落ちることを `pnpm -s mutate` で確かめる

#### 完了基準

- 本文を外す方式の判断が ADR にあり、モジュールの doc がそれを指している
- `jj describe --config ... -m '...'` と `jj describe --stdin --config ... <<'EOF'` が、外されずに元の文字列で判定される

### 順位 630: PR 検出の「push 先は本家」という前提を、仕様としてコードとテストに書く

> **動機**: #665 で、`detect_pr_number` (`src/cli-merge-pipeline/src/github.rs`) と `find_pr_number_with` (`src/cli-pr-monitor/src/util.rs`) は、`gh pr list --head <bookmark>` の結果から fork の PR (`isCrossRepository == true`) を除き、同じリポジトリの最初の PR を採るようにした。CodeRabbit は、`isCrossRepository == false` が「bookmark の push 先と同じリポジトリ」を保証しないと指摘し、対応済みとしなかった。push 先が fork で、本家にも同名ブランチの PR があると、本家の PR を選びうる。この repo では `pnpm push` が常に origin (本家) へ push し、PR も同じリポジトリのブランチから作るので前提は成り立つが、コードにもテストにも書かれていない (#665 の feedback、Tier 2 #1)。
>
> **由来**: `[improvement]`。
>
> **種類**: Tier 2 — 前提の明記とテストの追加。工数 S。
>
> **参照**: PR #665、[ADR-045](adr/adr-045-jj-workspace-parallel-sessions.md)

#### 作業計画

- [ ] 両関数の doc に「bookmark は本家 (origin) に push されている前提で、fork への push は対象外」と書く。前提を確かめる処理を足すか (bookmark の追跡先の remote が origin か) は、着手時に決める
- [ ] 前提を固定するテストを足す。足さない側にした場合は、その理由を doc に書く

#### 完了基準

- PR 検出の前提が両関数の doc にあり、確かめる処理を足した場合はその挙動がテストで固定されている
