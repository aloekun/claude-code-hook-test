# TODO

> **運用ルール**: 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイル + [docs/todo2.md](todo2.md) + [docs/todo3.md](todo3.md) + [docs/todo4.md](todo4.md) + [docs/todo5.md](todo5.md) + [docs/todo6.md](todo6.md) + [docs/todo7.md](todo7.md) + [docs/todo-summary.md](todo-summary.md) の使い分け** (PR #83 T3-2 で恒久化、2026-04-28 強化、PR #88 で todo3.md 追加、PR #96 セッションで todo4.md 追加、PR #101 セッションで todo5.md 追加、PR #123 セッションで todo6.md 追加、2026-05-09 に todo-summary.md 切り出し + todo5.md 分割で todo7.md 追加):
> - **docs/todo-summary.md**: 推奨実行順序サマリー table 専用 (旧 todo.md から切り出し)。table の新規行追加・既存行編集・順位再採番はここで行う。
> - **docs/todo.md**: 既存タスクの編集・完了削除専用。新規タスクの**詳細エントリ**は追加しない (~50KB 閾値内に維持し Claude Code 読み取り安定性を確保)
> - **docs/todo2.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (50KB に到達したため、PR #88 以降の新規エントリは todo3.md へ)
> - **docs/todo3.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (50KB に到達したため、PR #96 セッション以降の新規エントリは todo4.md へ)
> - **docs/todo4.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (50KB に到達したため、PR #101 セッション以降の新規エントリは todo5.md へ)
> - **docs/todo5.md**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (2026-05-09 に古い半分を todo7.md へ分割。PR #115 以降のエントリのみ残存。新規エントリは todo6.md へ)
> - **docs/todo6.md**: 新規タスクの追加先。50KB に到達するまでは本ファイルへ追加
> - **docs/todo7.md**: 既存タスクの編集・完了削除専用 (旧 todo5.md の PR #101〜#109 エントリを 2026-05-09 に分割移動)。**新規タスクは追加しない**
> - 例外: 既存 todo.md / todo2.md 〜 todo7.md タスクと **同一ファイル / 同一コンポーネント** を編集する密結合タスクは該当ファイルに追加可 (例: `~/.claude/rules/common/git-workflow.md` 配下のグローバルルール群)
> - **新セッションでは八つすべてを確認すること** (todo.md / todo2-7.md / todo-summary.md)

---

> **推奨実行順序サマリー**: [`docs/todo-summary.md`](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### 週次レビュー採用 (2026-07-19)

#### docs/todo.md preamble の「八つ」を実際の 14 ファイルに更新 (週次レビュー WR-2026-07-19-T01 採用)

> **動機**: 主 preamble が「新セッションでは八つすべてを確認すること」と指示しているが、corpus は実際には 14 ファイル (todo.md / todo2-13.md / todo-summary.md) 存在する。todo8-13.md への案内が preamble の使い分けリストに無く、新規セッションの onboarding routing を毀損する。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-07-19-T01 で採用 (severity=high, facet=todo, category=todo-preamble-drift)
>
> **参照**: `.claude/weekly-reviews/2026-07-19.md` WR-2026-07-19-T01、`docs/todo.md:1-15` (preamble)

##### 背景: todo8-13.md は逐次追加されたが、preamble の「八つ」記述と使い分けリスト (todo2-7.md までしか列挙していない) が未更新のまま drift した。

##### 設計決定: preamble を「新セッションでは十四つすべてを確認すること (todo.md / todo2-13.md / todo-summary.md)」に更新し、todo8-13.md 追加の経緯 (拡張履歴) を使い分けリストに追記する。

- [ ] preamble のファイル数記述と使い分けリストを実 corpus (14 ファイル) に一致させる
- [ ] T02 (todo14.md 新設) と整合させる (新規追加先ファイルの記述を同時更新)
- [ ] 本エントリ削除

##### 完了基準: preamble のファイル数・使い分けリストが実 corpus と一致し、onboarding で全 todo ファイルが辿れること。

#### todo13.md が 50KB 超過 — todo14.md 新設で新規追加先を移す (週次レビュー WR-2026-07-19-T02 採用)

> **動機**: `todo13.md` が 50KB 閾値を大幅超過 (171KB, 約 3.4 倍) しているにもかかわらず preamble が「新規エントリの追加先は本ファイル」と宣言し続けている。todo8-12 が一貫して守ってきた 50KB 分割ポリシーからの逸脱で、Claude Code の読み取り安定性を損なう。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-07-19-T02 で採用 (severity=high, facet=todo, category=todo-preamble-drift)
>
> **参照**: `.claude/weekly-reviews/2026-07-19.md` WR-2026-07-19-T02、`docs/todo13.md` (175,298 bytes)、file-length-watchlist の機械 scan と整合
>
> **注**: 本週次レビュー land 時点の PR #302 (feedback 採用登録) も todo13.md 肥大化の一因。

##### 背景: 新規エントリを todo13.md に追加し続けた結果 50KB を大幅超過。file-length watchlist の機械 scan でも todo13.md (175KB) / todo10.md (97KB) / todo-summary.md (79KB) が閾値超過として検出されている。

##### 設計決定: (Option A 推奨) `docs/todo14.md` を新設し新規エントリの追加先を移す。todo13.md preamble を「既存タスクの編集・完了削除専用」に変更し、todo.md / 各 detail file の preamble と todo-summary.md のファイルリストを更新する。(Option B) 50KB 閾値の運用停止を意図的に決定した場合はその旨を全 preamble と todo-summary.md に明記する。

- [ ] Option A/B を決定 (現状は A 推奨)
- [ ] A 採用時: todo14.md 新設 + 全 preamble のルーティング記述を更新 (T01 の preamble 更新と同時実施)
- [ ] 本エントリ削除

##### 完了基準: todo13.md への新規エントリ追加が止まり、新規は 50KB 未満のファイルへ向かうこと (または 50KB 運用停止が全 preamble に明記されること)。

#### fetch_head_is_recent() の mtime 依存を埋め込み timestamp に置換 (週次レビュー WR-2026-07-19-J01 採用)

> **動機**: `fetch_head_is_recent()` が `.git/FETCH_HEAD` の mtime のみで fetch 鮮度を判定している。jj workspace 操作 (working copy materialization) で mtime がリセットされると false positive となり、実際は stale でも staleness nudge が発火しない可能性がある。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-07-19-J01 で採用 (severity=high, facet=jj-robustness, category=jj-mtime-staleness)
>
> **参照**: `.claude/weekly-reviews/2026-07-19.md` WR-2026-07-19-J01、`src/hooks-session-start/src/jj_helpers.rs:12-25`、[ADR-039](adr/adr-039-experimental-feature-standard-pattern.md) (jj-robustness facet の bounded lifetime dogfood 文脈)

##### 背景: 本 bug class (jj 操作による mtime リセット) は 2026-07 セッションで実観測済みで、新設 jj-robustness facet (ADR-039 bounded lifetime dogfood) が再検出した good signal。ただし jj new / workspace 操作が実際に `.git/FETCH_HEAD` の mtime を書き換える具体的機序は本レビューで再現検証しておらず、実装前に経験的確認を推奨する。

##### 設計決定: mtime 依存を廃し、jj git fetch 成功後に `.claude/fetch-last-run.json` 等へ埋め込みタイムスタンプを書き込み、そこから鮮度判定する方式に置換する (weekly-review last-run / telemetry と同じ「内容 timestamp は checkout 不変」方式、CR #233 の mtime リセット教訓と整合)。

- [ ] jj 操作が FETCH_HEAD mtime を書き換える機序を経験的に確認 (前提検証)
- [ ] 埋め込み timestamp 方式へ置換 + mtime リセットを模擬する回帰テスト
- [ ] 本エントリ削除

##### 完了基準: jj workspace 操作後も fetch 鮮度が正しく判定されること (mtime リセット模擬の回帰テストで seal)。

#### gh 呼び出しに --repo を付与 — 非 colocated jj workspace の PR 検出 silent 失敗 (週次レビュー WR-2026-07-19-J02 採用)

> **動機**: `detect_owner_repo()` (cli-merge-pipeline/src/github.rs:92-99) および `get_pr_info()` / `find_pr_via_jj_bookmarks()` (cli-pr-monitor/src/util.rs:31-68) が `--repo` 無しで `gh repo view` / `gh pr list` を呼び出しており、非 colocated jj workspace (`.git` 無し) で gh の自動検出が失敗し merge/monitor パイプラインが silent に PR 検出不能となる。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-07-19-J02 で採用 (severity=high, facet=jj-robustness, category=jj-gh-no-repo)
>
> **参照**: `.claude/weekly-reviews/2026-07-19.md` WR-2026-07-19-J02、`src/cli-merge-pipeline/src/github.rs:92-99`、`src/cli-pr-monitor/src/util.rs:31-68`、[ADR-045](adr/adr-045-jj-workspace-parallel-sessions.md)、PR #238 (実インシデント)

##### 背景: 既に実インシデント化しており、`.claude/hooks-config.toml` の gh-repo-env-guard preset コメントが PR #238 / ADR-045 を明記している。既存 guard は誤った回避策 (`GH_REPO=` の場当たり利用) をブロックするのみで、根本原因 (呼び出し箇所の `--repo` 欠落) は未修正。J01 と同じ ADR-039 dogfood 文脈。

##### 設計決定: `GH_REPO` 環境変数 or jj remote 由来で owner/repo を明示的に解決し、全 gh 呼び出しに `--repo` を付与する。

- [ ] github.rs / util.rs の gh 呼び出しに owner/repo 解決 + `--repo` 付与
- [ ] 非 colocated workspace を模擬した PR 検出の回帰テスト
- [ ] 本エントリ削除

##### 完了基準: 非 colocated jj workspace でも merge/monitor パイプラインが PR を正しく検出できること (回帰テストで seal)。

---

### 週次レビュー採用 (2026-07-01)

#### Stop hook `[stop_quality]` と push-runner `[quality_gate]` の lint/test 重複を解消 (週次レビュー WR-2026-07-01-A01 採用)

> **動機**: `.claude/hooks-config.toml` `[stop_quality]` と `push-runner-config.toml` `[quality_gate]` が同一チェック (pnpm lint / cargo clippy --workspace -- -D warnings / pnpm test / pnpm test:e2e / pnpm build) を重複実行している。`push-runner-config.toml` は「Rust lint + test group: push pipeline でのみ実行。PostToolUse / Stop hook では実行せず」と明記しているにもかかわらず `[stop_quality]` が cargo clippy 等を実行しており、コメントで宣言した責務境界と実態が乖離している。ADR-015 が push-time 品質ゲートを push-runner-config に移行した際の Stop hook cleanup 漏れ (systemic harness-duplication)。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-07-01-A01 で採用 (severity=high, facet=architecture, category=harness-duplication)
>
> **⚠ 計画書 PR-W5 との整合 (競合注意)**: `docs/file-length-enforcement-plan.md` PR-W5 は `[stop_quality.steps]` に **file-length step を追加**する予定。analyzer 推奨の Option A (`[stop_quality]` 全削除) をそのまま採ると PR-W5 の追加先が消えて競合する。**Option A' (整合版)**: 重複する lint/clippy/test step のみ削除し、session 固有チェック (file-length gate 等) の受け皿として `[stop_quality]` セクション自体は残す。着手は PR-W5 の file-length gate 確定後が安全。
>
> **参照**: `.claude/weekly-reviews/2026-07-01.md` WR-2026-07-01-A01、`.claude/hooks-config.toml` `[stop_quality]` (修正対象)、`push-runner-config.toml` `[quality_gate]` (lint/test single authority 候補)、`docs/file-length-enforcement-plan.md` PR-W5 (整合先)、ADR-004 (Stop hook 品質ゲート)、ADR-015 (push-runner 移行)、ADR-022 (責務分離)

##### 背景: ADR-015 で push-time quality gate を push-runner-config.toml に集約した際、ADR-004 由来の Stop hook `[stop_quality]` の lint/test step が削除されず残存。push-runner-config.toml 自身のコメントが「Stop hook では実行しない」と意図を明記しているため意図と実装の乖離が明白。ただし `[stop_quality]` は PR-W5 の file-length gate 受け皿としての将来用途があるため、セクション全削除ではなく重複 step の選択的除去が必要。

##### 設計決定: Option A' (推奨、PR-W5 整合版) — `[stop_quality]` から push-runner `[quality_gate]` と重複する lint/clippy/test step のみを削除し、session 固有チェック (PR-W5 の file-length step 等) の受け皿としてセクションは維持。quality_gate を lint/test の single authority とする。ADR-004 と ADR-015 に責務境界 (Stop hook = session 固有 / push gate = lint/test authority) を明記。Option B (意図的 defense-in-depth として両 ADR にコスト試算コメント追記) は代替案。

- [ ] PR-W5 (file-length gate) land 後に着手 or 並行時は `[stop_quality.steps]` 追加先の整合を確認
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
> **参照**: `.claude/weekly-reviews/2026-06-01.md` WR-2026-06-01-C02、`src/cli-merge-pipeline/src/feedback.rs:156-207` (修正対象)、`src/lib-pending-file/src/lib.rs` `is_valid_owner_repo()` (既存 validator)、ADR-022 § defense-in-depth 原則

##### 背景: cli-merge-pipeline の feedback path は merge 完了後に `gh pr view` / `gh api` で PR メタデータを取得する経路で、入力 `owner_repo` は pending file 由来。hook 経由の通常 path では `is_valid_owner_repo()` が呼ばれるが、broken pending file (Claude 編集ミス / 手動修正等) が cli-merge-pipeline に直接到達した場合は無検証で gh CLI に渡る (cli-merge-pipeline は hook と独立して起動可能)。

##### 設計決定: `fetch_pr_time_range()` 先頭で `is_valid_owner_repo(owner_repo)` を呼び出し、無効時は `Err` 返却。もしくは関数 signature を `&PendingFile` 受取に変更し型不変条件で保証 (より構造的)。

- [ ] Option A 採用判断 (1 行 guard) or Option B 採用判断 (型 signature 変更)
- [ ] `is_valid_owner_repo()` の re-export / dependency 確認 (lib-pending-file → cli-merge-pipeline)
- [ ] test 追加: 無効 owner_repo (`../../../etc`, `owner;rm -rf`, `owner` (no slash) 等) で `Err` 返却を assert
- [ ] cargo test + cargo clippy pass
- [ ] 本エントリ削除 + todo-summary.md 行追加削除

##### 完了基準: 無効 `owner_repo` が gh CLI に到達しない (defense-in-depth)、test で各 invalid pattern (path-traversal / shell-injection / format-violation) を assert、lib-pending-file の既存 validator と挙動一致

#### CLAUDE.md ADR index に ADR-032 (reserved) スタブ追加 + ADR-033 参照を実在 ADR に差し替え (Phase E dogfood WR-2026-06-01-A01 採用)

> **動機**: CLAUDE.md の ADR インデックスが ADR-031 → ADR-033 へ飛び **ADR-032 が欠落**、`docs/adr/adr-033-todo-numbering-simplification.md:40-42, 81, 95, 130` は `ADR-032 PR-β` を 4 箇所参照しているが対応ファイル不在。Cross-File Reference Lifecycle ルール (permanent → 不在の永続成果物を参照不可) に違反した dead-pointer。reserved 状態 (`docs/todo2.md:232` で `adr-032-docs-only-fast-path.md` 起案予定として trackable) を CLAUDE.md にも明示する必要。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-06-01-A01 で採用 (severity=medium, facet=architecture, category=docs-internal)
>
> **⚠ 再検出 (2026-07-01)**: 本タスクは 2026-06-01 採用後 **1 か月未着手**のため、2026-07-01 週次レビューで同一問題が WR-2026-07-01-A02 (severity=**high** に昇格) として再検出された。ADR-031 重複検出方針により重複エントリは作らず本タスクに集約。優先度の引き上げを推奨。
>
> **参照**: `.claude/weekly-reviews/2026-06-01.md` WR-2026-06-01-A01、`.claude/weekly-reviews/2026-07-01.md` WR-2026-07-01-A02 (再検出)、`CLAUDE.md:5-45` (ADR index、修正対象)、`docs/adr/adr-033-todo-numbering-simplification.md:40-42, 81, 95, 130` (`ADR-032 PR-β` 参照 4 箇所、修正対象)、`docs/todo2.md:232` (ADR-032 reserved 文脈)

##### 背景: ADR-032 は「docs-only fast-path」関連の試験運用 ADR として `docs/todo2.md` 順位 20 で起案予定だが未作成。一方 ADR-033 は task naming 例示として `ADR-032 PR-β` を使用済みで、CLAUDE.md は ADR-031 → ADR-033 へジャンプする状態。reader が CLAUDE.md から ADR-032 を辿ろうとすると broken-link、ADR-033 から ADR-032 を辿ろうとしても dead-pointer。

##### 設計決定: Option A (recommended、reservation 明示) — CLAUDE.md ADR index に `- ADR-032: (reserved — docs-only fast-path、起案 docs/todo2.md 順位 20)` のスタブ行を追加し、ADR-033 内の `ADR-032 PR-β` を **実在 ADR (例: ADR-031 PR-β 相当の task 名)** に差し替えるか、`(reserved ADR-032 のタスク名例)` 等の reservation 明示 wording に変更。Option B (ADR-032 を本作業で実体化) は scope creep のため不採用、別 task として `docs/todo2.md` 順位 20 で trackable。

- [ ] CLAUDE.md ADR index に `ADR-032: (reserved)` 行追加 (位置: ADR-031 と ADR-033 の間)
- [ ] `docs/adr/adr-033-todo-numbering-simplification.md:40-42, 81, 95, 130` の `ADR-032 PR-β` 参照 4 箇所を差し替え判断 (実在 ADR 引用 or reservation 明示 wording)
- [ ] `grep -rn 'ADR-032' docs/ CLAUDE.md ~/.claude/` で他の dead-pointer 残存確認
- [ ] markdownlint / `pnpm exec cli-docs-lint` 等で broken-link 解消確認
- [ ] 本エントリ削除 + todo-summary.md 行追加削除

##### 完了基準: CLAUDE.md ADR index が ADR-031 → ADR-032 (reserved) → ADR-033 で連続化、ADR-033 の `ADR-032 PR-β` 参照 4 箇所が dead-pointer ではなくなる、`grep -rn 'ADR-032'` で残存 dead-pointer 0 件

### マージ後フィードバック機構の決定論化 (ADR-030 起案 + 実装)

> **動機**: PR #74 マージ後の dogfood で、ADR-029 設計の **silent loss 問題** が顕在化した。Stop hook + skill ベースの auto-trigger は Claude のターン取得次第で機能せず、決定論的実行が成立しない。skill 機構は本質的に "ask-based" であり must-run 要件には不適合という設計上の知見が得られた。
>
> **本タスクの位置づけ**: ADR-029 を partial supersede する新 ADR-030 を起案し、takt 経由の決定論的フィードバック機構へ移行する。本タスク完了で post-merge-feedback skill / pending file / Stop hook (hooks-stop-feedback-dispatch) はすべて廃止される。
>
> **Status update (2026-06-06)**: Phase D-7 (Drop guard + orphan reaper + ADR-030 spec) は **PR #154 (`c872da229df3`) で land 済**。L1 Floor / L2 Recovery / Drop guard / orphan reaper の決定論層は本採用昇格相当で運用中。残るは **Phase E (旧機構廃止 = post-merge-feedback skill + hooks-stop-feedback-dispatch crate + lib-pending-file + ADR-029/014 ステータス更新)** と **Phase F (dogfood 検証)**。Phase E 着手前提条件 (Phase D-7 land) は満たされた。
>
> **実行優先度**: 🧹 **Tier 4** — Phase A〜D は merged 済で workflow は機能。残る Phase E (旧機構廃止) / Phase F (dogfood) は cleanup 中心で daily efficiency への直接効果は小。Tier 1〜3 完了後の片付けタイミングで実施推奨。

#### 背景: ADR-029 の構造的欠陥 (PR #74 dogfood で実証)

ADR-029 は 4 層のトリガー機構を直列に積んでいるが、後半 2 層が非決定的:

| 層 | 機構 | 決定論性 | PR #74 で何が起きたか |
|---|------|---------|---------------------|
| 1 | cli-merge-pipeline が pending file を書き込む | ✅ 決定論的 | 正常動作 (status=pending) |
| 2 | Stop hook が pending を読み additionalContext を出力 | ✅ 決定論的 | 正常動作 (status→dispatched) |
| 3 | Claude が次ターンで additionalContext を読む | ❌ **非決定的** | ユーザー入力先行で次ターン消失 |
| 4 | Claude が skill 起動命令と解釈・実行 | ❌ **非決定的** | (3 で詰まったので未到達) |

層 3 はセッションライフサイクル依存 (ユーザー入力 / VSCode 終了 / Claude Code 再起動 で容易に壊れる)。層 4 は skill design philosophy が ask-based (AskUserQuestion で中止可、命令無視可)。**must-run 要件に skill を主動線で使うのは設計ミスと判定**。

dogfood では PR #74 マージ後、pending file が `dispatched` で stuck した状態で session が終了 → 24h 後に stale 削除 → フィードバック silent loss という最悪の経路が再現可能。

#### 設計決定: 2 層アーキテクチャ (新 ADR-030)

| 層 | 機構 | 保証レベル | 失敗時 |
|---|------|-----------|--------|
| **L1 Floor** (決定論) | cli-merge-pipeline → takt workflow `post-merge-feedback` を **同期実行** | Deterministic invocation: 成功 → at-most-once でレポート生成、失敗 → `.failed` marker で retryable (詳細は ADR-030 参照) | soft: merge 成功、`<pr>.md.failed` marker 残存 |
| **L2 Recovery** (safety net) | UserPromptSubmit hook が `*.md.failed` を検出 → additionalContext で再実行指示 | At-least-once (ユーザーが何か入力すれば必ず発火) | hook 自体は決定論的、Claude の応答は best-effort (ただし floor は既存なので silent loss は起きない) |

- **失敗ポリシー**: soft (merge 成功 + marker 残存。後続 prompt 入力で L2 が拾う)
- **skill enrichment 層 (旧案 L3) は廃止** (ask-based の弱点を再導入してしまうため)
- **入力源**: PR data (gh API) + pre-push reports (`.takt/runs/`) + transcript (`~/.claude/projects/<id>/*.jsonl`、commit 時刻 range filter)
- **出力**: `.claude/feedback-reports/<pr>.md`

#### Phase 0 調査結果 (実施済 — 2026-04-25)

##### transcript ファイル所在 (確認済)

`~/.claude/projects/<project-id>/<session-id>.jsonl` (1 session = 1 file, UUID 命名)

本プロジェクト: `%USERPROFILE%\.claude\projects\e--work-claude-code-hook-test\`

##### transcript スキーマ (確認済)

```json
{
  "parentUuid": "...",
  "isSidechain": false,
  "type": "user" | "assistant" | "attachment" | "queue-operation",
  "timestamp": "2026-04-25T05:44:35.040Z",
  "sessionId": "<uuid>",
  "cwd": "E:\\work\\claude-code-hook-test",
  "gitBranch": "HEAD",
  "message": {
    "role": "user" | "assistant",
    "content": [
      { "type": "text", "text": "..." },
      { "type": "thinking", "thinking": "", "signature": "<encrypted>" },
      { "type": "tool_use", "name": "Bash", "input": {...} }
    ]
  }
}
```

##### 重要な制約 (実装時に必ず参照)

| 観察 | 影響 |
|---|---|
| timestamp は ISO 8601 ms 精度 | commit 時刻からの逆引き filter が容易 |
| `thinking` content は encrypted (`signature` のみ可視、`thinking` field は空) | chain-of-thought は抽出不可。user/assistant text + tool calls/outputs で十分 |
| `gitBranch` は `HEAD` 固定 (jj detached state のため) | branch 名 filter は **使えない**。**時刻 range で filter する必要** |
| 1.7 MB / 621 行 (現セッション例) | takt context window 圧迫の可能性。filter 後の絞り込みが必須 |
| `type: queue-operation` はノイズ | parsing で skip すべき |

##### transcript 抽出戦略 (Q1 = commit 時刻逆引きの具体化)

```text
入力: <pr_number>
1. gh pr view <pr> --json commits,mergedAt → first_commit_time, end_time 取得
2. ~/.claude/projects/<project-id>/*.jsonl の全ファイルを mtime ∈ [first_commit_time, end_time + 1day buffer] でフィルタ
3. 該当 file 内で entry.timestamp ∈ [first_commit_time, end_time] かつ type ∈ {user, assistant} を抽出 (queue-operation, attachment は除外)
4. 合成 in-memory log を analyze-session facet に渡す
```

#### ユーザー判断記録 (本タスク策定時に合意済)

| 質問 | 回答 |
|---|---|
| 失敗時の挙動 | **soft** (merge 成功、後続 prompt で L2 が再実行) |
| レイテンシ許容 | **数分の追加レイテンシ OK** |
| Anthropic API 直接呼出し | **禁止** (pre-push-review / post-pr-monitor と同じ takt 経由パターンに統一) |
| transcript 紐付け方針 | **PR commit 時刻から逆引きして transcript 区間を抽出** |
| takt facets 構造 | **4 facets に分離** (`analyze-pr` / `analyze-session` / `analyze-prepush-reports` / `aggregate-feedback`) |
| PR 分割 | **PR 1 (ADR) → PR 2 (B) → PR 3 (C) → PR 4 (E)** の 4 段階 |
| 旧機構廃止 | 本作業計画に **Phase E として含める** (dogfood 数回後に実施) |

#### 作業計画

##### Phase D: ❌ 廃止 (skill enrichment 不要)

##### Phase E: 旧機構廃止 (PR 4 — Phase B/C dogfood 数回後)

- [ ] post-merge-feedback skill 削除
  - `~/.claude/skills/post-merge-feedback/` 削除
  - `E:\work\claude-code-skills\post-merge-feedback\` 削除
- [ ] hooks-stop-feedback-dispatch.exe / `src/hooks-stop-feedback-dispatch/` crate 削除
  - workspace member から外す
  - `package.json` の build/deploy script 削除
  - `.claude/hooks-stop-feedback-dispatch.exe` 配布物削除
- [ ] `src/lib-pending-file/` crate 廃止 (cli-merge-pipeline からの依存削除、`pending_file.rs` も整理)
- [ ] `.claude/hooks-config.toml` から Stop hook の `hooks-stop-feedback-dispatch` 登録を削除 (既に明示登録されていない可能性あり、要確認)
- [ ] settings.local.json + `templates/settings.json` から Stop hook 登録解除
- [ ] `.gitignore` から `.claude/post-merge-feedback-pending.json` 行を削除 (もう使わない)
- [ ] ADR-029 のステータスを `Superseded by ADR-030` に変更
- [ ] ADR-014 のステータスを `Superseded by ADR-030` に変更
- [ ] memory `feedback_*.md` / `project_*.md` で post-merge-feedback skill / pending file に言及している記述を更新
- [ ] CLAUDE.md の Architecture Decisions リストで ADR-014 / ADR-029 のステータス記載を更新

##### Phase F: dogfood 検証 (PR 4 マージ後 / 継続観察)

- [ ] 3-5 回の実マージで feedback report が **必ず生成** されることを確認 (silent loss 0 を証明)
- [ ] L2 recovery を人為的失敗で発火確認 (cli-merge-pipeline で takt fail を inject、`<pr>.md.failed` marker 残存 → 次 prompt で UserPromptSubmit hook 発火 → 再実行成功)
- [ ] transcript からの session 知見抽出が想定通りか確認 (実装時の学び・トラブル・ユーザー指示が拾えるか)
- [ ] feedback report の品質評価 (ADR 提案 / 仕組み改善案が出るか、Plankton 優先度が機能しているか)

#### 作業可能になるための前提情報 (新セッションで必読)

##### 既存コンポーネントとの参照関係

- **既存 takt workflow 例** (新 workflow `post-merge-feedback` も同じパターンで構築):
  - `pre-push-review`: simplicity-review + security-review facets (ADR-020)
  - `post-pr-review` (cli-pr-monitor 内): analyze-pr-review-comments + supervise / fix facets (ADR-018)
- **既存 cli-merge-pipeline**: post_steps `type = "ai"` 分岐の現行実装 (pending file 書き込み) を Phase B で takt workflow 起動に置き換える
- **既存 skill `analyze-pr`** (`E:\work\claude-code-skills\analyze-pr\SKILL.md`): facet `analyze-pr.md` への port 元
- **既存 skill `post-merge-feedback`** (`E:\work\claude-code-skills\post-merge-feedback\SKILL.md`): Phase 4 統合フィードバックロジックを `aggregate-feedback.md` facet への port 元

##### 重要な既存 ADR (実装時に必ず参照)

| ADR | 関係 |
|---|---|
| **ADR-014** | post-merge-feedback skill 自体の起案 (試験運用)。本タスク完了で **Superseded** |
| **ADR-015** | push-runner takt 移行。本タスクの設計パターン (CLI exe → takt workflow) の **先行事例** |
| **ADR-018** | cli-pr-monitor takt 移行。同上 |
| **ADR-020** | takt facets (fix/supervise) 共通化戦略。本タスクの **4 facets 分離方針の根拠** |
| **ADR-022** | 自動化コンポーネントの責務分離原則。L1 takt 経由は本原則に整合 (Claude 不在でも動く) |
| **ADR-026** | Cargo workspace。新 crate (`hooks-user-prompt-feedback-recovery`) はこのワークスペースに追加 |
| **ADR-028** | 外部可視成果物ゲート。本タスクは内部 artifact のみで対象外 |
| **ADR-029** | 本タスクで **partial supersede** (層 3-4 廃止、層 1 流用) |

##### memory 参照

- `feedback_side_effect_integration.md`: cleanup / consume 処理を新 phase ではなく既存 phase 末尾に統合する原則 (本設計の Phase D 廃止判断にも適用)
- `feedback_verify_edit_results.md`: 大きな Edit 後は grep で見出し検証 (Phase B での takt workflow / facets ファイル作成時に有用)
- `project_takt_push_runner_learnings.md`: takt 導入の知見 (バージョン固定、ハイブリッド構成等)
- `project_takt_pre_push_iterations.md`: takt fix の child commit 自動収束パターン

##### 残存する PR #74 の pending file の扱い

- 現状 `.claude/post-merge-feedback-pending.json` に PR #74 の pending file が残存している可能性 (status=dispatched のまま consume されていない)
- **対処方針**: Phase E で pending file 機構ごと廃止するため明示対処不要。Phase A 着手前に手動 `rm` でもよい (新セッション開始時の状態を整理する目的なら推奨)

##### 新セッションで最初に確認すべきこと

1. `git log --oneline -5` で master の最新状態を確認
2. `docs/todo.md` の本セクション (本記録) を読む
3. `docs/adr/adr-029-post-merge-feedback-auto-trigger.md` を読む (supersede 元の理解)
4. `docs/adr/adr-014-post-merge-feedback.md` を読む (skill 自体の元設計)
5. `docs/adr/adr-015-push-runner-takt-migration.md` / `docs/adr/adr-018-pr-monitor-takt-migration.md` を読む (takt 移行の先行事例として参考)
6. `docs/adr/adr-020-takt-facets-sharing.md` を読む (facets 共通化方針の根拠)
7. Phase C から着手 (Phase A: ADR 起案 / Phase B: takt workflow + facets + cli-merge-pipeline 統合 はマージ済)

#### 完了基準

- Phase A〜F すべて完了
- dogfood で 3-5 回連続マージ → 全 PR にレポート生成 (silent loss 0 を実証)
- ADR-029 / ADR-014 のステータスが `Superseded by ADR-030` に更新済
- 旧 skill / hook / lib-pending-file が repository から削除済
- 本 todo.md エントリを削除 (運用ルール: 完了タスクは ADR/仕組みに反映後に削除)

#### 詰まっている箇所

なし (全方向確定済、Phase A から着手可能)

### (追って) ADR-030 の takt-test-vc 反映

> **参照**: 上位タスク「マージ後フィードバック機構の決定論化」の Phase F 完了が前提。元の 1-F (ADR-014 本採用化 + takt-test-vc 反映) は ADR-014 が ADR-030 で Superseded されるため scope 変更。
>
> **実行優先度**: ⏳ **Tier 5** — 派生プロジェクトへの展開で本リポジトリへの効果はゼロ。ADR-030 Phase F 完了後の任意タスク。

- **やろうとしたこと**: 本プロジェクトで ADR-030 機構が安定稼働 (Phase F dogfood 完了) した後、takt-test-vc へ機構ごとバックポート
- **現在地**: 上位タスクの Phase F 完了待ち
- **詰まっている箇所**: ADR-030 実装 + dogfood 完了に依存

---

## スコープ外だが将来検討

### ADR-027 / PR #47 由来

- [ ] **loop_monitor judge の軽量化**: step 間 transition で毎回 AI 呼び出しされる judge を、閾値到達前はスキップする最適化。takt 本体にオプションがあるか未調査。実測で隠れオーバーヘッドが 15-70s/遷移、17-iter run では累計 ~6 分
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
