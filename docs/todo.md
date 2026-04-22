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

### 2. post-pr review フローの並行通知化 (E2E 検証待ち)

- **やろうとしたこと**: `pnpm create-pr` 実行中に CodeRabbit 指摘検出 → takt 自動修正 → re-push が BG で進行する間、Claude Code が中間状態を受け取れず「未対応レビューをリストアップして」の重複依頼が発生していた。observer パスで早期通知して解消する
- **現在地**: 実装完了 (ADR-018 追記セクションに仕組み反映済み)。残るは実 PR での E2E 観察のみ
  - [x] `src/cli-pr-monitor/src/stages/observe.rs` 新設 (Rust exe サブコマンド、PowerShell 廃案)
  - [x] `cli-pr-monitor --observe` ハンドラを `main.rs` に配線 + observe stage の unit test 7 件
  - [x] `poll.rs`: iteration を跨いで `notified` flag を preserve (`PrMonitorState::new` が毎回 reset する挙動を修正)
  - [x] `start_monitoring` 冒頭で state を明示初期化 (新セッション開始時の reset)
  - [x] `package.json` に `observe-pr` / `mark-notified` スクリプト追加
  - [x] `~/.claude/skills/post-pr-create-review-check/skill.md` を並行 BG 構成に更新 (stale な daemon/CronCreate 記述を除去)
  - [ ] 実 PR での E2E 検証: CodeRabbit Major ありの PR で、Claude Code が `pnpm create-pr` と `pnpm observe-pr` を並行 BG 起動し、observer の早期通知で Minor ヒアリングが走ることを確認
- **参照**:
  - ADR-018 追記 (2026-04-22) — observer モードと責務分離原則
  - ADR-022 — 「主フローは 100% 機械的 / 通知は read-only side effect」の境界

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
