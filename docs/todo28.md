# TODO (Part 28)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: `docs/todo.md` が 50406 B (2026-09-19 時点、50KB = 51200 B の安定読み取り閾値まで残り 794 B) に達したため、**2026-08-15 より前の「週次レビュー採用」節 4 節をここへ移した**。**新規エントリの追加先ではない** — 新規は `docs/todo26.md` に記録する。本ファイルは移送したエントリの編集・完了削除専用。
>
> **移送の基準**: `docs/todo.md` は全 todo ファイルの routing preamble (約 9.5KB) を持つため、本体を軽くするには**節単位でまとまって動かせるもの**を選ぶしかない。古い順に 4 節を選び、最新の「週次レビュー採用 (2026-08-15)」節は `docs/todo.md` に残した。移送対象はいずれも順位未採番の `####` エントリで順位 table (`docs/todo-summary*.md`) に行を持たないため、entry-pairing 検査の 1:1 対応には影響しない。
>
> **移送元の内訳**: `docs/todo.md` の「週次レビュー採用」節のうち 2026-08-13 / 2026-07-19 / 2026-07-01 / 2026-06-01 の 4 節、計 10 エントリ (うち 1 件は撤回の記録)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 移送したタスク

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

#### 撤回: WR-2026-08-13-T01 / T02 (台帳 finding 2 件) — 2026-08-16

> **採用済みだったが実行せずに撤回した 2 件**。採用の記録だけ消えると「なぜ実行されなかったのか」が追えなくなるため、撤回の事実をここに残す (エントリ本体は削除済み)。
>
> - **WR-2026-08-13-T02**「台帳の ✅無人可 5 行を condition 3 違反により — へ降格」— 前提が二重に消滅した。対象ブランチは 2026-08-15 に削除済みで順位 216/239 は完了済み、さらに根拠だった **condition 3 自体を 2026-08-16 に廃止**した ([ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 18)
> - **WR-2026-08-13-T01**「台帳 Batch 1 の closed-without-merge 行を棚卸し履歴へ移動し、in-flight を明示」— 台帳の明文規定「削除するのはマージした順位だけ」と矛盾する。実行すると**未完了タスク 3 件 (順位 203/228/240) が台帳から消える**
>
> **撤回の一般的な含意**: どちらも condition 3 と ADR-072 決定 3 が同じブランチを逆に解釈していたことから生まれた finding である。レビューが正しく規則を適用しても、規則同士が矛盾していれば誤った採用に至る — 採用の是非は finding 単体ではなく、根拠にした規則の整合性まで見ないと判定できない。

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
> **Status update (2026-08-12)**: **ブロッカー解消 — 着手可能**。PR-W5 は #234 で land 済みで `[stop_quality.steps]` に file-length step が実在し、file_length gate は本採用確定 (計画書は削除済み、分割制約は [ADR-080](adr/adr-080-rust-module-split-invariants.md) へ移設)。「PR-W5 確定待ち」の前提は消えた。
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
