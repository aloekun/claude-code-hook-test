# TODO

> **運用ルール**: 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。

---

## 現在進行中

### 1. マージ後フィードバックの定常化 (cli-merge-pipeline の post_steps 統合)

- **やろうとしたこと**: `pnpm merge-pr` 後の「ADR 記録すべきもの」「仕組みに反映すべきもの」の手動依頼を自動化。ADR-014 で提唱された `post-merge-feedback` スキルを cli-merge-pipeline から自動起動する
- **現在地**: 設計段階。未着手
  - [ ] `src/cli-merge-pipeline/src/main.rs` の `run_steps` の `"ai"` 分岐を現在の `SKIP` から実装に置き換える (takt 経由で skill を起動、または claude -p で起動)
  - [ ] `.claude/hooks-config.toml` の `[[merge_pipeline.post_steps]]` に `type = "ai"`, `prompt = "post-merge-feedback"` を設定
  - [ ] `post-merge-feedback` スキルが PR 番号とブランチ名を受け取れるよう、cli-merge-pipeline から環境変数または引数で渡す設計
  - [ ] マージ済みセッションの会話ログを参照する手段 (Claude Code Session ID 等) の検討
- **詰まっている箇所**:
  - **主要ブロッカー**: 「マージ時点のセッション会話」を post_steps 用の新セッションに引き継ぐ手段が決まっていない。会話ログがないと「何を議論した末のマージか」が失われ、フィードバック品質が下がる
    - **Why**: post-merge-feedback は ADR-014 で「セッション知見 + PR 知見の統合」を前提にしているが、merge-pipeline は別プロセスで起動されるため会話がない状態から始まる
    - **How to apply / 再開手順**: SessionStart hook (master の `src/hooks-session-start/`) で伝播した session ID を jsonl transcript に紐付けて読み取る方式が候補。ADR を書いてから実装
  - **制約**: ADR-016 (長時間コマンド) のため、post_steps の AI 起動も `run_in_background: true` + `timeout: 600000` 前提で設計する必要あり
- **依存関係**:
  - SessionStart hook は master に実装済み (`src/hooks-session-start/`)。セッション引継ぎ設計は session ID → jsonl transcript 紐付けの ADR が必要
  - takt-test-vc での試験運用を先に行い、本プロジェクトに反映

### 2. post-pr review フローの並行通知化 (観測用 BG タスク追加)

- **やろうとしたこと**: `pnpm create-pr` 実行中に CodeRabbit 指摘検出 → takt 自動修正 → re-push が BG で進行するが、ユーザーは完了通知まで状況を把握できない。Claude Code にも進捗が届かず、すでに自動修正されている指摘についてユーザーから「未対応レビューをリストアップして」と重複依頼が発生する。これを早期通知で解消する
- **現在地**: 設計確定、未実装
- **背景**:
  - 現状フロー (cli-pr-monitor が一気通貫): `poll → detect → takt fix → re-push → report → exit`
  - ユーザーは exit 時点まで中間状態を見られない
  - 深刻度別の扱い:
    - **Critical / Major**: 既存の `auto_push_severity` 設定で自動修正
    - **Minor 以下**: 並行してユーザーにヒアリング。主フローの外で判断
- **設計原則 (絶対に崩さないこと)**:
  - **主フロー (detect → fix → re-push) は 100% 機械的**。Claude Code の判断や通知受領を gate にしない。セッション切断や AI スキップでフローが止まると、ハーネスとしての「必ず修正まで到達する」保証が崩れる
  - **通知は side effect**。主フローの成否に影響しない観測用パス。cli-pr-monitor 自体には手を入れない
- **実装内容 (並行タスク方式)**:

  ```
  Claude Code が 2 つの BG タスクを同時起動:

  Task A (主フロー): pnpm create-pr
    → cli-pr-monitor 既存フローをそのまま実行
    → 変更不要

  Task B (通知用): scripts/observe-pr-state (新設)
    → <exe_dir>/pr-monitor-state.json をポーリング（cli-pr-monitor.exe と同じディレクトリ）
    → action=action_required を検出したら state 内容を stdout に出して exit
    → Claude Code が完了通知を受領 → ユーザーにレポート表示 + Minor ヒアリング
  ```

