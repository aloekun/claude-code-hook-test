# TODO

> **運用ルール**: 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。

---

## 現在進行中

### 1. pre-push-review の arch-review → simplicity-review 絞り込み

- **やろうとしたこと**: `pnpm push` のセルフレビューに時間がかかる問題 (ADR 1 本追加だけでも 5 分超) の解消。本来 push 時点では「コードのシンプルさ」を見たかったのに、現状は arch-review が architecture 全般を見ており、その重装備が遅さの主因になっている。別セッションで修正予定
- **現在地**: 実装済み (ADR-027)、実測検証のみ残り
  - [ ] 実測: 変更前後で `.takt/runs/*/meta.json` の duration を比較し、期待値 (5m → 2m) 通りか検証

### 2. マージ後フィードバックの定常化 (cli-merge-pipeline の post_steps 統合)

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

### 3. post-pr review フローの並行通知化 (観測用 BG タスク追加)

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
    → .claude/pr-monitor-state.json をポーリング
    → action=action_required を検出したら state 内容を stdout に出して exit
    → Claude Code が完了通知を受領 → ユーザーにレポート表示 + Minor ヒアリング
  ```

- **タスク分解**:
  - [ ] `scripts/observe-pr-state.ps1` (Windows 用) 新設
    - `.claude/pr-monitor-state.json` を 5-10s 間隔ポーリング
    - `action == "action_required"` または `action == "approved"` 検出で state 全文を出力して exit 0
    - 10 分のタイムアウト (ADR-016 準拠) 後は exit 1
    - `notified` フラグを見て、通知済みなら再通知しない
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

### 4. takt fix のレビュー修正コミット分離

- **やろうとしたこと**: CodeRabbit 指摘に対する takt 自動修正が元コミットに amend されるため、PR 上は commit 1 本に見える。結果:
  1. ユーザーがレビュー対応状況を追いにくい (「何度も未対応と誤認する」症状)
  2. 修正前後の比較が PR diff 上で取れない
  3. 「どの指摘にどの修正が対応したか」の辿り直しが git log に頼れない

  修正内容を別コミットに分離し、レビュー対応の可視性を上げる
- **現在地**: 設計確定、未実装
- **背景**:
  - 現状: takt fix は `@` を直接編集 → `cli-pr-monitor/src/stages/push.rs` の `run_push` が `jj new` してから push
  - `jj new` で child commit は作られるが、**fix 内容自体は元コミットに入ったまま**
  - ADR-022 により takt 側が commit message / bookmark を触ることは禁止。コミット分離は Rust 側の責務
- **実装内容**:
  - fix 実行前の commit ID を保持 (`pre_takt_commit_id` は既存)
  - takt 実行後、`@` の内容が変わっていれば (`decide_repush == HasChange`)、修正差分を**新しい子コミット**として分離する
  - 具体的な戦略候補:
    - **案 A (簡潔)**: `jj new` で child commit を作ってから fix 差分をそこに移す
    - **案 B (明示)**: `jj split` で元コミットから fix 差分だけを切り出して child にする
    - 案 A の方がシンプル。元コミットは不変、子コミットに `fix(review): ...` 相当の description を付けて push
- **タスク分解**:
  - [ ] `src/cli-pr-monitor/src/stages/push.rs` の `run_push` 調査 + 既存の `jj new` 動作確認 (どのタイミングで走るか)
  - [ ] コミット分離ロジック実装 (案 A ベース)
    - takt fix 後の `@` 差分 (`pre_takt_cid..post_takt_cid`) を検出
    - HasChange の場合のみ分離。NoChange (amend なし) は既存のスキップ動作
  - [ ] コミット description の生成方針
    - ADR-022 遵守: **takt は触らない**。Rust 側で固定文言または PR title 参照を使う
    - 候補: `fix(review): apply CodeRabbit fixes for #<PR番号>`
    - PR 番号は cli-pr-monitor が既に保持しているので流用可
  - [ ] unit テスト: `decide_repush` の分岐別でコミット構造が期待通りか
  - [ ] E2E 検証: CodeRabbit Major ありの PR で commit が 2 本 (original + fix) になることを確認
- **詰まっている箇所**:
  - **元コミット description の維持**: `jj new` 単独では元の description が保持される想定だが、過去の PR で `jj describe` による上書き事故があった (PR #44、ADR-022 の契機)。挙動を実測で再確認する必要あり
  - **複数回 fix される場合**: 1 PR で 2 回 3 回と CodeRabbit 指摘 → takt 修正が走った場合、毎回新しい fix コミットを作るか、同じ fix コミットに積むか要検討
- **考慮事項**:
  - コミット分離は `decide_repush == HasChange` のみ。NoChange (takt が実質変更なし) の場合は既存 no-op
  - `auto_push_severity = "none"` の場合は分離せずに手動 push を待つ (ユーザーが `jj describe` する余地を残す)
  - ADR-022 (automated actor boundary) の境界をまたがないこと。コミット分離のロジックは Rust 側に閉じる
- **参照 ADR**:
  - ADR-018 (post-pr-monitor takt 化): 既存フローの前提
  - ADR-022 (責務分離): takt は commit 操作禁止、Rust 側が担当
  - PR #44 の事故事例: 元 description の破壊で得た教訓

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
