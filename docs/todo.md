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

### 5. cli-pr-monitor の auto re-push に bookmark 自動前進を移植

- **やろうとしたこと**: takt 自動修正後の auto re-push で「修正コミットができても bookmark が動かず remote に届かない」問題を解消。cli-push-runner には PR #50 で `push_jj_bookmark.rs` の advance ロジックが入っているが、cli-pr-monitor の `run_push` は `jj new` → `jj git push` だけで bookmark を進めない
- **現在地**: port 完了 + 統合テストで機能等価を確認。あとは実 PR でのロールアウトのみ
  - [x] `src/cli-pr-monitor/src/stages/push_jj_bookmark.rs` 新設 (cli-push-runner から port、log prefix は `[action]`/`[state]` に調整、`lib_jj_helpers::is_trunk_bookmark` 再利用)
  - [x] `src/cli-pr-monitor/src/stages/push.rs:run_push` の `jj new` 後・push 前に `advance_jj_bookmarks` を挿入 (`push_command` が `jj ` で始まる場合のみ、失敗時はログして push 続行)
  - [x] unit テスト (dedup / parse_bookmarks_from_template / parse_bookmark_list_output / dispatch_bookmark_advance)
  - [x] 統合テスト `integration_advance_moves_bookmark_to_parent_after_jj_new` で実 jj を使い PR #53 症状の退行防止を確認 (push-runner-config の rust-test グループで自動実行される `#[ignore]` テスト、`--test-threads=1` 必須)
  - [ ] 実 PR での E2E 検証: 次回 CodeRabbit Major 指摘が出た PR で、takt 修正 → auto re-push で bookmark が remote 反映まで自動到達することを目視確認 (本 PR マージ後にリリース)
- **詰まっている箇所**:
  - **共通化方針**: まず port で機能等価を確認。将来 `lib-jj-helpers` へ集約する候補として `push_jj_bookmark.rs` 先頭に TODO コメントを残した (ADR-024)
  - **task 4 との順序**: fallback path (`jj bookmark list` ベース) が amend / split 両方に耐性があるため、task 4 を先送りしても問題なし
- **参照 ADR / PR**:
  - PR #50 (cli-push-runner の bookmark fallback)
  - ADR-024 (共通 jj helper、試験運用)
  - 関連: task 4 (コミット分離)

### 6. post-pr-review workflow の verdict に push 反映確認を追加

- **やろうとしたこと**: post-pr-review workflow が local file の状態だけを見て `approved` を判定する設計のため、auto re-push が失敗していても `approved` になる gap を埋める。PR #53 で「local 修正済み + bookmark 未前進 → workflow approved」の食い違いを実測
- **現在地**: 設計段階、未着手。task 5 (問題 A) の defense-in-depth 的位置付け
- **実装内容案**:
  - [ ] workflow の analyze (or 終了前) ステップで `gh api` を叩いて remote の最新 commit SHA を取得し、local の fix commit SHA と比較
  - [ ] 一致しなければ `verdict = action_required` にダウングレードし「remote 未反映」のメッセージを出す
- **詰まっている箇所**:
  - **task 5 実装後の優先度再評価**: task 5 が入れば「auto re-push が失敗しない限り approved は妥当」となるため、task 6 の優先度は大きく下がる。実装可否は task 5 完了後に判断
  - **takt workflow の YAML から外部コマンド呼び出しが可能か**: facets instruction 内でシェルが呼べるかは要調査 (ADR-019/020 の facets 設計内)
- **参照 ADR**:
  - ADR-018 (post-pr-monitor takt 化)
  - ADR-019 (CodeRabbit レビュー運用ハイブリッド)
  - 関連: task 5 (bookmark auto-advance)

### 7. prepare-pr skill の責務分離 (試験運用フィードバック)

- **やろうとしたこと**: 2026-04-19 に試験運用を始めた `prepare-pr` skill (ADR-028) を初利用したところ、1 スキル内に **draft 生成 (知的労働)** と **実行オーケストレーション (承認ゲート + 一時ファイル書き出し + create-pr + cleanup)** が同居しており、責務が散らかっている。セッション 2026-04-20 のユーザー指摘
- **現在地**: 設計段階、未着手。`prepare-pr` の試験運用データ (PR #61 が初回利用) が 1 件貯まった時点での再設計候補
- **再設計案**:
  - `propose-pr-draft` skill (新設): 純粋に jj log / diff から title / body の初稿を推論する副作用なし skill。再利用可能でテスト容易
  - `prepare-pr` skill (現行をオーケストレータ化): `propose-pr-draft` を呼び出して draft を得た後、AskUserQuestion ゲート → `pnpm prepare-pr-body` → `pnpm create-pr` → cleanup を回す薄い存在に縮退
- **分離しきれない部分 (確認済)**:
  - ADR-028 承認ゲートは create-pr 実行の直前で atomic に挟む必要があり、draft skill 側には移動できない
  - PR #51 由来の body 切り詰め対策 (`--body-file` 必須) は `pnpm prepare-pr-body` helper による一時ファイル経由が必須
  - cleanup は create-pr 成否に連動
- **タスク分解**:
  - [ ] `propose-pr-draft` skill を `.claude/skills/` に新設 (description に「PR draft 提案のみ、副作用なし」を明記)
  - [ ] `prepare-pr` skill の Step 1-3 を `propose-pr-draft` 呼び出しに置き換え
  - [ ] 試験運用期間 (2026-04-19〜、半年) のログを `.claude/skills/prepare-pr/SKILL.md` の ステータスセクションに反映
- **参照**:
  - ADR-028 (外部可視成果物の生成コマンドの実行ゲート)
  - PR #57 (permissions.ask + prepare-pr-body helper)
  - PR #61 (prepare-pr skill の初利用事例、責務分離の指摘源)

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