- **タスク分解**:
  - [ ] `scripts/observe-pr-state.ps1` (Windows 用) 新設
    - `<exe_dir>/pr-monitor-state.json`（cli-pr-monitor.exe と同じディレクトリ）を 5-10s 間隔ポーリング
    - `action == "action_required"` または `action == "approved"` 検出で state 全文を出力して exit 0
    - 10 分のタイムアウト (ADR-016 準拠) 後は exit 1
    - 起動時に `notified` フラグを確認し、`true` であれば出力をスキップして exit 0（Claude Code 再起動時の重複レポート防止）
      - `notified=true` への書き込みは `cli-pr-monitor` の `mark_notified` ステージ（`src/cli-pr-monitor/src/stages/mark_notified.rs`）が担う
      - `notified=false` へのリセットは新しい監視セッション開始時に `cli-pr-monitor` 側で行う（observer は stateless/single-shot のため自身ではリセットしない）
  - [ ] `package.json` に `"observe-pr"` スクリプト追加 (PowerShell 起動)
  - [ ] `post-pr-create-review-check` スキルを修正
    - 現状: daemon 起動後に state file を一度読んで報告する一段構成
    - 変更後: `pnpm create-pr` と `pnpm observe-pr` を並行 BG 起動
    - observer exit 時に state を整形してユーザーに提示
    - Minor 指摘があれば AskUserQuestion で対応方針をヒアリング
  - [ ] E2E 検証: CodeRabbit Major ありの PR で通知タイミングを確認
- **詰まっている箇所**:
  - **Windows 依存**: Bash 経由では PowerShell 起動が二重シェルで動作不安定。pnpm script で `pwsh -File` 直接呼び出しが必要か要調査
  - **Minor ヒアリングの UX**: observer 完了時に Claude Code に state が届くが、並行して Task A の fix も進行中。Task A が Minor を `user_decision` verdict で止めた場合と action_required で進んでいる場合の挙動差を確認
- **考慮事項**:
  - observer は read-only (state file 読み取りのみ)。Task A に影響しない
  - Task A と Task B 両方タイムアウト時のクリーンアップ (observer 側は orphan OK、Task A 側は既存タイムアウト機構に委ねる)
- **参照 ADR**:
  - ADR-018 (post-pr-monitor takt 化): 既存フローの前提
  - ADR-016 (長時間コマンド): observer の 10 分タイムアウト設計

### 3. cli-pr-monitor の auto re-push に bookmark 自動前進を移植

- **やろうとしたこと**: takt 自動修正後の auto re-push で「修正コミットができても bookmark が動かず remote に届かない」問題を解消。cli-push-runner には PR #50 で `push_jj_bookmark.rs` の advance ロジックが入っているが、cli-pr-monitor の `run_push` は `jj new` → `jj git push` だけで bookmark を進めない
- **現在地**: port 完了 + 統合テストで機能等価を確認。あとは実 PR でのロールアウトのみ
  - [x] `src/cli-pr-monitor/src/stages/push_jj_bookmark.rs` 新設 (cli-push-runner から port、log prefix は `[action]`/`[state]` に調整、`lib_jj_helpers::is_trunk_bookmark` 再利用)
  - [x] `src/cli-pr-monitor/src/stages/push.rs:run_push` の `jj new` 後・push 前に `advance_jj_bookmarks` を挿入 (`push_command` が `jj ` で始まる場合のみ、失敗時はログして push 続行)
  - [x] unit テスト (dedup / parse_bookmarks_from_template / parse_bookmark_list_output / dispatch_bookmark_advance)
  - [x] 統合テスト `integration_advance_moves_bookmark_to_parent_after_jj_new` で実 jj を使い PR #53 症状の退行防止を確認 (push-runner-config の rust-test グループで自動実行される `#[ignore]` テスト、`--test-threads=1` 必須)
  - [ ] 実 PR での E2E 検証: 次回 CodeRabbit Major 指摘が出た PR で、takt 修正 → auto re-push で bookmark が remote 反映まで自動到達することを目視確認 (本 PR マージ後にリリース)
- **詰まっている箇所**:
  - **共通化方針**: まず port で機能等価を確認。将来 `lib-jj-helpers` へ集約する候補として `push_jj_bookmark.rs` 先頭に TODO コメントを残した (ADR-024)
- **参照 ADR / PR**:
  - PR #50 (cli-push-runner の bookmark fallback)
  - PR #63 (takt fix のコミット分離、完了済)
  - ADR-024 (共通 jj helper、試験運用)

### 4. post-pr-review workflow の verdict に push 反映確認を追加

- **やろうとしたこと**: post-pr-review workflow が local file の状態だけを見て `approved` を判定する設計のため、auto re-push が失敗していても `approved` になる gap を埋める。PR #53 で「local 修正済み + bookmark 未前進 → workflow approved」の食い違いを実測
- **現在地**: 設計段階、未着手。task 3 (bookmark 自動前進) の defense-in-depth 的位置付け
- **実装内容案**:
  - [ ] workflow の analyze (or 終了前) ステップで `gh api` を叩いて remote の最新 commit SHA を取得し、local の fix commit SHA と比較
  - [ ] 一致しなければ `verdict = action_required` にダウングレードし「remote 未反映」のメッセージを出す
- **詰まっている箇所**:
  - **task 3 実装後の優先度再評価**: task 3 が入れば「auto re-push が失敗しない限り approved は妥当」となるため、本タスクの優先度は大きく下がる。実装可否は task 3 完了後に判断
  - **takt workflow の YAML から外部コマンド呼び出しが可能か**: facets instruction 内でシェルが呼べるかは要調査 (ADR-019/020 の facets 設計内)
- **参照 ADR**:
  - ADR-018 (post-pr-monitor takt 化)
  - ADR-019 (CodeRabbit レビュー運用ハイブリッド)
  - 関連: task 3 (bookmark auto-advance)

### 5. prepare-pr skill の前提条件を ADR-022 v3 に整合

- **やろうとしたこと**: `prepare-pr` skill が前提不成立で停止する運用痛を解消する。現状の前提 3「jj working copy は `pnpm push` 完了済」は ADR-022 v1 時代 (Claude が commit description / bookmark / push に触れない前提) の書き方で、ADR-022 v3 で確立した「Claude が草案生成 → 承認 → 実行」フローと噛み合っていない
- **背景**:
  - PR #64 セッション冒頭で発生: 「PR を作成して」と依頼された際、skill が前提 3 で停止し、ユーザーに「commit description 書いて pnpm push まで先にやれ」と返した。しかし ADR-022 v3 の承認ゲート表では commit description / bookmark / pnpm push はすべて Claude に委譲される想定
  - 結果として skill の責務境界が実運用とズレており、ADR-022 v3 を書く契機になった
  - ADR-022 v3 側は修正済み (PR #64)。skill 側が取り残されている
- **現在地**: 設計段階、未着手。skill の本体は global + `$CLAUDE_SKILLS_REPO` (`C:\Users\HIROKI\.claude\skills\prepare-pr\` / `E:\work\claude-code-skills` 配下)。本リポジトリ外
- **作業内容**:
  - [ ] `SKILL.md` の「前提条件」セクション書き換え:
    - 旧: 前提 3「jj working copy は pnpm push 完了済」
    - 新: 「commit description / bookmark / push が完了している。未完了なら skill 外で Claude が順次実行してから起動する」(or skill 内 Step 0 で実行)
  - [ ] 「実行手順 > Step 1: 現状確認」のチェック項目も前提変更に合わせて書き換え。`master..@` 差分空 / bookmark なし / remote 未反映は「skill の責務外のはずだが、運用中に前提未達なら Claude に fallback を促す」方針に
  - [ ] `draft.md` の入力セクション (`jj log -r 'master..@'` 等) が「空」を返す場合の fallback 記述を追加
  - [ ] ADR-022 v3 承認ゲート表 への参照リンクを skill 冒頭に明記
  - [ ] evals/evals.json の scenario 2-4 (前提未達 → 早期終了) を新設計に合わせて更新
- **完了条件**:
  - 「PR を作成して」依頼で skill が止まらず、PR 作成まで抜ける happy path を 1 回検証
  - `$CLAUDE_SKILLS_REPO` と global deploy 先 (`~/.claude/skills/prepare-pr/`) の同期を `/skill-sync-check` で確認
- **参照**:
  - ADR-022 v3 (PR #64) — 承認ゲート表 / 原則 1 再構築
  - PR #62 (task 7) — skill を global + skill repo に分割した経緯
  - memory `feedback_bookmark_auto_naming.md` — Claude の権限境界
  - PR #64 の本セッション記録 — 前提不成立で停止した実例

### 6. cli-pr-monitor の空 fix commit cleanup で @ を PR tip に re-parent

- **やろうとしたこと**: takt fix が NoChange で空 child commit を abandon した後、`@` (working copy) が孤児位置に残る問題を解消。結果として次の `jj new` が孤児 commit の上に積まれ、手動で `jj abandon` + `jj new -r <tip>` の修正が必要になる
- **背景**:
  - PR #64 セッション内で 3 回発生:
    - 1 回目 (原則 5 child commit 作成時): 親が `sprrwyln a0a15f2f` (空)
    - 2 回目 (3→4 条件修正時): 親が `pqxmuwvo ba3c0b65` (空)
    - 3 回目 (MD040 修正時): 親が `qmxmoqnm 02e1cbf4` (空)
  - 毎回 `jj abandon <current>` → `jj abandon <stale>` → `jj new -r <PR-tip>` の 3 ステップ手修正が必要だった
  - `cli-pr-monitor/src/stages/` 配下で空 commit を abandon する処理はあるが、abandon 後の `@` 再配置までは実装されていない
- **現在地**: 原因箇所特定済み。未実装
- **作業内容**:
  - [ ] `src/cli-pr-monitor/src/stages/` で fix_state=Created 後の `CleanupEmptyFixCommit` 分岐を特定 (本セッションログで該当メッセージ確認済)
  - [ ] abandon 後に `jj edit <pr-tip-commit-id>` または `jj new <pr-tip>` で `@` を PR tip 直下に戻す処理を追加
  - [ ] PR tip の commit ID は既存の state (`post_takt_commit_id` の親、または bookmark の指す先) から取得可能
  - [ ] unit テスト: NoChange 分岐で @ が PR tip に戻ることを assert
  - [ ] 統合テスト: takt fix NoChange → `jj log -r @` が PR tip の直接子であることを確認
- **詰まっている箇所**:
  - **PR tip の確定方法**: abandon 対象の空 commit の親が PR tip とは限らない (空 commit 自体が残留孤児の上にある場合もある)。bookmark の指す commit を信頼する方が堅い
  - **interactive Claude Code との衝突**: ユーザーが skill 経由で作業中に cli-pr-monitor が @ を動かすのは違和感がある。ただし cli-pr-monitor の実行タイミングは pnpm push 後の BG 処理なので、ユーザー操作中には走らない想定
- **関連ファイル**:
  - `src/cli-pr-monitor/src/stages/push.rs` (空 commit 処理)
  - `src/cli-pr-monitor/src/fix_commit.rs` (PR #63 で追加された fix commit 分離実装)
- **参照**:
  - ADR-022 原則 5 (PR 包含 changeset の不変性) — 本問題は原則 5 実装の副作用の一つ
  - PR #63 (takt fix の child commit 分離) — 空 commit が出るようになった起点
  - PR #64 の実地記録 — 3 回連続発生した事例

### 7. ADR-028 に ADR-022 原則 5 との関係を明記

- **やろうとしたこと**: ADR-028 (外部可視成果物の生成コマンドの実行ゲート) と ADR-022 原則 5 (PR 包含 changeset の不変性) が別々の観点 (作成ゲート vs 改変ゲート) から PR ライフサイクルを規律しており、両者の関係を 1 節で明記して読み手の混同を防ぐ
- **背景**:
  - ADR-022 原則 5 は「changeset が PR に含まれる場合 amend 禁止、修正は child commit」。一方 ADR-028 は「PR 作成/マージのような外部可視イベントは承認ゲート経由」
  - `pnpm create-pr` と `pnpm merge-pr` 自体は履歴書き換えではないので ADR-022 原則 5 の拘束外。この線引きを ADR-028 側に 1 節書いておくと、将来「merge は amend 扱いか?」のような混乱を予防できる
  - PR #64 セッション retrospective で拾った気付き
- **現在地**: 設計確定、未実装 (docs only、PR 粒度は小)
- **作業内容**:
  - [ ] ADR-028 の「影響」または末尾に「関連 ADR との境界」セクションを追加
  - [ ] 記述内容:
    - ADR-028 の射程 = 外部可視 artifact (PR / release tag) の**生成**ゲート
    - ADR-022 原則 5 の射程 = 既存 changeset の**改変**ゲート
    - 重なり: `pnpm create-pr` / `pnpm merge-pr` 自体は履歴書き換えでないため ADR-022 原則 5 の拘束外、ADR-028 のみが適用
    - PR 作成後の commit 追加は ADR-022 原則 5 の child commit ルール適用、ADR-028 の追加ゲートは不要
  - [ ] ADR-022 側にも対応する cross-ref 追記を検討 (原則 5 本文か「影響」セクションに 1 行)
- **完了条件**: ADR-028 に 1 節追加、cross-ref が双方向に張られている
- **関連**:
  - ADR-022 v3 (原則 5) — PR #64 で追加
  - ADR-028 (外部可視成果物の生成コマンドの実行ゲート) — 既存
  - PR #64 retrospective — 本タスクの起点

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
